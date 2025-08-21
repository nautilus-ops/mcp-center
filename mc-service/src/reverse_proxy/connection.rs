use crate::cache::mcp_servers::Cache;
use crate::reverse_proxy::{ProxyResponse, build_error_stream_response};
use axum::body::Body;
use axum::extract::Request;
use axum::response::Response;
use bytes::Bytes;
use http::{HeaderValue, StatusCode, Uri};
use http_body_util::{BodyExt, StreamBody};
use hyper::body::Frame;
use hyper_rustls::HttpsConnector;
use hyper_util::client::legacy::Client;
use hyper_util::client::legacy::connect::HttpConnector;
use once_cell::sync::Lazy;
use regex::Regex;
use std::convert::Infallible;
use std::pin::Pin;
use std::sync::Arc;
use std::task::Poll;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::ReceiverStream;
use tower_service::Service;

static REGEX_CONNECT_ROUTER: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^/proxy/connect/([^/]+)/([^/]+)(/.*)?$").unwrap());

static REGEX_MESSAGE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^(?P<path>[^?]+)\?sessionId=(?P<sid>[0-9a-fA-F\-]+)").unwrap());

const HEADER_HOST: &str = "Host";

#[derive(Clone)]
pub struct ConnectionService {
    client: Arc<Client<HttpsConnector<HttpConnector>, Body>>,
    cache: Arc<Cache>,
}

impl ConnectionService {
    pub(crate) fn new(
        client: Arc<Client<HttpsConnector<HttpConnector>, Body>>,
        cache: Arc<Cache>,
    ) -> Self {
        ConnectionService { client, cache }
    }
}

impl Service<Request<Body>> for ConnectionService {
    type Response = ProxyResponse;
    type Error = Infallible;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, _cx: &mut std::task::Context<'_>) -> Poll<Result<(), Self::Error>> {
        Ok(()).into()
    }

    fn call(&mut self, mut req: Request<Body>) -> Self::Future {
        let cache = self.cache.clone();
        let client = self.client.clone();

        Box::pin(async move {
            let (tx, rx) = tokio::sync::mpsc::channel::<Result<Frame<Bytes>, std::io::Error>>(100);
            let stream = ReceiverStream::new(rx);

            let path = req.uri().path();

            let (name, tag) = match parse_connection_router(path) {
                Ok(res) => (res.0, res.1),
                Err(err) => {
                    tracing::error!("Failed to parse connection_router {err}");
                    return Ok(build_error_stream_response(
                        tx,
                        stream,
                        "Failed to parse connection_router".to_string(),
                        StatusCode::INTERNAL_SERVER_ERROR,
                    ));
                }
            };

            let mcp_server = match cache.load_server_info(&name, &tag).await {
                Some(ep) => ep,
                None => {
                    tracing::error!("Failed to find server info for '{name}'");
                    return Ok(build_error_stream_response(
                        tx,
                        stream,
                        format!("Failed to load server info for {name} {tag}"),
                        StatusCode::INTERNAL_SERVER_ERROR,
                    ));
                }
            };

            *req.uri_mut() = match Uri::try_from(&mcp_server.endpoint) {
                Ok(uri) => uri,
                Err(err) => {
                    tracing::error!(
                        "Failed to convert endpoint to uri for {name} {tag}, error {err}"
                    );
                    return Ok(build_error_stream_response(
                        tx,
                        stream,
                        format!("Failed to convert endpoint to uri for {name} {tag}"),
                        StatusCode::INTERNAL_SERVER_ERROR,
                    ));
                }
            };

            if let Ok(host) = HeaderValue::from_str(mcp_server.host.as_str()) {
                req.headers_mut().insert(HEADER_HOST, host);
            };

            let response = client
                .request(req)
                .await
                .map_err(|err| {
                    println!("request error: {:?}", err);
                })
                .unwrap();

            let status_code = response.status();
            let headers = response.headers().clone();

            tokio::task::spawn(async move {
                let mut response_stream = response.into_data_stream();

                while let Some(chunk_result) = response_stream.next().await {
                    match chunk_result {
                        Ok(mut chunk) => {
                            let chunk_str = String::from_utf8_lossy(&chunk);
                            tracing::info!("chunk: {:?}", chunk_str);

                            if let Some((path, session_id)) = parse_message(chunk_str.as_ref()) {
                                tracing::info!(
                                    "connect mcp success sessionId={}, name={}, tag={}",
                                    session_id,
                                    &name,
                                    &tag
                                );

                                let proxy_message_path =
                                    build_proxy_message_path(&name, &tag, &path, &session_id);

                                let mut proxy_body = String::from("event: endpoint\ndata: ");
                                proxy_body.push_str(proxy_message_path.as_str());
                                proxy_body.push_str("\r\n\r\n");

                                chunk = Bytes::from(proxy_body);
                            }

                            if let Err(e) = tx.send(Ok(Frame::data(chunk))).await {
                                tracing::warn!("connection closed: {:?}", e);
                                break;
                            }
                        }
                        Err(e) => {
                            tracing::error!("connection error: {:?}", e);
                            let _ = tx.send(Err(std::io::Error::other(e))).await;
                            break;
                        }
                    }
                }

                let _ = tx.send(Ok(Frame::trailers(http::HeaderMap::new()))).await;
            });

            let mut response_builder = Response::builder().status(status_code);

            for (name, value) in &headers {
                if name.as_str() != "content-length" && name.as_str() != "transfer-encoding" {
                    response_builder = response_builder.header(name, value);
                }
            }

            response_builder = response_builder.header("transfer-encoding", "chunked");

            response_builder = response_builder.header("connection", "keep-alive");

            Ok(response_builder.body(StreamBody::new(stream)).unwrap())
        })
    }
}

// parse {name} {tag} from uri
pub fn parse_connection_router(uri: &str) -> Result<(String, String), String> {
    match REGEX_CONNECT_ROUTER.captures(uri) {
        None => {
            tracing::error!("Can't parse [connection] uri {}", uri);
            Err(format!("Can't parse [connection] uri {}", uri))
        }
        Some(caps) => Ok((caps[1].to_string(), caps[2].to_string())),
    }
}

fn parse_message(input: &str) -> Option<(String, String)> {
    let uri = if let Some(line) = input.lines().find(|l| l.trim_start().starts_with("data:")) {
        line.trim_start_matches("data:").trim()
    } else {
        input.trim()
    };

    REGEX_MESSAGE
        .captures(uri)
        .map(|caps| (caps["path"].to_string(), caps["sid"].to_string()))
}

fn build_proxy_message_path(name: &str, tag: &str, message_path: &str, session_id: &str) -> String {
    // build to /message/{name}/{tag}/{raw_message_path}?sessionId={session_id}
    let raw_message_path = message_path.trim_start_matches("/");

    let mut proxy_message_path = String::from("/proxy/message/");
    proxy_message_path.push_str(name);
    proxy_message_path.push('/');
    proxy_message_path.push_str(tag);
    proxy_message_path.push('/');
    proxy_message_path.push_str(raw_message_path);
    proxy_message_path.push_str("?sessionId=");
    proxy_message_path.push_str(session_id);

    proxy_message_path
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_parse_connection_router() {
        struct TestCase {
            uri: &'static str,
            want: Result<(String, String), String>,
        }

        let tests = vec![
            TestCase {
                uri: "/proxy/connect/mcp-test/1.0.0",
                want: Ok(("mcp-test".to_string(), "1.0.0".to_string())),
            },
            TestCase {
                uri: "/proxy/connect/another-app/2.3.4",
                want: Ok(("another-app".to_string(), "2.3.4".to_string())),
            },
            TestCase {
                uri: "/proxy/connect/mcp-test",
                want: Err("Can't parse [connection] uri /proxy/connect/mcp-test".to_string()),
            },
            TestCase {
                uri: "/wrong/xxx/yyy",
                want: Err("Can't parse [connection] uri /wrong/xxx/yyy".to_string()),
            },
        ];

        for t in tests {
            let got = parse_connection_router(t.uri);
            match (&got, &t.want) {
                (Ok(g), Ok(w)) => assert_eq!(g, w, "uri: {}", t.uri),
                (Err(e), Err(w)) => assert_eq!(e, w, "uri: {}", t.uri),
                _ => panic!("uri: {} => expected {:?}, got {:?}", t.uri, t.want, got),
            }
        }
    }

    #[test]
    fn test_parse_message() {
        struct TestCase {
            input: &'static str,
            want: Option<(String, String)>,
        }

        let tests = vec![
            TestCase {
                input: "event: endpoint\ndata: /message?sessionId=36f34c7e-ec0c-4f6d-8451-38b4488ff4e4\r\n\r\n",
                want: Some((
                    "/message".to_string(),
                    "36f34c7e-ec0c-4f6d-8451-38b4488ff4e4".to_string(),
                )),
            },
            TestCase {
                input: "data: /msg?sessionId=36f34c7e-ec0c-4f6d-8451-38b4488ff4e4\r\n",
                want: Some((
                    "/msg".to_string(),
                    "36f34c7e-ec0c-4f6d-8451-38b4488ff4e4".to_string(),
                )),
            },
            TestCase {
                input: "/message?sessionId=36f34c7e-ec0c-4f6d-8451-38b4488ff4e4",
                want: Some((
                    "/message".to_string(),
                    "36f34c7e-ec0c-4f6d-8451-38b4488ff4e4".to_string(),
                )),
            },
            TestCase {
                input: "data: /message/no-session-id\r\n",
                want: None,
            },
            TestCase {
                input: "",
                want: None,
            },
        ];

        for t in tests {
            let got = parse_message(t.input);
            assert_eq!(got, t.want, "input: {:?}", t.input);
        }
    }

    #[test]
    fn test_build_proxy_message_path() {
        struct TestCase {
            name: &'static str,
            tag: &'static str,
            message_path: &'static str,
            session_id: &'static str,
            want: &'static str,
        }

        let tests = vec![
            TestCase {
                name: "mcp-test",
                tag: "1.0.0",
                message_path: "/api/v1/message",
                session_id: "36f34c7e-ec0c-4f6d-8451-38b4488ff4e4",
                want: "/proxy/message/mcp-test/1.0.0/api/v1/message?sessionId=36f34c7e-ec0c-4f6d-8451-38b4488ff4e4",
            },
            TestCase {
                name: "service",
                tag: "v2",
                message_path: "path/to/msg",
                session_id: "36f34c7e-ec0c-4f6d-8451-38b4488ff4e4",
                want: "/proxy/message/service/v2/path/to/msg?sessionId=36f34c7e-ec0c-4f6d-8451-38b4488ff4e4",
            },
            TestCase {
                name: "test",
                tag: "0.1",
                message_path: "/",
                session_id: "36f34c7e-ec0c-4f6d-8451-38b4488ff4e4",
                want: "/proxy/message/test/0.1/?sessionId=36f34c7e-ec0c-4f6d-8451-38b4488ff4e4",
            },
        ];

        for t in tests {
            let got = build_proxy_message_path(t.name, t.tag, t.message_path, t.session_id);
            assert_eq!(
                got, t.want,
                "name: {}, tag: {}, message_path: {}",
                t.name, t.tag, t.message_path
            );
        }
    }
}

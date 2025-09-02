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
use mc_common::app::cache::{Cache, McpServerInfo};

static REGEX_MESSAGE_ROUTER: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^/proxy/message/([^/]+)/([^/]+)(/.*)?$").unwrap());

#[derive(Clone)]
pub struct MessageService {
    client: Arc<Client<HttpsConnector<HttpConnector>, Body>>,
    cache: Arc<Cache>,
}

impl MessageService {
    pub fn new(
        client: Arc<Client<HttpsConnector<HttpConnector>, Body>>,
        cache: Arc<Cache>,
    ) -> Self {
        Self { client, cache }
    }
}

impl Service<Request<Body>> for MessageService {
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
            let path_query = req.uri().query();

            tracing::info!("path ===> {path}");
            // tracing::info!("path_query ===> {path_query}");

            let (name, tag, sub_path) = match parse_message_router(path) {
                Ok(res) => res,
                Err(err) => {
                    tracing::error!(error = ?err, "parse message router failed {path}");
                    return Ok(build_error_stream_response(
                        tx,
                        stream,
                        format!("Failed to parse message router {path}"),
                        StatusCode::INTERNAL_SERVER_ERROR,
                    ));
                }
            };

            let mcp_server = match cache.load_server_info(&name, &tag).await {
                None => {
                    tracing::error!("Failed to find server info for '{name}'");
                    return Ok(build_error_stream_response(
                        tx,
                        stream,
                        format!("Failed to load server info for {name} {tag}"),
                        StatusCode::INTERNAL_SERVER_ERROR,
                    ));
                }
                Some(ep) => ep,
            };

            *req.uri_mut() =
                match Uri::try_from(build_raw_message_path(&mcp_server, &sub_path, path_query)) {
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
                req.headers_mut().insert("host", host);
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
                        Ok(chunk) => {
                            let chunk_str = String::from_utf8_lossy(&chunk);
                            tracing::info!("chunk: {:?}", chunk_str);

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

fn build_raw_message_path(
    mcp_server: &McpServerInfo,
    sub_path: &str,
    path_query: Option<&str>,
) -> String {
    let sub_path = sub_path.trim_matches('/');
    let mut message_path = format!(
        "{}://{}/{}",
        mcp_server.scheme.as_str(),
        mcp_server.host,
        sub_path,
    );

    if let Some(query) = path_query {
        message_path = format!("{message_path}?{query}");
    }

    message_path
}

//
pub fn parse_message_router(uri: &str) -> Result<(String, String, String), String> {
    if let Some(caps) = REGEX_MESSAGE_ROUTER.captures(uri) {
        let name = caps.get(1).unwrap().as_str().to_string();
        let tag = caps.get(2).unwrap().as_str().to_string();
        let sub_path = caps
            .get(3)
            .map_or("".to_string(), |m| m.as_str().to_string());
        return Ok((name, tag, sub_path));
    }

    tracing::error!("Can't parse [message] uri {}", uri);
    Err(format!("Can't parse [message] uri {}", uri))
}

#[cfg(test)]
mod tests {
    use super::*;
    use mc_common::types::HttpScheme;
    #[test]
    fn test_parse_message_router() {
        struct TestCase {
            uri: &'static str,
            want: Result<(String, String, String), String>,
        }

        let tests = vec![
            TestCase {
                uri: "/proxy/message/mcp-test/1.0.0/api/sse/message",
                want: Ok((
                    "mcp-test".to_string(),
                    "1.0.0".to_string(),
                    "/api/sse/message".to_string(),
                )),
            },
            TestCase {
                uri: "/proxy/message/mcp-test/1.0.0/message",
                want: Ok((
                    "mcp-test".to_string(),
                    "1.0.0".to_string(),
                    "/message".to_string(),
                )),
            },
            TestCase {
                uri: "/invalid/uri",
                want: Err("Can't parse [message] uri /invalid/uri".to_string()),
            },
        ];

        for t in tests {
            let result = parse_message_router(t.uri);
            assert_eq!(result, t.want, "uri: {}", t.uri);
        }
    }

    #[test]
    fn test_build_raw_message_path() {
        let https_server = McpServerInfo {
            endpoint: "".to_string(),
            scheme: HttpScheme::Https,
            host: "example.com".to_string(),
            port: "".to_string(),
            path: "".to_string(),
        };

        struct TestCase {
            server: McpServerInfo,
            sub_path: &'static str,
            path_query: Option<&'static str>,
            want: &'static str,
        }

        let tests = vec![
            TestCase {
                server: https_server.clone(),
                sub_path: "api/v1/message",
                path_query: None,
                want: "https://example.com/api/v1/message",
            },
            TestCase {
                server: https_server.clone(),
                sub_path: "/api/v1/message/",
                path_query: None,
                want: "https://example.com/api/v1/message",
            },
            TestCase {
                server: https_server.clone(),
                sub_path: "api/v1/message",
                path_query: Some("foo=bar"),
                want: "https://example.com/api/v1/message?foo=bar",
            },
            TestCase {
                server: https_server.clone(),
                sub_path: "/api/v1/message/",
                path_query: Some("k=v&x=1"),
                want: "https://example.com/api/v1/message?k=v&x=1",
            },
        ];

        for t in tests {
            let got = build_raw_message_path(&t.server, t.sub_path, t.path_query);
            assert_eq!(
                got, t.want,
                "sub_path: {}, query: {:?}",
                t.sub_path, t.path_query
            );
        }
    }
}

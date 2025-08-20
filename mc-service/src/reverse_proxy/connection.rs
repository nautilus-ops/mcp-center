use crate::cache::mcp_servers::Cache;
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
use mc_common::types::HttpScheme;
use once_cell::sync::Lazy;
use pingora_core::InternalError;
use regex::Regex;
use std::convert::Infallible;
use std::error::Error;
use std::pin::Pin;
use std::str::FromStr;
use std::sync::Arc;
use std::task::Poll;
use http::uri::InvalidUri;
use tokio::sync::mpsc::Sender;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::ReceiverStream;
use tower_service::Service;

static REGEX_ENDPOINT: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^(?P<scheme>https?)://(?P<host>[^/:]+)(?::(?P<port>\d+))?(?P<path>/.*)?$").unwrap()
});

static REGEX_PROXY_URI: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^/connect/([^/]+)/([^/]+)(/.*)?$").unwrap());

const HEADER_HOST: &str = "Host";

type ProxyResponse = Response<StreamBody<ReceiverStream<Result<Frame<Bytes>, std::io::Error>>>>;

#[derive(Debug, Clone)]
pub struct ParsedEndpoint {
    pub(crate) endpoint: String,
    pub(crate) host: String,
    pub(crate) port: String,
    pub(crate) path: String,
    pub(crate) scheme: HttpScheme,
}

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
            // let path_query = req
            //     .uri()
            //     .path_and_query()
            //     .map(|v| v.as_str())
            //     .unwrap_or(path);

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

            let endpoint = match cache.load_server_info(&name, &tag).await {
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

            let parsed = match parse_endpoint(&endpoint) {
                Ok(p) => {p}
                Err(err) => {
                    tracing::error!("Failed to parse endpoint for {name} {tag}, error {err}");
                    return Ok(build_error_stream_response(
                        tx,
                        stream,
                        format!("Failed to parse endpoint for {name} {tag}"),
                        StatusCode::INTERNAL_SERVER_ERROR,
                    ));
                }
            };

            *req.uri_mut() = match Uri::try_from(&endpoint) {
                Ok(uri) => uri,
                Err(err) => {
                    tracing::error!("Failed to convert endpoint to uri for {name} {tag}, error {err}");
                    return Ok(build_error_stream_response(
                        tx,
                        stream,
                        format!("Failed to convert endpoint to uri for {name} {tag}"),
                        StatusCode::INTERNAL_SERVER_ERROR,
                    ));
                }
            };

            if let Ok(host) = HeaderValue::from_str(parsed.host.as_str()) {
                req.headers_mut().insert(HEADER_HOST, host);
            };
            req.headers_mut().insert("Authorization", HeaderValue::from_static("230d11a2-b53a-431a-b924-e786b715f50e"));

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

                            if let Err(e) = tx.send(Ok(Frame::data(Bytes::from(chunk)))).await {
                                tracing::warn!("connection closed: {:?}", e);
                                break;
                            }
                        }
                        Err(e) => {
                            tracing::error!("connection error: {:?}", e);
                            let _ = tx
                                .send(Err(std::io::Error::new(std::io::ErrorKind::Other, e)))
                                .await;
                            break;
                        }
                    }
                }

                let _ = tx.send(Ok(Frame::trailers(http::HeaderMap::new()))).await;
            });

            let mut response_builder = Response::builder().status(status_code);

            for (name, value) in &headers {
                if name != "content-length" && name != "transfer-encoding" {
                    response_builder = response_builder.header(name, value);
                }
            }

            response_builder = response_builder.header("transfer-encoding", "chunked");

            response_builder = response_builder.header("connection", "keep-alive");

            Ok(response_builder.body(StreamBody::new(stream)).unwrap())
        })
    }
}

fn parse_endpoint(endpoint: &str) -> Result<crate::proxy::ParsedEndpoint, Box<dyn Error>> {
    if let Some(caps) = REGEX_ENDPOINT.captures(endpoint) {
        let scheme = caps.name("scheme").map(|m| m.as_str()).unwrap_or("");
        let host = caps.name("host").map(|m| m.as_str()).unwrap_or("");
        let port = match caps.name("port") {
            None => match scheme {
                "http" => "80",
                "https" => "443",
                _ => {
                    return Err("Unsupported scheme".into());
                }
            },
            Some(p) => p.as_str(),
        };
        let path = caps.name("path").map(|m| m.as_str()).unwrap_or("/");
        Ok(crate::proxy::ParsedEndpoint {
            endpoint: endpoint.to_string(),
            host: host.to_string(),
            port: port.to_string(),
            path: path.to_string(),
            scheme: HttpScheme::from_str(scheme)?,
        })
    } else {
        Err(format!("Failed to parse endpoint {endpoint}").into())
    }
}

// parse {name} {tag} from uri
pub fn parse_connection_router(uri: &str) -> pingora_core::Result<(String, String)> {
    match REGEX_PROXY_URI.captures(uri) {
        None => {
            tracing::error!("Can't parse [connection] uri {}", uri);
            Err(pingora_core::Error::new(InternalError))
        }
        Some(caps) => Ok((caps[1].to_string(), caps[2].to_string())),
    }
}

pub fn build_error_stream_response(
    tx: Sender<Result<Frame<Bytes>, std::io::Error>>,
    stream: ReceiverStream<Result<Frame<Bytes>, std::io::Error>>,
    msg: String,
    status: StatusCode,
) -> ProxyResponse {
    tokio::task::spawn(async move {
        tx.send(Ok(Frame::data(Bytes::from(msg)))).await.unwrap();
    });

    let mut response_builder = Response::builder();
    response_builder = response_builder.status(status);
    response_builder.body(StreamBody::new(stream)).unwrap()
}

use crate::config::AppConfig;
use crate::router::{Matcher, Router};
use mc_common::types::HttpScheme;
use mc_register::Registry;

use async_trait::async_trait;
use bytes::Bytes;
use http::Uri;
use pingora_core::prelude::HttpPeer;
use pingora_core::{ErrorType, InternalError};
use pingora_http::{RequestHeader, ResponseHeader};
use pingora_load_balancing::prelude::RoundRobin;
use pingora_load_balancing::{Backend, LoadBalancer};
use pingora_proxy::{ProxyHttp, Session};
use regex::Regex;
use std::collections::HashMap;
use std::error::Error;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use tokio::runtime::Runtime;
use tokio::sync::RwLock;
use tokio::time::interval;

struct McpServerInfo {
    lb: LoadBalancer<RoundRobin>,
    pub endpoint: String,
}

pub struct ProxyService {
    handle: Arc<Box<dyn Registry>>,
    runtime: Arc<Runtime>,
    router_matcher: Arc<Matcher>,
    // mcp-name -> tag(version) -> load-balancer
    server_cache: Arc<RwLock<HashMap<String, HashMap<String, McpServerInfo>>>>,
}

impl ProxyService {
    pub fn new(handle: Box<dyn Registry>, runtime: Arc<Runtime>, config: AppConfig) -> Self {
        let service = Self {
            handle: Arc::new(handle),
            runtime,
            router_matcher: Arc::new(Matcher::new()),
            server_cache: Arc::new(RwLock::new(HashMap::new())),
        };

        service.async_cache(config.mcp_center.cache_reflash_interval);

        service
    }

    pub fn async_cache(&self, cache_interval: u64) {
        let cache = self.server_cache.clone();
        let handle = self.handle.clone();
        self.runtime.spawn(async move {
            let mut ticker = interval(Duration::from_secs(cache_interval));
            loop {
                ticker.tick().await;

                let mcp_servers = match handle.list_mcp().await {
                    Ok(results) => results,
                    Err(err) => {
                        tracing::error!("Can't list mcp servers, error: {}", err);
                        continue;
                    }
                };

                let mut mcps = cache.write().await;

                mcp_servers.iter().for_each(|server| {
                    let mut tag = match &server.version {
                        None => "default".to_string(),
                        Some(version) => version.clone(),
                    };

                    if let Some(t) = &server.tag {
                        tag = t.clone();
                    }

                    let mut item = HashMap::<String, McpServerInfo>::new();
                    let result = match parse_endpoint(server.endpoint.as_str()) {
                        Ok(result) => result,
                        Err(err) => {
                            tracing::error!(
                                "Can't parse endpoint {}, error: {}",
                                server.endpoint,
                                err
                            );
                            return;
                        }
                    };

                    let ep = format!("{}:{}", result.host, result.port);

                    // set this tag
                    let upstream = match LoadBalancer::try_from_iter([ep.clone()]) {
                        Ok(result) => result,
                        Err(err) => {
                            tracing::error!(
                                "Can't create load balancer endpoint: {}, error: {}",
                                ep,
                                err
                            );
                            return;
                        }
                    };

                    item.insert(
                        tag.clone(),
                        McpServerInfo {
                            lb: upstream,
                            endpoint: server.endpoint.clone(),
                        },
                    );
                    mcps.insert(server.name.clone(), item);

                    tracing::info!(
                        "Load mcp server {}/{} success, endpoint: {}",
                        server.name,
                        tag,
                        ep
                    );
                });

                // TODO delete ---------test----------
                let upstream = match LoadBalancer::try_from_iter(["127.0.0.1:3000"]) {
                    Ok(result) => result,
                    Err(err) => {
                        tracing::error!(
                            "Can't create load balancer endpoint: {}, error: {}",
                            "127.0.0.1:3000",
                            err
                        );
                        return;
                    }
                };

                let mut tmp = HashMap::new();
                tmp.insert(
                    "default".to_owned(),
                    McpServerInfo {
                        lb: upstream,
                        endpoint: "http://127.0.0.1:3000/mcp".to_string(),
                    },
                );
                mcps.insert("local".to_string(), tmp);
                // TODO delete --------------------
            }
        });
    }

    async fn load_server_info(&self, mcp_name: &str, tag: &str) -> Option<(Backend, String)> {
        let cache = self.server_cache.read().await;
        if let Some(tags) = cache.get(mcp_name) {
            if let Some(info) = tags.get(tag) {
                return info
                    .lb
                    .select(b"", 256)
                    .map(|backend| (backend, info.endpoint.clone()));
            }
        }
        None
    }

    pub async fn build_context_from_connection(
        &self,
        name: &str,
        tag: &str,
        ctx: &mut ProxyContext,
    ) -> pingora_core::Result<()> {
        let (backend, parsed) = self.load_mcp_info_from_cache(name, tag).await?;

        ctx.endpoint = parsed.endpoint.clone();
        ctx.connect_path = parsed.path.clone();
        ctx.host = parsed.host.clone();
        ctx.scheme = parsed.scheme;
        ctx.port = parsed.port.clone();
        ctx.backend = Some(backend);
        ctx.name = name.to_string();
        ctx.tag = tag.to_string();
        Ok(())
    }

    pub async fn build_context_from_message(
        &self,
        name: &str,
        tag: &str,
        message_path: &str,
        ctx: &mut ProxyContext,
    ) -> pingora_core::Result<()> {
        let (backend, parsed) = self.load_mcp_info_from_cache(name, tag).await?;

        ctx.endpoint = parsed.endpoint.clone();
        ctx.connect_path = parsed.path.clone();
        ctx.host = parsed.host.clone();
        ctx.scheme = parsed.scheme;
        ctx.port = parsed.port.clone();
        ctx.backend = Some(backend);
        ctx.name = name.to_string();
        ctx.tag = tag.to_string();
        ctx.message_path = message_path.to_string();
        Ok(())
    }

    async fn load_mcp_info_from_cache(
        &self,
        mcp_name: &str,
        tag: &str,
    ) -> pingora_core::Result<(Backend, ParsedEndpoint)> {
        let (backend, endpoint) = match self.load_server_info(mcp_name, tag).await {
            Some(info) => info,
            None => {
                tracing::error!("Can't load server {mcp_name}, tag {tag}");
                return Err(pingora_core::Error::explain(
                    ErrorType::HTTPStatus(404),
                    format!("Can't load server {mcp_name}, tag {tag}"),
                ));
            }
        };

        let parsed = match parse_endpoint(endpoint.as_str()) {
            Ok(result) => result,
            Err(err) => {
                tracing::error!("Can't parse endpoint {endpoint}, error: {err}");
                return Err(pingora_core::Error::explain(
                    InternalError,
                    format!("Can't parse endpoint {endpoint}, error: {err}"),
                ));
            }
        };
        Ok((backend, parsed))
    }
}
#[async_trait]
impl ProxyHttp for ProxyService {
    type CTX = ProxyContext;

    fn new_ctx(&self) -> Self::CTX {
        ProxyContext::new()
    }

    async fn upstream_peer(
        &self,
        _session: &mut Session,
        ctx: &mut Self::CTX,
    ) -> pingora_core::Result<Box<HttpPeer>> {
        if ctx.backend.is_none() {
            tracing::error!("Can't get backend for upstream peer");
            return Err(pingora_core::Error::explain(
                InternalError,
                "upstream backend not set",
            ));
        }

        let backend = ctx.backend.as_ref().unwrap();

        let mut proxy = HttpPeer::new(backend.clone(), false, String::from(""));

        if ctx.scheme.is_https() {
            proxy = HttpPeer::new(backend.clone(), true, ctx.host.clone());
        }
        Ok(Box::from(proxy))
    }

    async fn request_filter(
        &self,
        session: &mut Session,
        ctx: &mut Self::CTX,
    ) -> pingora_core::Result<bool>
    where
        Self::CTX: Send + Sync,
    {
        tracing::info!(
            "Request filters method: {} uri： {}",
            session.req_header().method,
            session.req_header().uri
        );

        let router = self
            .router_matcher
            .matching(session.req_header().uri.to_string())?;

        match router {
            Router::ConnectRouter { name, tag } => {
                self.build_context_from_connection(name.as_str(), tag.as_str(), ctx)
                    .await?;
                Ok(false)
            }
            Router::MessageRouter {
                name,
                tag,
                message_path,
                ..
            } => {
                self.build_context_from_message(
                    name.as_str(),
                    tag.as_str(),
                    message_path.as_str(),
                    ctx,
                )
                .await?;
                Ok(false)
            }
        }
    }

    async fn upstream_request_filter(
        &self,
        session: &mut Session,
        upstream_request: &mut RequestHeader,
        ctx: &mut Self::CTX,
    ) -> pingora_core::Result<()>
    where
        Self::CTX: Send + Sync,
    {
        if ctx.scheme.is_https() {
            upstream_request.insert_header("Host", ctx.host.clone())?;
        }

        if let Some((_, session_id)) =
            parse_message(session.req_header().uri.to_string().as_str())
        {
            let mut message_uri = ctx.message_path.clone();
            message_uri.push_str("?sessionId=");
            message_uri.push_str(session_id.as_str());

            let uri = Uri::from_str(message_uri.as_str()).unwrap();

            upstream_request.set_uri(uri);
            return Ok(());
        }

        let uri = Uri::from_str(ctx.connect_path.as_str()).unwrap();
        upstream_request.set_uri(uri);
        Ok(())
    }

    async fn response_filter(
        &self,
        _session: &mut Session,
        upstream_response: &mut ResponseHeader,
        _ctx: &mut Self::CTX,
    ) -> pingora_core::Result<()>
    where
        Self::CTX: Send + Sync,
    {
        tracing::info!("response_filter {:?}", upstream_response);
        Ok(())
    }

    fn upstream_response_body_filter(
        &self,
        session: &mut Session,
        body: &mut Option<Bytes>,
        _end_of_stream: bool,
        ctx: &mut Self::CTX,
    ) -> pingora_core::Result<()> {
        if let Some(body_bytes) = body.clone() {
            let content = String::from_utf8_lossy(body_bytes.as_ref());
            tracing::info!("upstream_response_body_filter {:?}", content);

            let path = session.req_header().uri.path();
            if path.starts_with("/connect") || path == "/" {
                if let Some((path, session_id)) = parse_message(content.to_string().as_str()) {
                    tracing::info!(
                        "connect mcp success sessionId={}, name={}, tag={}",
                        session_id,
                        ctx.name,
                        ctx.tag
                    );
                    ctx.message_path = path;

                    let proxy_message_path = build_proxy_message_path(ctx, session_id.as_str());
                    let mut proxy_body = String::from("event: endpoint\ndata: ");
                    proxy_body.push_str(proxy_message_path.as_str());
                    proxy_body.push_str("\r\n\r\n");

                    *body = Some(Bytes::from(proxy_body));
                }
            }
        }
        Ok(())
    }
}

pub struct ProxyContext {
    scheme: HttpScheme,
    endpoint: String,
    host: String,
    port: String,
    connect_path: String,
    backend: Option<Backend>,
    name: String,
    tag: String,
    message_path: String,
}

impl ProxyContext {
    pub fn new() -> Self {
        Self {
            scheme: HttpScheme::Http,
            endpoint: String::from(""),
            host: String::from(""),
            port: String::from(""),
            connect_path: String::from("/"),
            backend: None,
            name: String::from(""),
            tag: String::from(""),
            message_path: String::from("/"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ParsedEndpoint {
    pub(crate) endpoint: String,
    pub(crate) host: String,
    pub(crate) port: String,
    pub(crate) path: String,
    pub(crate) scheme: HttpScheme,
}

fn parse_endpoint(endpoint: &str) -> Result<ParsedEndpoint, Box<dyn Error>> {
    let re =
        Regex::new(r"^(?P<scheme>https?)://(?P<host>[^/:]+)(?::(?P<port>\d+))?(?P<path>/.*)?$")
            .unwrap();

    if let Some(caps) = re.captures(endpoint) {
        let scheme = caps.name("scheme").map(|m| m.as_str()).unwrap_or("");
        let host = caps.name("host").map(|m| m.as_str()).unwrap_or("");
        let port = match caps.name("port") {
            Some(p) => p.as_str(),
            None => match scheme {
                "http" => "80",
                "https" => "443",
                _ => {
                    return Err("Unsupported scheme".into());
                }
            },
        };
        let path = caps.name("path").map(|m| m.as_str()).unwrap_or("/");
        Ok(ParsedEndpoint {
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

fn parse_message(input: &str) -> Option<(String, String)> {
    let uri = if let Some(line) = input.lines().find(|l| l.trim_start().starts_with("data:")) {
        line.trim_start_matches("data:").trim()
    } else {
        input.trim()
    };

    let re = Regex::new(r"^(?P<path>[^?]+)\?sessionId=(?P<sid>[0-9a-fA-F\-]+)").unwrap();

    re.captures(uri)
        .map(|caps| (caps["path"].to_string(), caps["sid"].to_string()))
}

fn build_proxy_message_path(ctx: &mut ProxyContext, session_id: &str) -> String {
    let raw_message_path = ctx.message_path.trim_start_matches("/");

    let mut proxy_message_path = String::from("/message/");
    proxy_message_path.push_str(ctx.name.as_str());
    proxy_message_path.push('/');
    proxy_message_path.push_str(ctx.tag.as_str());
    proxy_message_path.push('/');
    proxy_message_path.push_str(raw_message_path);
    proxy_message_path.push_str("?sessionId=");
    proxy_message_path.push_str(session_id);

    proxy_message_path
}

#[cfg(test)]
mod test {
    use crate::proxy::{ProxyContext, build_proxy_message_path, parse_message};
    use mc_common::types::HttpScheme;

    #[test]
    fn test_parse_message() {
        #[derive(Debug)]
        struct TestCase {
            input: &'static str,
            path_want: &'static str,
            id_want: &'static str,
        }
        let tests = vec![
            TestCase {
                input: "/message?sessionId=49b420bb-adc1-4231-917a-08822da1e8f3",
                path_want: "/message",
                id_want: "49b420bb-adc1-4231-917a-08822da1e8f3",
            },
            TestCase {
                input: "/sse/message?sessionId=49b420bb-adc1-4231-917a-08822da1e8f3",
                path_want: "/sse/message",
                id_want: "49b420bb-adc1-4231-917a-08822da1e8f3",
            },
            TestCase {
                input: "/api/sse/message?sessionId=49b420bb-adc1-4231-917a-08822da1e8f3",
                path_want: "/api/sse/message",
                id_want: "49b420bb-adc1-4231-917a-08822da1e8f3",
            },
            TestCase {
                input: "event: endpoint\ndata: /message?sessionId=2e029713-f2e5-41db-bdb7-a9255efaa586\r\n\r\n",
                path_want: "/message",
                id_want: "2e029713-f2e5-41db-bdb7-a9255efaa586",
            },
            TestCase {
                input: "event: endpoint\ndata: /api/message?sessionId=2e029713-f2e5-41db-bdb7-a9255efaa586\r\n\r\n",
                path_want: "/api/message",
                id_want: "2e029713-f2e5-41db-bdb7-a9255efaa586",
            },
            TestCase {
                input: "event: endpoint\ndata: /api/sse/message?sessionId=2e029713-f2e5-41db-bdb7-a9255efaa586\r\n\r\n",
                path_want: "/api/sse/message",
                id_want: "2e029713-f2e5-41db-bdb7-a9255efaa586",
            },
        ];

        for t in tests {
            match parse_message(t.input) {
                None => {
                    panic!("Failed to parse sessionId and path for input: {}", t.input);
                }
                Some((path, id)) => {
                    assert_eq!(
                        path, t.path_want,
                        "message_path mismatch: got {}, want {}",
                        path, t.path_want
                    );
                    assert_eq!(
                        id, t.id_want,
                        "session_id mismatch: got {}, want {}",
                        id, t.id_want
                    );
                }
            }
        }
    }

    #[test]
    fn test_build_proxy_message_path() {
        let tests = vec![
            (
                ProxyContext {
                    scheme: HttpScheme::Http,
                    endpoint: "".to_string(),
                    host: "".to_string(),
                    port: "".to_string(),
                    connect_path: "".to_string(),
                    backend: None,
                    name: "fetch".to_string(),
                    tag: "1.0.0".to_string(),
                    message_path: "/message".to_string(),
                },
                String::from("49b420bb-adc1-4231-917a-08822da1e8f3"),
                String::from(
                    "/message/fetch/1.0.0/message?sessionId=49b420bb-adc1-4231-917a-08822da1e8f3",
                ),
            ),
            (
                ProxyContext {
                    scheme: HttpScheme::Http,
                    endpoint: "".to_string(),
                    host: "".to_string(),
                    port: "".to_string(),
                    connect_path: "".to_string(),
                    backend: None,
                    name: "fetch".to_string(),
                    tag: "1.0.0".to_string(),
                    message_path: "/sse/message".to_string(),
                },
                String::from("49b420bb-adc1-4231-917a-08822da1e8f3"),
                String::from(
                    "/message/fetch/1.0.0/sse/message?sessionId=49b420bb-adc1-4231-917a-08822da1e8f3",
                ),
            ),
            (
                ProxyContext {
                    scheme: HttpScheme::Http,
                    endpoint: "".to_string(),
                    host: "".to_string(),
                    port: "".to_string(),
                    connect_path: "".to_string(),
                    backend: None,
                    name: "fetch".to_string(),
                    tag: "1.0.0".to_string(),
                    message_path: "api/sse/message".to_string(),
                },
                String::from("49b420bb-adc1-4231-917a-08822da1e8f3"),
                String::from(
                    "/message/fetch/1.0.0/api/sse/message?sessionId=49b420bb-adc1-4231-917a-08822da1e8f3",
                ),
            ),
        ];

        for (mut ctx, session_id, want) in tests {
            let proxy_message_path = build_proxy_message_path(&mut ctx, &session_id);
            assert_eq!(
                want, proxy_message_path,
                "message_path mismatch: got {}, want {}",
                proxy_message_path, want
            );
        }
    }
}

use crate::service::config::AppConfig;
use crate::service::register::ListHandler;
use crate::service::session;
use crate::service::session::SessionInfo;
use crate::service::session::manager::LocalManager;
use async_trait::async_trait;
use bytes::Bytes;
use http::{StatusCode, Uri};
use pingora_core::prelude::HttpPeer;
use pingora_core::protocols::http::ServerSession as HttpSession;
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

pub(crate) struct ProxyService {
    handle: Arc<Box<dyn ListHandler>>,
    runtime: Arc<Runtime>,

    // mcp-name -> tag(version) -> load-balancer
    cache: Arc<RwLock<HashMap<String, HashMap<String, McpServerInfo>>>>,
    session_manager: Arc<Box<dyn session::Manager>>,
}

impl ProxyService {
    pub fn new(handle: Box<dyn ListHandler>, runtime: Arc<Runtime>, config: AppConfig) -> Self {
        let service = Self {
            handle: Arc::new(handle),
            runtime,
            cache: Arc::new(RwLock::new(HashMap::new())),
            session_manager: Arc::new(Box::new(LocalManager::new(config.session_manager.clone()))),
        };

        service.async_cache();

        service
    }

    #[allow(dead_code)]
    pub fn with_session_manager(&mut self, manager: Box<dyn session::Manager>) {
        self.session_manager = Arc::new(manager)
    }

    pub fn async_cache(&self) {
        let cache = self.cache.clone();
        let handle = self.handle.clone();
        self.runtime.spawn(async move {
            let mut ticker = interval(Duration::from_secs(24 * 60 * 60));
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

                // ---------test----------
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
                //--------------------
            }
        });
    }

    async fn load_server_info(&self, mcp_name: &str, tag: &str) -> Option<(Backend, String)> {
        let cache = self.cache.read().await;
        if let Some(tags) = cache.get(mcp_name) {
            if let Some(info) = tags.get(tag) {
                return match info.lb.select(b"", 256) {
                    None => None,
                    Some(backend) => Some((backend, info.endpoint.clone())),
                };
            }
        }
        None
    }

    pub(crate) async fn build_context_from_header(
        &self,
        session: &mut Session,
        ctx: &mut ProxyContext,
    ) -> pingora_core::Result<()> {
        let name = match session.req_header().headers.get("Proxy-Mcp-Name") {
            None => {
                tracing::error!("Can't find Proxy-Mcp-Name header");
                return Err(pingora_core::Error::explain(
                    InternalError,
                    "Can't get Proxy-Mcp-Name from headers.",
                ));
            }
            Some(value) => value.to_str().map_err(|_| {
                tracing::error!("Can't parse Proxy-Mcp-Name from header.");
                pingora_core::Error::explain(
                    InternalError,
                    "Can't parse Proxy-Mcp-Name from headers.",
                )
            })?,
        };

        let version = match session.req_header().headers.get("Proxy-Mcp-Version") {
            None => {
                tracing::error!("Can't get Proxy-Mcp-Version from headers.");
                return Err(pingora_core::Error::explain(
                    InternalError,
                    "Can't get Proxy-Mcp-Version from headers.",
                ));
            }
            Some(value) => value.to_str().map_err(|_| {
                tracing::error!("Can't parse Proxy-Mcp-Version from headers.");
                pingora_core::Error::explain(
                    InternalError,
                    "Can't parse Proxy-Mcp-Version from headers.",
                )
            })?,
        };

        let (backend, parsed) = self.load_mcp_info_from_cache(name, version).await?;
        ctx.endpoint = parsed.endpoint.clone();
        ctx.path = parsed.path.clone();
        ctx.host = parsed.host.clone();
        ctx.scheme = parsed.scheme.clone();
        ctx.port = parsed.port.clone();
        ctx.backend = Some(backend);
        ctx.name = name.to_string();
        ctx.tag = version.to_string();
        Ok(())
    }

    pub(crate) async fn build_context_from_uri(
        &self,
        session: &mut Session,
        ctx: &mut ProxyContext,
    ) -> pingora_core::Result<()> {
        let (mcp_name, tag) = match ctx
            .regex
            .captures(session.req_header().uri.to_string().as_str())
        {
            None => {
                tracing::error!("Can't parse uri {}", session.req_header().uri);
                return Err(pingora_core::Error::new(InternalError));
            }
            Some(caps) => (caps[1].to_string(), caps[2].to_string()),
        };

        let (backend, parsed) = self
            .load_mcp_info_from_cache(mcp_name.as_str(), tag.as_str())
            .await?;
        ctx.endpoint = parsed.endpoint.clone();
        ctx.path = parsed.path.clone();
        ctx.host = parsed.host.clone();
        ctx.scheme = parsed.scheme.clone();
        ctx.port = parsed.port.clone();
        ctx.backend = Some(backend);
        ctx.name = mcp_name.clone();
        ctx.tag = tag.clone();

        Ok(())
    }

    pub(crate) async fn build_context_from_message(
        &self,
        session_id: &str,
        ctx: &mut ProxyContext,
    ) -> pingora_core::Result<()> {
        let manager = self.session_manager.clone();
        let info = manager.load(session_id).map_err(|e| {
            tracing::error!("Can't load session {}: {}", session_id, e);
            pingora_core::Error::explain(
                InternalError,
                format!("Can't load session {}: {}", session_id, e),
            )
        })?;

        let (backend, parsed) = self.load_mcp_info_from_cache(&info.name, &info.tag).await?;

        ctx.endpoint = parsed.endpoint.clone();
        ctx.path = parsed.path.clone();
        ctx.host = parsed.host.clone();
        ctx.scheme = parsed.scheme.clone();
        ctx.port = parsed.port.clone();
        ctx.backend = Some(backend);
        ctx.name = info.name.clone();
        ctx.tag = info.tag.clone();
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
                tracing::error!("Can't load server {}, tag {}", mcp_name, tag);
                return Err(pingora_core::Error::explain(
                    ErrorType::HTTPStatus(404),
                    format!("Can't load server {}, tag {}", mcp_name, tag),
                ));
            }
        };

        let parsed = match parse_endpoint(endpoint.as_str()) {
            Ok(result) => result,
            Err(err) => {
                tracing::error!("Can't parse endpoint {}, error: {}", endpoint, err);
                return Err(pingora_core::Error::explain(
                    InternalError,
                    format!("Can't parse endpoint {}, error: {}", endpoint, err),
                ));
            }
        };
        Ok((backend, parsed))
    }

    // generate an error response and return
    async fn return_error_response(
        &self,
        session: &mut Session,
        status_code: u16,
        content: Bytes,
    ) -> pingora_core::Result<()> {
        let mut response_header = HttpSession::generate_error(status_code);

        response_header.insert_header("Server", "MCP-Proxy")?;
        response_header.set_content_length(content.len())?;

        session
            .write_error_response(response_header, Bytes::from(content))
            .await?;
        Ok(())
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

        if ctx.scheme == "https" {
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
        tracing::info!("Request filter =====> {}", session.req_header().uri);
        tracing::info!("Request filter =====> {}", session.req_header().method);
        if session.req_header().uri == "/" {
            self.build_context_from_header(session, ctx).await?;
            return Ok(false);
        }

        if let Some(caps) = ctx
            .regex
            .captures(session.req_header().uri.to_string().as_str())
        {
            let transport_type = &caps[3];
            if transport_type == "sse" || transport_type == "streamable" {
                self.build_context_from_uri(session, ctx).await?;
                return Ok(false);
            }

            let content = String::from("not supported transport type");

            self.return_error_response(
                session,
                StatusCode::INTERNAL_SERVER_ERROR.as_u16(),
                Bytes::from(content),
            )
            .await?;

            tracing::error!("not supported transport type: {}", transport_type);
            return Ok(true);
        }

        let session_id = parse_message(session.req_header().uri.to_string().as_str());
        if session_id != "" {
            self.build_context_from_message(&session_id, ctx).await?;
            return Ok(false);
        }

        let content = String::from("Invalid URI");

        self.return_error_response(
            session,
            StatusCode::NOT_FOUND.as_u16(),
            Bytes::from(content),
        )
        .await?;

        tracing::error!("Invalid URI : {}", session.req_header().uri);
        Ok(true)
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
        if ctx.scheme == "https" {
            upstream_request.insert_header("Host", ctx.host.clone())?;
        }

        // TODO Change to a more optimal logic to distinguish between /message and the initial connection.
        let session_id = parse_message(session.req_header().uri.to_string().as_str());
        if session_id != "" {
            return Ok(());
        }

        let uri = Uri::from_str(ctx.path.as_str()).unwrap();
        upstream_request.set_uri(uri);
        Ok(())
    }

    async fn response_filter(
        &self,
        session: &mut Session,
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
        let path = session.req_header().uri.path();
        if path.ends_with("/sse") {
            if let Some(body) = body.clone() {
                let text = String::from_utf8_lossy(body.as_ref());

                let re = Regex::new(r"sessionId=([a-f0-9-]+)").unwrap();

                if let Some(caps) = re.captures(&text) {
                    let session_id = caps.get(1).map(|m| m.as_str()).unwrap();
                    tracing::info!(
                        "connect mcp success sessionId={}, name={}, tag={}",
                        session_id,
                        ctx.name,
                        ctx.tag
                    );

                    let manager = self.session_manager.clone();
                    let name = ctx.name.clone();
                    let tag = ctx.tag.clone();
                    let scheme = ctx.scheme.clone();
                    let host = ctx.host.clone();
                    let sid = session_id.to_string().clone();

                    // waiting session info save
                    if let Err(err) = manager.save(
                        &sid,
                        SessionInfo {
                            name,
                            tag,
                            scheme,
                            host,
                        },
                    ) {
                        tracing::error!("error while saving session: {}", err);
                    }
                }
            }
        }
        Ok(())
    }
}

pub(crate) struct ProxyContext {
    regex: Regex,
    scheme: String,
    endpoint: String,
    host: String,
    port: String,
    path: String,
    backend: Option<Backend>,
    name: String,
    tag: String,
}

impl ProxyContext {
    pub fn new() -> Self {
        Self {
            regex: Regex::new(r"^/connect/([^/]+)/([^/]+)/([^/]+)(/.*)?$").unwrap(),
            scheme: String::from("https"),
            endpoint: String::from(""),
            host: String::from(""),
            port: String::from(""),
            path: String::from(""),
            backend: None,
            name: String::from(""),
            tag: String::from(""),
        }
    }
}

#[derive(Debug, Clone)]
struct ParsedEndpoint {
    endpoint: String,
    host: String,
    port: String,
    path: String,
    scheme: String,
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
            scheme: scheme.to_string(),
        })
    } else {
        Err(format!("Failed to parse endpoint {}", endpoint).into())
    }
}

fn parse_message(uri: &str) -> String {
    let re = Regex::new(r"^/message\?sessionId=([0-9a-fA-F\-]+)$").unwrap();

    if let Some(id) = re
        .captures(uri)
        .and_then(|caps| caps.get(1))
        .map(|m| m.as_str().to_string())
    {
        return id.clone();
    };

    "".to_owned()
}

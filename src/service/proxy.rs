use crate::app::application::Application;
use crate::app::config::McpCenter;
use crate::envs;
use crate::envs::REGISTRY_TYPE;
use crate::service::register::ListHandler;
use crate::service::register::external_api::ExternalApiHandler;
use crate::service::register::self_manager::SelfManagerHandler;
use async_trait::async_trait;
use bytes::Bytes;
use http::{StatusCode, Uri};
use pingora_core::prelude::{HttpPeer, Server};
use pingora_core::protocols::http::ServerSession as HttpSession;
use pingora_core::server::{RunArgs, ShutdownSignal, ShutdownSignalWatch};
use pingora_core::{ErrorType, InternalError};
use pingora_http::{RequestHeader, ResponseHeader};
use pingora_load_balancing::prelude::RoundRobin;
use pingora_load_balancing::{Backend, LoadBalancer};
use pingora_proxy::{ProxyHttp, Session};
use regex::Regex;
use std::collections::HashMap;
use std::env;
use std::error::Error;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use std::fs;
use tokio::runtime::Runtime;
use tokio::sync::RwLock;
use tokio::time::interval;
use tokio_util::sync::CancellationToken;
use crate::app::config;
use crate::common::utils;

const REGISTRY_TYPE_SELF_MANAGEMENT: &str = "self";
const REGISTRY_TYPE_NACOS: &str = "nacos";
const REGISTRY_TYPE_REDIS: &str = "redis";
const REGISTRY_TYPE_EXTERNAL_API: &str = "external";

#[derive(Default)]
pub enum Registry {
    #[default]
    SelfManagement,
    Nacos(NacosConfig),
    Redis(RedisConfig),
    ExternalAPI(ExternalApiConfig),
}

// TODO support get mcp server from nacos
pub struct NacosConfig {}

// TODO support get mcp server from redis
pub struct RedisConfig {}

pub struct ExternalApiConfig {
    pub url: String,
    pub authorization: Option<String>,
}

#[derive(Default)]
struct Bootstrap {
    pub port: u16,
    pub registry: Registry,
}

pub struct MainServer {
    bootstrap: Bootstrap,
}

impl MainServer {
    pub fn new() -> Self {
        Self {
            bootstrap: Default::default(),
        }
    }

    fn start(
        &self,
        shutdown_signal: Box<dyn ShutdownSignalWatch>,
        rt: Runtime,
    ) -> Result<(), Box<dyn Error>> {
        let mut server = Server::new(None).unwrap();

        server.bootstrap();

        let handler: Box<dyn ListHandler> = match &self.bootstrap.registry {
            Registry::SelfManagement => Box::new(SelfManagerHandler::new()),
            // TODO: support nacos
            Registry::Nacos(_) => Box::new(SelfManagerHandler::new()),
            // TODO: support redis
            Registry::Redis(_) => Box::new(SelfManagerHandler::new()),
            Registry::ExternalAPI(config) => Box::new(ExternalApiHandler::new(
                config.url.as_str(),
                config.authorization.clone(),
            )),
        };

        let runtime = Arc::new(rt);

        let mut service = pingora_proxy::http_proxy_service_with_name(
            &server.configuration,
            ProxyService::new(handler, runtime.clone()),
            "McpGateway",
        );

        tracing::info!("starting HTTP server on port {}", self.bootstrap.port);
        service.add_tcp(format!("0.0.0.0:{}", self.bootstrap.port).as_str());

        server.add_service(service);

        server.run(RunArgs { shutdown_signal });

        Ok(())
    }
}

impl Application for MainServer {
    fn prepare(&mut self, path: String) -> Result<(), Box<dyn Error>> {
        tracing::info!("Preparing Censor application with config: {}", path);

        let mut content = fs::read_to_string(path.clone()).map_err(|e| {
            tracing::error!("Failed to read config file {}: {}", path, e);
            e
        })?;

        content = utils::replace_env_variables(content);

        let config: config::McpCenter = toml::from_str(&content).map_err(|e| {
            tracing::error!("Failed to parse TOML config: {}", e);
            e
        })?;


        self.bootstrap.port = config.port;

        let registry = env::var(REGISTRY_TYPE);
        self.bootstrap.registry = if let Ok(tpy) = &registry {
            match tpy.clone().as_str() {
                REGISTRY_TYPE_SELF_MANAGEMENT => Registry::SelfManagement,
                REGISTRY_TYPE_NACOS => Registry::Nacos(NacosConfig {}),
                REGISTRY_TYPE_REDIS => Registry::Redis(RedisConfig {}),
                REGISTRY_TYPE_EXTERNAL_API => build_external_api_registry()?,
                _ => return Err("Unsupported registration types".into()),
            }
        } else {
            // default use self management
            Registry::SelfManagement
        };

        Ok(())
    }

    fn run(&mut self, shutdown: CancellationToken, rt: Runtime) -> Result<(), Box<dyn Error>> {
        self.start(Box::new(ShutdownSign::new(shutdown)), rt)?;
        Ok(())
    }
}

struct ShutdownSign(CancellationToken);

impl ShutdownSign {
    pub fn new(cancel: CancellationToken) -> Self {
        Self(cancel)
    }
}

#[async_trait]
impl ShutdownSignalWatch for ShutdownSign {
    async fn recv(&self) -> ShutdownSignal {
        self.0.cancelled().await;
        ShutdownSignal::FastShutdown
    }
}

struct McpServerInfo {
    lb: LoadBalancer<RoundRobin>,
    pub endpoint: String,
}

struct ProxyService {
    handle: Arc<Box<dyn ListHandler>>,
    runtime: Arc<Runtime>,
    // mcp-name -> tag(version) -> load-balancer
    cache: Arc<RwLock<HashMap<String, HashMap<String, McpServerInfo>>>>,
    
}

impl ProxyService {
    pub fn new(handle: Box<dyn ListHandler>, runtime: Arc<Runtime>) -> Self {
        let service = Self {
            handle: Arc::new(handle),
            runtime,
            cache: Arc::new(RwLock::new(HashMap::new())),
        };

        service.async_cache();

        service
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
                tmp.insert("default".to_owned(), McpServerInfo{ lb: upstream, endpoint: "http://127.0.0.1:3000/mcp".to_string() });
                mcps.insert("local".to_string(), tmp);

                tracing::info!("Mcp servers: {:?}", mcps.len());
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
        _session: &mut Session,
        upstream_request: &mut RequestHeader,
        ctx: &mut Self::CTX,
    ) -> pingora_core::Result<()>
    where
        Self::CTX: Send + Sync,
    {
        if ctx.scheme == "https" {
            upstream_request.insert_header("Host", ctx.host.clone())?;
        }
        let uri = Uri::from_str(ctx.path.as_str()).unwrap();
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

    fn upstream_response_body_filter(&self, session: &mut Session, body: &mut Option<Bytes>, _end_of_stream: bool, _ctx: &mut Self::CTX) -> pingora_core::Result<()> {
        let path = session.req_header().uri.path();
        if path.ends_with("/sse") {
            if let Some(body) = body.clone() {
                let text = String::from_utf8_lossy(body.as_ref());

                let re = Regex::new(r"sessionId=([a-f0-9-]+)").unwrap();

                if let Some(caps) = re.captures(&text) {
                    let session_id = caps.get(1).map(|m| m.as_str()).unwrap();
                    println!("sessionId: {}", session_id);
                } else {
                    println!("No sessionId found");
                }
            }
        }
        Ok(())
    }
}

struct ProxyContext {
    regex: Regex,
    scheme: String,
    endpoint: String,
    host: String,
    port: String,
    path: String,
    backend: Option<Backend>,
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
        }
    }
}

fn build_external_api_registry() -> Result<Registry, Box<dyn Error>> {
    let mut config = ExternalApiConfig {
        url: env::var(envs::EXTERNAL_API)?,
        authorization: None,
    };

    config.authorization = match env::var(envs::EXTERNAL_API_AUTHORIZATION) {
        Ok(auth) => Some(auth),
        Err(_) => {
            tracing::warn!("External API authorization env variable not set");
            None
        }
    };

    Ok(Registry::ExternalAPI(config))
}

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

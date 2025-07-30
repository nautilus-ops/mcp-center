use crate::app::application::Application;
use crate::app::config::AppConfig;
use crate::envs;
use crate::envs::REGISTRY_TYPE;
use crate::service::handle::ListHandler;
use crate::service::handle::external_api::ExternalApiHandler;
use crate::service::handle::self_manager::SelfManagerHandler;
use async_trait::async_trait;
use bytes::Bytes;
use http::{StatusCode, Uri};
use pingora_core::ErrorType;
use pingora_core::prelude::{HttpPeer, Server};
use pingora_core::protocols::http::ServerSession as HttpSession;
use pingora_core::server::{RunArgs, ShutdownSignal, ShutdownSignalWatch};
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
use tokio::runtime::Runtime;
use tokio::sync::RwLock;
use tokio::time::interval;
use tokio_util::sync::CancellationToken;

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
    fn prepare(&mut self, config: AppConfig) -> Result<(), Box<dyn Error>> {
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
                    let mut item = HashMap::<String, McpServerInfo>::new();
                    let (_, host, port, _) = match parse_endpoint(server.endpoint.as_str()) {
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

                    let ep = format!("{}:{}", host, port);

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

                    let tag = match &server.version {
                        None => "default".to_string(),
                        Some(version) => version.clone(),
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
}
#[async_trait]
impl ProxyHttp for ProxyService {
    type CTX = ProxyContext;

    fn new_ctx(&self) -> Self::CTX {
        ProxyContext::new()
    }

    async fn upstream_peer(
        &self,
        session: &mut Session,
        ctx: &mut Self::CTX,
    ) -> pingora_core::Result<Box<HttpPeer>> {
        let (mcp_name, tag) = match ctx
            .regex
            .captures(session.req_header().uri.to_string().as_str())
        {
            None => {
                tracing::error!("Can't parse uri {}", session.req_header().uri);
                return Err(pingora_core::Error::new(ErrorType::InternalError));
            }
            Some(caps) => (caps[1].to_string(), caps[2].to_string()),
        };

        let info = match self.load_server_info(mcp_name.as_str(), tag.as_str()).await {
            Some(info) => info,
            None => {
                tracing::error!("Can't load server {}, tag {}", mcp_name, tag);
                return Err(pingora_core::Error::new(ErrorType::InternalError));
            }
        };

        let (scheme, host, port, path) = match parse_endpoint(info.1.as_str()) {
            Ok(result) => result,
            Err(err) => {
                tracing::error!("Can't parse endpoint {}, error: {}", info.1, err);
                return Err(pingora_core::Error::new(ErrorType::InternalError));
            }
        };

        ctx.endpoint = info.1.clone();
        ctx.scheme = scheme.clone();
        ctx.host = host.clone();
        ctx.path = path.clone();
        ctx.port = port.clone();

        let mut proxy = HttpPeer::new(info.0.clone(), false, String::from(""));

        if scheme == "https" {
            proxy = HttpPeer::new(info.0.clone(), true, host.clone());

            session
                .req_header_mut()
                .insert_header("Upstream-Host", host.clone())?;
        }

        session
            .req_header_mut()
            .insert_header("Upstream-Path", path.clone())?;

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
            return Ok(false);
        }

        if let Some(caps) = ctx
            .regex
            .captures(session.req_header().uri.to_string().as_str())
        {
            let transport_type = &caps[3];
            if transport_type == "sse" || transport_type == "streamable" {
                return Ok(false);
            }

            let content = String::from("not supported transport type");
            let mut response_header =
                HttpSession::generate_error(StatusCode::INTERNAL_SERVER_ERROR.as_u16());

            response_header.insert_header("Server", "MCP-Proxy")?;
            response_header.set_content_length(content.len())?;

            session
                .write_error_response(response_header, Bytes::from(content))
                .await?;

            tracing::error!("not supported transport type: {}", transport_type);
            return Ok(true);
        }

        let content = String::from("Invalid URI");
        let mut response_header = HttpSession::generate_error(StatusCode::NOT_FOUND.as_u16());

        response_header.insert_header("Server", "MCP-Proxy")?;
        response_header.set_content_length(content.len())?;

        session
            .write_error_response(response_header, Bytes::from(content))
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
}

struct ProxyContext {
    regex: Regex,
    scheme: String,
    endpoint: String,
    host: String,
    port: String,
    path: String,
}

impl ProxyContext {
    pub fn new() -> Self {
        Self {
            regex: Regex::new(r"^/connect/([^/]+)/([^/]+)/([^/]+)$").unwrap(),
            scheme: String::from("https"),
            endpoint: String::from(""),
            host: String::from(""),
            port: String::from(""),
            path: String::from(""),
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

fn parse_endpoint(endpoint: &str) -> Result<(String, String, String, String), Box<dyn Error>> {
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
        Ok((
            String::from(scheme),
            String::from(host),
            String::from(port),
            String::from(path),
        ))
    } else {
        Err(format!("Failed to parse endpoint {}", endpoint).into())
    }
}

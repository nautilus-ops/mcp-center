use crate::app::application::Application;
use crate::app::config::AppConfig;
use crate::envs;
use crate::service::handle::ListHandler;
use crate::service::handle::external_api::ExternalApiHandler;
use crate::service::handle::self_manager::SelfManagerHandler;
use async_trait::async_trait;
use bytes::Bytes;
use http::StatusCode;
use pingora_core::prelude::{HttpPeer, Server};
use pingora_core::protocols::http::ServerSession as HttpSession;
use pingora_core::server::{RunArgs, ShutdownSignal, ShutdownSignalWatch};
use pingora_http::{RequestHeader, ResponseHeader};
use pingora_proxy::{ProxyHttp, Session};
use regex::Regex;
use std::env;
use std::error::Error;
use std::rc::Rc;
use std::sync::Arc;
use tokio::runtime::Runtime;
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
    rt: Option<Arc<Runtime>>,
}

impl MainServer {
    pub fn new() -> Self {
        Self {
            bootstrap: Default::default(),
            rt: None,
        }
    }

    fn start(&self, shutdown_signal: Box<dyn ShutdownSignalWatch>) -> Result<(), Box<dyn Error>> {
        let mut server = Server::new(None).unwrap();

        server.bootstrap();

        let handler: Box<dyn ListHandler> = match &self.bootstrap.registry {
            Registry::SelfManagement => Box::new(SelfManagerHandler::default()),
            // TODO: support nacos
            Registry::Nacos(_) => Box::new(SelfManagerHandler::default()),
            // TODO: support redis
            Registry::Redis(_) => Box::new(SelfManagerHandler::default()),
            Registry::ExternalAPI(config) => Box::new(ExternalApiHandler::new(
                config.url.as_str(),
                config.authorization.clone(),
            )),
        };

        let mut service = pingora_proxy::http_proxy_service_with_name(
            &server.configuration,
            ProxyService::new(handler),
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

        let registry = env::var("REGISTRY_TYPE");
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
        self.start(Box::new(ShutdownSign::new(shutdown)))?;
        self.rt = Some(Arc::new(rt));
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

struct ProxyService {
    handle: Box<dyn ListHandler>,
}

impl ProxyService {
    pub fn new(handle: Box<dyn ListHandler>) -> Self {
        Self { handle }
    }
}

#[async_trait]
impl ProxyHttp for ProxyService {
    type CTX = Regex;

    fn new_ctx(&self) -> Self::CTX {
        Regex::new(r"^/connect/([^/]+)/([^/]+)/([^/]+)$").unwrap()
    }

    async fn upstream_peer(
        &self,
        session: &mut Session,
        ctx: &mut Self::CTX,
    ) -> pingora_core::Result<Box<HttpPeer>> {
        let proxy = HttpPeer::new(
            "www.google.com:443",
            true,
            "www.google.com".to_string(),
        );
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

        if let Some(caps) = ctx.captures(session.req_header().uri.to_string().as_str()) {
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
        session: &mut Session,
        upstream_request: &mut RequestHeader,
        ctx: &mut Self::CTX,
    ) -> pingora_core::Result<()>
    where
        Self::CTX: Send + Sync,
    {
        upstream_request.insert_header("Host", "www.google.com")?;
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

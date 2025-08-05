use std::error::Error;
use std::fs;
use std::sync::Arc;
use async_trait::async_trait;
use pingora_core::prelude::Server;
use pingora_core::server::{RunArgs, ShutdownSignal, ShutdownSignalWatch};
use tokio::runtime::Runtime;
use tokio_util::sync::CancellationToken;
use crate::app::application::Application;
use crate::service::config::{AppConfig, McpRegistry};
use crate::common::utils;
use crate::service::proxy;
use crate::service::register::external_api::ExternalApiHandler;
use crate::service::register::ListHandler;
use crate::service::register::self_manager::SelfManagerHandler;

#[derive(Default)]
pub enum Registry {
    #[default]
    Memory,
    ExternalAPI(ExternalApiConfig),
}

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
    config: AppConfig
}
impl MainServer {
    pub fn new() -> Self {
        Self {
            bootstrap: Default::default(),
            config: Default::default(),
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
            Registry::Memory => Box::new(SelfManagerHandler::new()),
            Registry::ExternalAPI(config) => Box::new(ExternalApiHandler::new(
                config.url.as_str(),
                config.authorization.clone(),
            )),
        };

        let runtime = Arc::new(rt);

        let mut service = pingora_proxy::http_proxy_service_with_name(
            &server.configuration,
            proxy::ProxyService::new(handler, runtime.clone(),self.config.clone()),
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

        let config: AppConfig = toml::from_str(&content).map_err(|e| {
            tracing::error!("Failed to parse TOML config: {}", e);
            e
        })?;

        tracing::debug!("The application config: \n{:?}", config);

        self.config = config.clone();

        self.bootstrap.port = config.mcp_center.http_port;

        self.bootstrap.registry = match config.mcp_registry {
            McpRegistry::LocalMemory => Registry::Memory,
            McpRegistry::External { url, token } => build_external_api_registry(url, token),
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

fn build_external_api_registry(url: String, token: Option<String>) -> Registry {
    let config = ExternalApiConfig {
        url,
        authorization: token,
    };
    Registry::ExternalAPI(config)
}
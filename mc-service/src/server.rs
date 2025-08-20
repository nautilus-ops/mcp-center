use crate::cache::mcp_servers::Cache;
use crate::config::{AppConfig, McpRegistry};
use async_trait::async_trait;
use axum::Router;
use hyper_rustls::HttpsConnectorBuilder;
use hyper_util::client::legacy::Client;
use hyper_util::rt::TokioExecutor;
use mc_booter::app::application::Application;
use mc_common::utils;
use mc_register::external_api::ExternalApiHandler;
use mc_register::self_manager::SelfManagerRegistry;
use pingora_core::server::{ShutdownSignal, ShutdownSignalWatch};
use std::error::Error;
use std::fs;
use std::sync::Arc;
use tokio::runtime::Runtime;
use tokio_util::sync::CancellationToken;
use crate::reverse_proxy::connection::ConnectionService;

pub enum Registry {
    Memory(String),
    ExternalAPI(ExternalApiConfig),
}

impl Default for Registry {
    fn default() -> Self {
        Self::Memory(String::from("mcp_servers.toml"))
    }
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
    config: AppConfig,
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
        // let mut server = Server::new(None).unwrap();
        //
        // server.bootstrap();
        //
        let handler: Box<dyn mc_register::Registry> = match &self.bootstrap.registry {
            Registry::Memory(path) => Box::new(SelfManagerRegistry::new(path.clone())),
            Registry::ExternalAPI(config) => Box::new(ExternalApiHandler::new(
                config.url.as_str(),
                config.authorization.clone(),
            )),
        };
        //
        //
        // let mut service = pingora_proxy::http_proxy_service_with_name(
        //     &server.configuration,
        //     proxy::ProxyService::new(handler, runtime.clone(), self.config.clone()),
        //     "McpGateway",
        // );
        //
        // tracing::info!("starting HTTP server on port {}", self.bootstrap.port);
        // service.add_tcp(format!("0.0.0.0:{}", self.bootstrap.port).as_str());
        //
        // server.add_service(service);
        //
        // server.run(RunArgs { shutdown_signal });
        let runtime = Arc::new(rt);

        let cache = Arc::new(Cache::new(Arc::new(handler), runtime.clone(), 100));

        let https = HttpsConnectorBuilder::new()
            .with_native_roots()
            .unwrap()
            .https_or_http()
            .enable_http1()
            .build();

        let client = Arc::new(Client::builder(TokioExecutor::new()).build(https));

        // let service = ReverseProxyService::new(Arc::new(client)).with_filter(Arc::new(SseFilter::new()));

        let service = ConnectionService::new(client, cache);

        let app = Router::new().route_service("/connect/{name}/{tag}", service);

        runtime.block_on(async move {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:4000")
                .await
                .unwrap();
            println!("listening on {}", listener.local_addr().unwrap());
            axum::serve(listener, app).await.unwrap();
        });
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
            McpRegistry::LocalMemory {
                mcp_definition_path,
            } => Registry::Memory(mcp_definition_path),
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

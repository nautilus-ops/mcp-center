use crate::cache::mcp_servers::Cache;
use crate::config::{AppConfig, McpRegistry};
use crate::reverse_proxy::connection::ConnectionService;
use crate::reverse_proxy::message::MessageService;
use axum::Router;
use hyper_rustls::HttpsConnectorBuilder;
use hyper_util::client::legacy::Client;
use hyper_util::rt::TokioExecutor;
use mc_booter::app::application::Application;
use mc_common::utils;
use mc_loader::external_api::ExternalApiLoader;
use mc_loader::local::LocalFileLoader;
use std::error::Error;
use std::fs;
use std::sync::Arc;
use tokio::runtime::Runtime;
use tokio_util::sync::CancellationToken;

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

pub struct McpCenterServer {
    bootstrap: Bootstrap,
    config: AppConfig,
}
impl McpCenterServer {
    pub fn new() -> Self {
        Self {
            bootstrap: Default::default(),
            config: Default::default(),
        }
    }

    fn start(&self, shutdown_signal: CancellationToken, rt: Runtime) -> Result<(), Box<dyn Error>> {
        let handler: Box<dyn mc_loader::Loader> = match &self.bootstrap.registry {
            Registry::Memory(path) => Box::new(LocalFileLoader::new(path.clone())),
            Registry::ExternalAPI(config) => Box::new(ExternalApiLoader::new(
                config.url.as_str(),
                config.authorization.clone(),
            )),
        };

        let runtime = Arc::new(rt);

        let cache = Arc::new(Cache::new(Arc::new(handler), runtime.clone(), 100));

        let https = HttpsConnectorBuilder::new()
            .with_native_roots()
            .unwrap()
            .https_or_http()
            .enable_http1()
            .build();

        let client = Arc::new(Client::builder(TokioExecutor::new()).build(https));

        let app = Router::new()
            .route_service(
                "/proxy/connect/{name}/{tag}",
                ConnectionService::new(client.clone(), cache.clone()),
            )
            .route_service(
                "/proxy/message/{name}/{tag}/{*subPath}",
                MessageService::new(client.clone(), cache.clone()),
            );

        runtime.block_on(async move {
            let listener =
                tokio::net::TcpListener::bind(format!("0.0.0.0:{}", self.bootstrap.port))
                    .await
                    .unwrap();

            let shutdown_signal = || async move {
                shutdown_signal.cancelled().await;
                tracing::info!("Shutting down...");
            };

            tracing::info!("starting HTTP server on port {}", self.bootstrap.port);

            axum::serve(listener, app)
                .with_graceful_shutdown(shutdown_signal())
                .await
                .unwrap();
        });
        Ok(())
    }
}

impl Application for McpCenterServer {
    fn new() -> Self {
        Self::new()
    }

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
        self.start(shutdown, rt)?;
        Ok(())
    }
}
fn build_external_api_registry(url: String, token: Option<String>) -> Registry {
    let config = ExternalApiConfig {
        url,
        authorization: token,
    };
    Registry::ExternalAPI(config)
}

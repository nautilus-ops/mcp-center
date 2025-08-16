use crate::service::config::AppConfig;
use axum::Router;
use axum::routing::get;
use mc_booter::app::application::Application;
use mc_common::utils;
use std::error::Error;
use std::fs;
use tokio::net::TcpListener;
use tokio::runtime::Runtime;
use tokio_util::sync::CancellationToken;

async fn shutdown_signal(shutdown: CancellationToken) {
    shutdown.cancelled().await;
}

pub struct RegisterService {
    config: AppConfig,
}

impl RegisterService {
    pub fn new() -> Self {
        Self {
            config: Default::default(),
        }
    }

    fn start(&self, shutdown: CancellationToken, rt: Runtime) -> Result<(), Box<dyn Error>> {
        rt.block_on(async {
            let app = Router::new().route("/", get(|| async { "Hello from Axum" }));

            let listener =
                TcpListener::bind(format!("0.0.0.0:{}", self.config.mcp_registry.http_port))
                    .await
                    .unwrap();
            tracing::info!("listening on {}", listener.local_addr().unwrap());

            axum::serve(listener, app)
                .with_graceful_shutdown(shutdown_signal(shutdown))
                .await
                .unwrap();
        });

        Ok(())
    }
}

impl Application for RegisterService {
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

        self.config = config;

        Ok(())
    }

    fn run(&mut self, shutdown: CancellationToken, rt: Runtime) -> Result<(), Box<dyn Error>> {
        self.start(shutdown, rt)
    }
}

use crate::config::{AppConfig, McpRegistry};
use crate::reverse_proxy;
use crate::reverse_proxy::connection::ConnectionService;
use crate::reverse_proxy::message::MessageService;
use axum::extract::{Request, State};
use axum::middleware::Next;
use axum::response::Response;
use axum::routing::{get, post};
use axum::{Router, middleware};
use http::StatusCode;
use hyper_rustls::HttpsConnectorBuilder;
use hyper_util::client::legacy::Client;
use hyper_util::rt::TokioExecutor;
use mc_booter::app::application::Application;
use mc_common::app::cache::Cache;
use mc_common::app::event::Event;
use mc_common::app::{AppState, HandlerManager};
use mc_common::router;
use mc_common::router::RouterHandler;
use mc_db::DBClient;
use std::error::Error;
use std::sync::Arc;
use tokio::runtime::Runtime;
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;

pub enum Registry {
    #[allow(dead_code)]
    Memory(String),
    #[allow(dead_code)]
    ExternalAPI(ExternalApiConfig),
}

impl Default for Registry {
    fn default() -> Self {
        Self::Memory(String::from("mcp_servers.toml"))
    }
}

pub struct ExternalApiConfig {
    #[allow(dead_code)]
    pub url: String,
    #[allow(dead_code)]
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
    state: Option<AppState>,
}
impl McpCenterServer {
    pub fn new() -> Self {
        Self {
            bootstrap: Default::default(),
            config: Default::default(),
            state: None,
        }
    }

    fn start(
        &self,
        shutdown_signal: CancellationToken,
        runtime: Arc<Runtime>,
    ) -> Result<(), Box<dyn Error>> {
        let state = self.state.clone().unwrap();

        let builder = router::RouterBuilder::<AppState>::new()
            .with_register(reverse_proxy::register_router(
                state.https_client.clone(),
                state.mcp_cache.clone(),
            ))
            .with_register(mc_registry::register_router())
            .with_layer(layer_authorization(self.config.clone(), state.clone()));

        let app = builder.build(state);

        // starting axum service
        runtime.block_on(async move {
            let listener =
                tokio::net::TcpListener::bind(format!("0.0.0.0:{}", self.bootstrap.port))
                    .await
                    .unwrap();

            let shutdown = || async move {
                shutdown_signal.cancelled().await;
                tracing::info!("Shutting down...");
            };

            tracing::info!("starting HTTP server on port {}", self.bootstrap.port);

            axum::serve(listener, app)
                .with_graceful_shutdown(shutdown())
                .await
                .unwrap();
        });
        Ok(())
    }
}

impl Application for McpCenterServer {
    type Config = AppConfig;

    fn new() -> Self {
        Self::new()
    }

    fn prepare(
        &mut self,
        config: Self::Config,
        runtime: Arc<Runtime>,
    ) -> Result<(), Box<dyn Error>> {
        self.config = config.clone();

        self.bootstrap.port = config.mcp_center.http_port;

        self.bootstrap.registry = match config.mcp_registry {
            McpRegistry::LocalMemory {
                mcp_definition_path,
            } => Registry::Memory(mcp_definition_path),
            McpRegistry::External { url, token } => build_external_api_registry(url, token),
        };

        let (tx, _) = broadcast::channel::<Event>(100);

        let https = HttpsConnectorBuilder::new()
            .with_native_roots()
            .unwrap()
            .https_or_http()
            .enable_http1()
            .build();

        let client = Arc::new(Client::builder(TokioExecutor::new()).build(https));

        let host = &self.config.postgres.host;
        let port = self.config.postgres.port;
        let username = &self.config.postgres.username;
        let database = &self.config.postgres.database;
        let password = &self.config.postgres.password;
        let max_connection = self.config.postgres.max_connection;

        let db_client = runtime.block_on(async move {
            DBClient::create(host, port, username, password, database, max_connection)
                .await.inspect_err(|_| {
                tracing::error!("Error creating database client, host: {host}, port: {port}, user: {username}, database: {database}, max_connection: {max_connection}");
            }).unwrap()
        });
        let db_client = Arc::new(db_client);

        // mcp cache for reverse proxy, load mcp servers from postgres
        let cache = Arc::new(Cache::new(
            db_client.clone(),
            tx.subscribe(),
            runtime.clone(),
            100,
        ));

        let manager = HandlerManager::new(db_client.clone())
            .with_mcp_handler()
            .with_system_settings_handler()
            .with_api_keys_handler();

        let state = AppState::new(
            db_client.clone(),
            tx.clone(),
            client.clone(),
            cache.clone(),
            manager,
        );

        self.state = Some(state);

        Ok(())
    }

    fn run(
        &mut self,
        shutdown: CancellationToken,
        runtime: Arc<Runtime>,
    ) -> Result<(), Box<dyn Error>> {
        self.start(shutdown, runtime)?;
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

fn layer_authorization(config: AppConfig, state: AppState) -> RouterHandler<AppState> {
    Box::new(move |router| {
        router.layer(middleware::from_fn_with_state(
            (config.clone(), state.clone()),
            authorization,
        ))
    })
}

async fn authorization(
    State((config, state)): State<(AppConfig, AppState)>,
    req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    if let Some(key) = req.headers().get(http::header::AUTHORIZATION) {
        if key == config.mcp_center.admin_token.as_str() {
            return Ok(next.run(req).await);
        }
    }
    Err(StatusCode::UNAUTHORIZED)
}

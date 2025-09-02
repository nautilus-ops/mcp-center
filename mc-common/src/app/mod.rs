pub mod cache;
pub mod event;

use crate::app::cache::Cache;
use crate::app::event::Event;
use axum::body::Body;
use hyper_rustls::HttpsConnector;
use hyper_util::client::legacy::Client;
use hyper_util::client::legacy::connect::HttpConnector;
use mc_db::DBClient;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::broadcast::Sender;

#[derive(Clone)]
pub struct AppState {
    pub db: Arc<DBClient>,
    pub event_sender: Sender<Event>,
    pub https_client: Arc<Client<HttpsConnector<HttpConnector>, Body>>,
    pub mcp_cache: Arc<Cache>,
    handler_manager: HandlerManager,
}
impl AppState {
    pub fn new(
        db: Arc<DBClient>,
        event_sender: Sender<Event>,
        https_client: Arc<Client<HttpsConnector<HttpConnector>, Body>>,
        mcp_cache: Arc<Cache>,
        handler_manager: HandlerManager,
    ) -> Self {
        Self {
            db,
            event_sender,
            https_client,
            mcp_cache,
            handler_manager,
        }
    }
    pub fn handlers(&self) -> &HandlerManager {
        &self.handler_manager
    }
}

#[derive(Clone)]
pub struct HandlerManager {
    pub mcp_handler: Option<Arc<mc_db::McpDBHandler>>,
    pub system_settings_handler: Option<Arc<mc_db::SystemSettingsDBHandler>>,
    pub api_keys_handler: Option<Arc<mc_db::ApiKeyDBHandler>>,
    db: Arc<DBClient>,
}

impl HandlerManager {
    pub fn new(db: Arc<DBClient>) -> Self {
        HandlerManager {
            db,
            mcp_handler: None,
            system_settings_handler: None,
            api_keys_handler: None,
        }
    }

    pub fn with_mcp_handler(mut self) -> Self {
        self.mcp_handler = Some(Arc::new(mc_db::McpDBHandler::new(self.db.clone())));
        self
    }
    pub fn with_system_settings_handler(mut self) -> Self {
        self.system_settings_handler = Some(Arc::new(mc_db::SystemSettingsDBHandler::new(
            self.db.clone(),
        )));
        self
    }

    pub fn with_api_keys_handler(mut self) -> Self {
        self.api_keys_handler = Some(Arc::new(mc_db::ApiKeyDBHandler::new(self.db.clone())));
        self
    }
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct Response {
    data: Option<serde_json::Value>,
    code: u16,
    message: String,
}

impl Response {
    pub fn new(data: Option<serde_json::Value>) -> Self {
        Self {
            data,
            code: 200,
            message: "ok".to_string(),
        }
    }

    #[allow(dead_code)]
    fn with_code(mut self, code: u16) -> Self {
        self.code = code;
        self
    }

    #[allow(dead_code)]
    fn with_message(mut self, message: String) -> Self {
        self.message = message;
        self
    }
}

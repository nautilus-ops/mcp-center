mod mcp_server;

use crate::event::Event;
use axum::body::Body;
use cache::mcp_servers::Cache;
use hyper_rustls::HttpsConnector;
use hyper_util::client::legacy::Client;
use hyper_util::client::legacy::connect::HttpConnector;
use mc_db::DBClient;
pub use mcp_server::*;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::broadcast::Sender;

pub mod cache;
pub mod event;

#[derive(Clone)]
pub struct AppState {
    pub db: Arc<DBClient>,
    pub event_sender: Sender<Event>,
    pub https_client: Arc<Client<HttpsConnector<HttpConnector>, Body>>,
    pub mcp_cache: Arc<Cache>,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct Response {
    data: Option<serde_json::Value>,
    code: u16,
    message: String,
}

impl Response {
    fn new(data: Option<serde_json::Value>) -> Self {
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

mod mcp_server;

pub use mcp_server::*;

use crate::db::DBClient;
use crate::event::Event;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::broadcast::Sender;

#[derive(Clone)]
pub struct AppState {
    pub db: Arc<DBClient>,
    pub event_sender: Sender<Event>,
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

    fn with_code(mut self, code: u16) -> Self {
        self.code = code;
        self
    }

    fn with_message(mut self, message: String) -> Self {
        self.message = message;
        self
    }

    fn internal_error_resp(message: &str) -> Self {
        Self::new(None)
            .with_code(500)
            .with_message(message.to_string())
    }
}

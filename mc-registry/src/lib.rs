mod mcp_server;

use crate::event::Event;
use mc_db::DBClient;
pub use mcp_server::*;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::broadcast::Sender;

pub mod event;
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

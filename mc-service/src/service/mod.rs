mod mcp_server;

use std::sync::Arc;
pub use mcp_server::*;
use crate::db::DBClient;

#[derive(Clone)]
pub struct AppState {
    pub db: Arc<DBClient>,
}

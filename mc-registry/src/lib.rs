mod mcp_server;

use axum::routing::{get, post};
use axum::{Router, middleware};
use mc_common::app::AppState;
use mc_common::router;
pub use mcp_server::*;

pub fn register_router() -> router::RouterHandler<AppState> {
    Box::new(|router| {
        router
            .route("/api/registry/mcp-server", get(list_all))
            .route("/api/registry/mcp-server", post(register_mcp_server))
    })
}

use crate::db::{DBClient, McpHandler};
use axum::Json;
use axum::extract::State;
use http::StatusCode;
use std::sync::Arc;
use tokio::sync::Mutex;
use crate::db::model::McpServers;
use crate::service::AppState;

pub async fn list_all(
    State(state): State<AppState>,
) -> Result<Json<Vec<McpServers>>, (StatusCode, String)> {
    let db_client = state.db.clone();
    let handler = McpHandler::new(db_client);
    let servers = handler.list_all().await.map_err(|e| {
        tracing::error!("Failed to list all mcp servers {}", e);
        (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error".to_string())
    })?;

    Ok(Json(servers))
}

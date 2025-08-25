use crate::db::McpDBHandler;
use crate::db::model::McpServers;
use crate::service::{AppState, Response};
use axum::Json;
use axum::extract::State;
use chrono::NaiveDateTime;
use http::StatusCode;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub async fn list_all(
    State(state): State<AppState>,
) -> Result<Json<Response>, (StatusCode, String)> {
    let db_client = state.db.clone();
    let handler = McpDBHandler::new(db_client);
    let servers = handler.list_all().await.map_err(|e| {
        tracing::error!("Failed to list mcp servers {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to list mcp servers".to_string(),
        )
    })?;

    let data = serde_json::to_value(servers).map_err(|e| {
        tracing::error!("Failed to parse mcp servers {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Internal server error".to_string(),
        )
    })?;

    Ok(Json(Response::new(Some(data))))
}

#[derive(Deserialize,Serialize,Clone)]
pub struct McpRegisterRequest {
    pub name: String,
    pub tag: String,
    pub endpoint: String,
    pub transport_type: String,
    pub description: String,
    pub extra: Option<serde_json::Value>,
}

pub async fn register_mcp_server(
    State(state): State<AppState>,
    Json(server): Json<McpRegisterRequest>,
) -> Result<Json<Response>, (StatusCode, String)> {
    let db_client = state.db.clone();
    let res = McpDBHandler::new(db_client).create_or_update(&McpServers{
        id: Uuid::new_v4(),
        name: server.name.clone(),
        tag: server.tag.clone(),
        endpoint: server.endpoint.clone(),
        transport_type: server.transport_type.clone(),
        description: server.description.clone(),
        extra: server.extra.clone(),
        created_at: Default::default(),
        updated_at: Default::default(),
        deleted_at: None,
    }).await.map_err(|e| {
        tracing::error!("Failed to create mcp server {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to create mcp server".to_string(),
        )
    })?;

    let data = serde_json::to_value(res).map_err(|e| {
        tracing::error!("Failed to parse mcp servers {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Internal server error".to_string(),
        )
    })?;

    Ok(Json(Response::new(Some(data))))
}

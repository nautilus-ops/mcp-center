use crate::db::model::{CreateFrom, McpServers, SettingKey};
use crate::db::{McpDBHandler, SystemSettingsDBHandler};
use crate::event::Event;
use crate::service::{AppState, Response};
use axum::Json;
use axum::extract::{Query, State};
use http::StatusCode;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Deserialize, Debug)]
pub struct ListAllRequest {
    use_raw_endpoint: Option<bool>,
}

pub async fn list_all(
    State(state): State<AppState>,
    Query(params): Query<ListAllRequest>,
) -> Result<Json<Response>, (StatusCode, String)> {
    let mcp_handler = McpDBHandler::new(state.db.clone());
    let settings_handler = SystemSettingsDBHandler::new(state.db.clone());

    let self_address = settings_handler
        .get_system_settings(SettingKey::SelfAddress)
        .await;

    let mut servers = mcp_handler.list_all().await.map_err(|e| {
        tracing::error!("Failed to list mcp servers {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to list mcp servers".to_string(),
        )
    })?;

    if params.use_raw_endpoint.is_none() || !params.use_raw_endpoint.unwrap() {
        servers.iter_mut().for_each(|server| {
            server.endpoint = format!(
                "{self_address}/proxy/connect/{}/{}",
                server.name, server.tag
            );
        })
    }

    let data = serde_json::to_value(servers).map_err(|e| {
        tracing::error!("Failed to parse mcp servers {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Internal server error".to_string(),
        )
    })?;

    Ok(Json(Response::new(Some(data))))
}

#[derive(Deserialize, Serialize, Clone)]
pub struct McpRegisterRequest {
    pub name: String,
    pub tag: String,
    pub endpoint: String,
    pub transport_type: String,
    pub description: String,
    pub create_from: Option<String>,
    pub extra: Option<serde_json::Value>,
}

pub async fn register_mcp_server(
    State(state): State<AppState>,
    Json(server): Json<McpRegisterRequest>,
) -> Result<Json<Response>, (StatusCode, String)> {
    let db_client = state.db.clone();
    let res = McpDBHandler::new(db_client)
        .create(&McpServers {
            id: Uuid::new_v4(),
            name: server.name.clone(),
            tag: server.tag.clone(),
            endpoint: server.endpoint.clone(),
            transport_type: server.transport_type.clone(),
            description: server.description.clone(),
            create_from: if server.create_from.is_some() {
                server.create_from.unwrap().clone()
            } else {
                CreateFrom::Register.to_string()
            },
            extra: server.extra.clone(),
            created_at: Default::default(),
            updated_at: Default::default(),
            deleted_at: None,
        })
        .await
        .map_err(|e| {
            tracing::error!("Failed to create mcp server {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to create mcp server".to_string(),
            )
        })?;

    tokio::task::spawn(async move {
        if let Err(err) = state.event_sender.send(Event::CreateOrUpdate {
            mcp_name: server.name.clone(),
            tag: server.tag.clone(),
            endpoint: server.endpoint.clone(),
        }) {
            tracing::error!("Failed to send event {}", err);
        }
        tracing::info!("MCP server {} registered", server.name);
    });

    let data = serde_json::to_value(res).map_err(|e| {
        tracing::error!("Failed to parse mcp servers {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Internal server error".to_string(),
        )
    })?;

    Ok(Json(Response::new(Some(data))))
}

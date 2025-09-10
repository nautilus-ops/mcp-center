use axum::Json;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use mc_common::app::event::Event;
use mc_common::app::{AppState, Response};
use mc_db::model::{CreateFrom, McpServers, SettingKey};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Deserialize, Debug)]
pub struct ListAllRequest {
    use_raw_endpoint: Option<bool>,
    page_size: Option<i64>,
    page_num: Option<i64>,
}

#[derive(Deserialize, Serialize, Debug)]
pub struct ListAllResponse {
    servers: Vec<McpServers>,
    count: i64,
}

pub async fn list_all(
    State(state): State<AppState>,
    Query(request): Query<ListAllRequest>,
) -> Result<Json<Response>, (StatusCode, String)> {
    if request.page_size.is_some() ^ request.page_num.is_some() {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            "Both page_size and page_num must be provided together or omitted together".to_string(),
        ));
    }

    let page_size = request.page_size.unwrap_or(0);
    let page_num = request.page_num.unwrap_or(0);

    let mcp_handler = match &state.handlers().mcp_handler {
        None => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                "Can't get MCP handler not found".to_string(),
            ));
        }
        Some(handler) => handler,
    };

    let settings_handler = match &state.handlers().system_settings_handler {
        None => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                "Can't get MCP handler not found".to_string(),
            ));
        }
        Some(handler) => handler,
    };

    let self_address = settings_handler
        .get_system_settings(SettingKey::SelfAddress)
        .await;

    // select mcp servers
    let mut servers = if page_size > 0 && page_num > 0 {
        mcp_handler
            .list_with_limit(page_size, (page_num - 1) * page_size)
            .await
            .map_err(|e| {
                tracing::error!("Failed to list mcp servers {}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Failed to list mcp servers".to_string(),
                )
            })?
    } else {
        mcp_handler.list_all().await.map_err(|e| {
            tracing::error!("Failed to list mcp servers {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to list mcp servers".to_string(),
            )
        })?
    };

    // replace endpoint host
    if request.use_raw_endpoint.is_none() || !request.use_raw_endpoint.unwrap() {
        servers.iter_mut().for_each(|server| {
            server.endpoint = format!(
                "{self_address}/proxy/connect/{}/{}",
                server.name, server.tag
            );
        })
    }

    let count = mcp_handler.count().await.map_err(|e| {
        tracing::error!("Failed to count mcp servers {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to count mcp servers".to_string(),
        )
    })?;

    let data = serde_json::to_value(ListAllResponse { servers, count }).map_err(|e| {
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
    let mcp_handler = match &state.handlers().mcp_handler {
        None => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                "Can't get MCP handler not found".to_string(),
            ));
        }
        Some(handler) => handler,
    };

    let res = mcp_handler
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
            disabled: Default::default(),
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

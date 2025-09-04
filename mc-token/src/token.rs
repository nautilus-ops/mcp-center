use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use mc_common::app::{AppState, Response};
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Clone)]
pub struct AdminLoginRequest {
    pub username: String,
    pub token: Option<String>,
}

#[derive(Deserialize, Serialize, Clone)]
pub struct AdminLoginResponse {
    pub code: Option<u16>,
    pub message: Option<String>,
}

pub async fn admin_login(
    State(_state): State<AppState>,
    Json(request): Json<AdminLoginRequest>,
) -> Result<Json<Response>, (StatusCode, String)> {
    if request.username != "admin" {
        return Err((
            StatusCode::UNAUTHORIZED,
            String::from("only admin can login"),
        ));
    }
    let env_token = std::env::var("MCP_ADMIN_TOKEN").unwrap_or_else(|e| {
        tracing::warn!("MCP_ADMIN_TOKEN environment variable is not set: {}", e);
        String::from("")
    });

    if let Some(token) = request.token {
        if token != env_token {
            tracing::error!(
                "MCP_ADMIN_TOKEN environment variable is {}, request token is {}",
                env_token,
                token
            );
            return Err((StatusCode::UNAUTHORIZED, String::from("Invalid token")));
        }
        let response = AdminLoginResponse {
            code: Some(u16::from(StatusCode::OK)),
            message: Some(String::from("successfully logged in")),
        };

        let data = serde_json::to_value(response).map_err(|e| {
            tracing::error!("Failed to parse response {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Internal server error".to_string(),
            )
        })?;

        return Ok(Json(Response::new(Some(data))));
    }

    tracing::warn!("only support admin token in current version");
    Err((
        StatusCode::UNAUTHORIZED,
        String::from("only support admin token in current version"),
    ))
}

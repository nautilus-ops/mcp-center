use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use sqlx::types::chrono::NaiveDateTime;
use std::fmt::Display;
use std::str::FromStr;
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize, FromRow)]
pub struct McpServers {
    pub id: Uuid,
    pub name: String,
    pub tag: String,
    pub endpoint: String,
    pub transport_type: String,
    pub description: String,
    pub extra: Option<serde_json::Value>,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
    pub deleted_at: Option<NaiveDateTime>,
}

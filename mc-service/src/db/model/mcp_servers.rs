use std::fmt::Display;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use sqlx::types::chrono::NaiveDateTime;
use std::str::FromStr;
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "transport_type", rename_all = "lowercase")]
pub enum TransportType {
    Sse,
    Streamable,
}

impl FromStr for TransportType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "sse" => Ok(TransportType::Sse),
            "streamable" => Ok(TransportType::Streamable),
            _ => Err(format!("Unknown transport type: {}", s)),
        }
    }
}

impl Display for TransportType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let str = match self {
            TransportType::Sse => "sse".to_string(),
            TransportType::Streamable => "streamable".to_string(),
        };
        write!(f, "{}", str)
    }
}

#[derive(Debug, Serialize, Deserialize, FromRow)]
pub struct McpServers {
    pub id: Uuid,
    pub name: String,
    pub tag: String,
    pub endpoint: String,
    pub transport_type: TransportType,
    pub create_at: NaiveDateTime,
    pub update_at: NaiveDateTime,
    pub delete_at: Option<NaiveDateTime>,
}

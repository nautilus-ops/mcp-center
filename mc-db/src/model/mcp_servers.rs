use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use chrono::NaiveDateTime;
use std::fmt::Display;
use std::str::FromStr;
use uuid::Uuid;

pub enum CreateFrom {
    Manual,
    Register,
    KubernetesService,
}

impl FromStr for CreateFrom {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "manual" => Ok(CreateFrom::Manual),
            "register" => Ok(CreateFrom::Register),
            "kubernetes-service" => Ok(CreateFrom::KubernetesService),
            _ => Ok(CreateFrom::Register),
        }
    }
}

impl Display for CreateFrom {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let str = match self {
            CreateFrom::Manual => "manual".to_string(),
            CreateFrom::Register => "register".to_string(),
            CreateFrom::KubernetesService => "kubernetes-service".to_string(),
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
    pub transport_type: String,
    pub description: String,
    pub create_from: String,
    pub extra: Option<serde_json::Value>,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
    pub deleted_at: Option<NaiveDateTime>,
}

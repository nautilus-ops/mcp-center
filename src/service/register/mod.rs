use serde::Deserialize;
use std::error::Error;

pub mod external_api;
pub mod self_manager;

#[derive(Default, Debug, Deserialize, Clone)]
pub struct McpServer {
    pub endpoint: String,
    pub name: String,
    pub version: Option<String>,
    pub tag: Option<String>,
    #[serde(rename = "isPublished")]
    pub is_published: Option<bool>,
}

#[async_trait::async_trait]
pub trait ListHandler: Send + Sync {
    async fn list_mcp(&self) -> Result<Vec<McpServer>, Box<dyn Error>>;
}

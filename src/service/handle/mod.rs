use std::error::Error;

pub mod external_api;
pub mod self_manager;

#[derive(Default, Debug)]
pub struct McpServer {
    pub transport_type: String,
    pub endpoint: String,
}

#[async_trait::async_trait]
pub trait ListHandler: Send + Sync {
    async fn list_mcp(&self) -> Result<Vec<McpServer>, Box<dyn Error>>;
}

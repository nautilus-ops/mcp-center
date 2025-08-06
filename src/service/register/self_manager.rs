use crate::service::register::{ListHandler, McpServer};
use std::error::Error;

#[derive(Debug, Default)]
pub struct SelfManagerHandler {}

impl SelfManagerHandler {
    pub fn new() -> Self {
        Self {}
    }
}

#[async_trait::async_trait]
impl ListHandler for SelfManagerHandler {
    async fn list_mcp(&self) -> Result<Vec<McpServer>, Box<dyn Error>> {
        Ok(Vec::new())
    }
}

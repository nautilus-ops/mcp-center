use crate::db::DBClient;
use crate::db::model::McpServers;
use std::sync::Arc;

pub struct McpHandler {
    client: Arc<DBClient>,
}
impl McpHandler {
    pub fn new(client: Arc<DBClient>) -> Self {
        McpHandler { client }
    }

    pub async fn list_all(&self) -> Result<Vec<McpServers>, sqlx::Error> {
        Ok(sqlx::query_as::<_, McpServers>("SELECT * FROM tb_mcp_servers")
            .fetch_all(&self.client.pool)
            .await?)
    }
}

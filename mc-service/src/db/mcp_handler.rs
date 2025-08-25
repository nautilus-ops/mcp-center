use crate::db::DBClient;
use crate::db::model::McpServers;
use crate::service::Response;
use axum::Json;
use http::StatusCode;
use std::sync::Arc;

pub struct McpDBHandler {
    client: Arc<DBClient>,
}

impl McpDBHandler {
    pub fn new(client: Arc<DBClient>) -> Self {
        McpDBHandler { client }
    }

    pub async fn list_all(&self) -> Result<Vec<McpServers>, sqlx::Error> {
        Ok(
            sqlx::query_as::<_, McpServers>("SELECT * FROM tb_mcp_servers")
                .fetch_all(&self.client.pool)
                .await?,
        )
    }

    pub async fn create_or_update(&self, server: &McpServers) -> Result<McpServers, sqlx::Error> {
        let res = if server.extra.is_some() {
            sqlx::query_as::<_, McpServers>(
                r#"
        INSERT INTO tb_mcp_servers
            (id, name, tag, endpoint, transport_type, description, extra)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        RETURNING *
        "#,
            )
            .bind(&server.id)
            .bind(&server.name)
            .bind(&server.tag)
            .bind(&server.endpoint)
            .bind(&server.transport_type)
            .bind(&server.description)
            .bind(&server.extra)
            .fetch_one(&self.client.pool)
            .await?
        } else {
            sqlx::query_as::<_, McpServers>(
                r#"
        INSERT INTO tb_mcp_servers
            (id,name, tag, endpoint, transport_type, description)
        VALUES ($1, $2, $3, $4, $5, $6)
        RETURNING *
        "#,
            )
            .bind(&server.id)
            .bind(&server.name)
            .bind(&server.tag)
            .bind(&server.endpoint)
            .bind(&server.transport_type)
            .bind(&server.description)
            .fetch_one(&self.client.pool)
            .await?
        };

        Ok(res)
    }
}

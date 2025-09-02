use crate::{DBClient, model};
use std::sync::Arc;

pub struct ApiKeyDBHandler {
    client: Arc<DBClient>,
}

impl ApiKeyDBHandler {
    pub fn new(client: Arc<DBClient>) -> Self {
        ApiKeyDBHandler { client }
    }

    pub async fn create(&self, name: &str) -> Result<model::ApiKeys, sqlx::Error> {
        let api_key = sqlx::query_as::<_, model::ApiKeys>(
            r#"
        INSERT INTO tb_api_keys
            (name)
        VALUES ($1)
        RETURNING *
        "#,
        )
        .bind(name)
        .fetch_one(&self.client.pool)
        .await?;
        Ok(api_key)
    }

    pub async fn find(&self, api_key: &str) -> Result<model::ApiKeys, sqlx::Error> {
        let api_key =
            sqlx::query_as::<_, model::ApiKeys>(r#"SELECT * FROM tb_api_keys WHERE apikey = $1"#)
                .bind(api_key)
                .fetch_one(&self.client.pool)
                .await?;
        Ok(api_key)
    }
}

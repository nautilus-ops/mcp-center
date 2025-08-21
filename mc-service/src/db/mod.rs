mod mcp_handler;
mod model;
mod token_handler;

use sqlx::postgres::PgPoolOptions;
use std::error::Error;
use std::sync::Arc;
use sqlx::{Pool, Postgres};

pub struct DBClient {
    pub pool: Pool<Postgres>,
}

impl DBClient {
    pub async fn create(
        host: &str,
        username: &str,
        password: &str,
        database: &str,
        max_connections: u32,
    ) -> Result<DBClient, Box<dyn Error>> {
        if max_connections < 1 {
            return Err("Max connections must be greater than 0".into());
        }
        let pool = PgPoolOptions::new()
            .max_connections(max_connections)
            .connect(format!("postgres://{username}:{password}@{host}/{database}").as_str())
            .await?;

        Ok(DBClient {
            pool,
        })
    }
}

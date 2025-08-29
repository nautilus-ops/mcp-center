mod mcp_handler;
pub mod model;
mod settings_handler;
mod token_handler;

pub use mcp_handler::*;
pub use settings_handler::*;

use sqlx::postgres::PgPoolOptions;
use sqlx::{Pool, Postgres};
use std::error::Error;

#[derive(Clone)]
pub struct DBClient {
    pub pool: Pool<Postgres>,
}

impl DBClient {
    pub async fn create(
        host: &str,
        port: u16,
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
            .connect(format!("postgres://{username}:{password}@{host}:{port}/{database}").as_str())
            .await?;

        Ok(DBClient { pool })
    }
}

use async_trait::async_trait;
use std::error::Error;

pub mod manager;

pub struct SessionInfo {
    pub name: String,
    pub tag: String,
}

#[async_trait]
pub trait Manager: Send + Sync {
    async fn load(&self, session_id: &str) -> Result<SessionInfo, Box<dyn Error>>;
    async fn save(&self, session_id: &str, info: SessionInfo) -> Result<(), Box<dyn Error>>;
}

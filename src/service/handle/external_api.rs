use crate::service::handle::{ListHandler, McpServer};
use async_trait::async_trait;
use reqwest::Client;
use std::error::Error;

#[derive(Debug,Default)]
pub struct ExternalApiHandler {
    url: String,
    authorization: Option<String>,
}

impl ExternalApiHandler {
    pub fn new(url: &str, authorization: Option<String>) -> Self {
        Self {
            url: url.to_string(),
            authorization,
        }
    }
}

#[async_trait::async_trait]
impl ListHandler for ExternalApiHandler {
    async fn list_mcp(&self) -> Result<Vec<McpServer>, Box<dyn Error>> {
        let client = Client::new();
        let mut builder = client.get(self.url.as_str());
        if let Some(auth) = &self.authorization {
            builder = builder.header("Authorization", auth.as_str());
        }
        let response = builder.send().await?;
        tracing::info!("ExternalApiHandler: {:?}", response.text().await?);
        Ok(Vec::new())
    }
}

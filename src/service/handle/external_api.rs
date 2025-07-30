use crate::service::handle::{ListHandler, McpServer};
use reqwest::Client;
use serde::Deserialize;
use std::error::Error;

#[derive(Debug, Default)]
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
        let raw = response.text().await?;
        // tracing::info!("ExternalApiHandler: {:?}", );

        let res: ListResponse = serde_json::from_str(raw.as_str())?;
        tracing::info!("list mcp number: {}", res.data.list.len());

        Ok(res.data.list)
    }
}

#[derive(Debug, Deserialize)]
pub struct ListResponse {
    pub data: ListData,
}

#[derive(Debug, Deserialize)]
pub struct ListData {
    pub list: Vec<McpServer>,
}

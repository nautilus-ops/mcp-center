use crate::{McpServer, Loader};
use serde::Deserialize;
use std::error::Error;
use std::fs;

#[derive(Deserialize, Default, Clone)]
struct McpServers {
    mcp_servers: Vec<McpServer>,
}

#[derive(Debug, Default, Clone)]
pub struct LocalFileLoader {
    mcp_servers: Vec<McpServer>,
}

impl LocalFileLoader {
    pub fn new(path: String) -> Self {
        let mut registry = Self {
            mcp_servers: Vec::new(),
        };
        let mcp_servers = load(path.as_str());
        registry.mcp_servers = mcp_servers;
        registry
    }
}

#[async_trait::async_trait]
impl Loader for LocalFileLoader {
    async fn list_mcp(&self) -> Result<Vec<McpServer>, Box<dyn Error>> {
        Ok(self.mcp_servers.clone())
    }
}

fn load(path: &str) -> Vec<McpServer> {
    let content = fs::read_to_string(path)
        .map_err(|e| {
            tracing::error!("Failed to read config file {}: {}", path, e);
            e
        })
        .unwrap_or_default();

    let servers: McpServers = toml::from_str(&content)
        .map_err(|e| {
            tracing::error!("Failed to parse TOML config: {}", e);
            e
        })
        .unwrap_or_default();

    tracing::info!("Loaded config file {}", path);
    tracing::info!("Loaded servers {:?}", servers.mcp_servers);

    servers.mcp_servers.clone()
}

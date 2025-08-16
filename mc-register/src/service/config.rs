use serde::{Deserialize, Serialize};

#[derive(Default,Debug,Deserialize,Clone)]
pub(crate) struct AppConfig {
    #[serde(default)]
    pub mcp_registry: McpRegistry
}

#[derive(Default,Debug,Deserialize,Clone)]
pub(crate) struct McpRegistry {
    #[serde(default)]
    pub http_port: u16,
}
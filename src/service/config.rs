use serde::Deserialize;

#[derive(Deserialize, Debug, Clone, Default)]
pub struct AppConfig {
    #[serde(default)]
    pub mcp_center: McpCenter,
    #[serde(default)]
    pub mcp_registry: McpRegistry,
    #[serde(default)]
    pub session_manager: SessionManager,
}

#[derive(Deserialize, Debug, Clone, Default)]
pub struct McpCenter {
    #[serde(default)]
    pub http_port: u16,
    #[serde(default)]
    pub grpc_port: u16, // TODO support grpc
}

#[derive(Deserialize, Debug, Clone, Default)]
#[serde(tag = "type")]
pub enum McpRegistry {
    #[default]
    #[serde(rename = "memory")]
    LocalMemory,
    #[serde(rename = "external")]
    External { url: String, token: Option<String> },
}

#[derive(Deserialize, Debug, Clone, Default)]
pub struct SessionManager {
    #[serde(default)]
    pub expiration: u64
}
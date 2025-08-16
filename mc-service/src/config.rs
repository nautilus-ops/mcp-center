use serde::Deserialize;

#[derive(Deserialize, Debug, Clone, Default)]
pub struct AppConfig {
    #[serde(default)]
    pub mcp_center: McpCenter,
    #[serde(default)]
    pub mcp_registry: McpRegistry,
}

#[derive(Deserialize, Debug, Clone, Default)]
pub struct McpCenter {
    #[serde(default)]
    pub http_port: u16,
    #[serde(default)]
    pub cache_reflash_interval: u64,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(tag = "type")]
pub enum McpRegistry {
    #[serde(rename = "memory")]
    LocalMemory { mcp_definition_path: String },
    #[serde(rename = "external")]
    External { url: String, token: Option<String> },
}

impl Default for McpRegistry {
    fn default() -> Self {
        McpRegistry::LocalMemory {
            mcp_definition_path: "mcp_servers".to_string(),
        }
    }
}

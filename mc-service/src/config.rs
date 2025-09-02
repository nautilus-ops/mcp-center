use serde::Deserialize;

#[derive(Deserialize, Debug, Clone, Default)]
pub struct AppConfig {
    #[serde(default)]
    pub mcp_center: McpCenter,
    #[serde(default)]
    pub mcp_registry: McpRegistry,
    #[serde(default)]
    pub postgres: Postgres,
}

#[derive(Deserialize, Debug, Clone, Default)]
pub struct McpCenter {
    #[serde(default)]
    pub http_port: u16,
    #[serde(default)]
    pub admin_token: String,
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

#[derive(Deserialize, Debug, Clone, Default)]
pub struct Postgres {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: String,
    pub database: String,
    pub max_connection: u32,
}

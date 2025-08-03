use serde::Deserialize;

#[derive(Deserialize, Debug, Clone)]
pub struct McpCenter {
    #[serde(default)]
    pub port: u16,
}
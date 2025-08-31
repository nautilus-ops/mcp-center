use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use std::fmt::Display;

#[derive(Debug, Serialize, Deserialize, FromRow)]
pub struct SystemSettings {
    pub setting_name: String,
    pub setting_value: String,
}

pub enum SettingKey {
    SelfAddress,
}

impl Display for SettingKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                SettingKey::SelfAddress => "SELF_ADDRESS".to_string(),
            }
        )
    }
}

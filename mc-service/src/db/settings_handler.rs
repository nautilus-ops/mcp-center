use crate::db::DBClient;
use crate::db::model::{SettingKey, SystemSettings};
use std::sync::Arc;

pub struct SystemSettingsDBHandler {
    client: Arc<DBClient>,
}

impl SystemSettingsDBHandler {
    pub fn new(client: Arc<DBClient>) -> Self {
        SystemSettingsDBHandler { client }
    }

    pub async fn get_system_settings(&self, key: SettingKey) -> String {
        if let Ok(settings) = sqlx::query_as::<_, SystemSettings>(
            "SELECT * FROM tb_system_settings where setting_name = $1",
        )
        .bind(key.to_string())
        .fetch_one(&self.client.pool)
        .await
        {
            settings.setting_value.trim_end_matches('/').to_string()
        } else {
            String::from("http://127.0.0.1")
        }
    }
}

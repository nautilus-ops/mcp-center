use crate::service::session::{Manager, SessionInfo};
use async_trait::async_trait;
use std::error::Error;
use std::fs;
use std::path::PathBuf;
use tokio::fs::OpenOptions;
use tokio::io::AsyncWriteExt;

const SESSION_DIR_PATH: &str = "sessions";

#[derive(Clone, Debug)]
pub struct LocalManager {}

impl LocalManager {
    pub fn new() -> Self {
        fs::create_dir_all(SESSION_DIR_PATH).unwrap();
        Self {}
    }
}

impl Default for LocalManager {
    fn default() -> Self {
        Self::new()
    }
}
#[async_trait]
impl Manager for LocalManager {
    async fn load(&self, session_id: &str) -> Result<SessionInfo, Box<dyn Error>> {
        let dir_path = build_path(session_id);

        let content = tokio::fs::read_to_string(dir_path).await?;
        let split = content.split(" ").collect::<Vec<&str>>();
        if split.len() != 2 {
            return Err("[LocalManager] Invalid input".into());
        }
        Ok(SessionInfo {
            name: split.get(0).unwrap_or(&"").to_string(),
            tag: split.get(1).unwrap_or(&"").to_string(),
        })
    }

    async fn save(&self, session_id: &str, info: SessionInfo) -> Result<(), Box<dyn Error>> {
        let dir_path = build_path(session_id);

        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(dir_path)
            .await?;
        file.write_all(format!("{} {}", info.name, info.tag).as_bytes())
            .await?;
        Ok(())
    }
}

fn build_path(session_id: &str) -> String {
    let mut path = PathBuf::from(SESSION_DIR_PATH);
    path.push(session_id);
    path.display().to_string()
}

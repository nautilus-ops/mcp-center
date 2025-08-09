use crate::service::config::SessionManager;
use crate::service::session::{Manager, ManagerError, SessionInfo};
use cached::{Cached, TimedSizedCache};
use std::error::Error;
use std::fs;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

const SESSION_DIR_PATH: &str = "sessions";

#[derive(Clone)]
pub struct LocalManager {
    cache: Arc<Mutex<TimedSizedCache<String, SessionInfo>>>,
}

impl LocalManager {
    pub fn new(config: SessionManager) -> Self {
        fs::create_dir_all(SESSION_DIR_PATH).unwrap();

        let duration = Duration::from_secs(config.expiration);

        let cache = Arc::new(Mutex::new(TimedSizedCache::with_size_and_lifespan(
            100,
            duration.clone(),
        )));
        let sessions = load_sessions(duration.clone()).unwrap();

        {
            let mut c = cache.lock().unwrap();
            for (id, info) in sessions {
                c.cache_set(id, info.clone());
            }
        }

        Self { cache }
    }
}

impl Manager for LocalManager {
    fn load(&self, session_id: &str) -> Result<SessionInfo, ManagerError> {
        {
            let mut cache = self.cache.lock().unwrap();
            if let Some(info) = cache.cache_get(session_id) {
                tracing::info!("loaded session from cache {}", session_id);
                return Ok(info.clone());
            }
        }

        let dir_path = build_path(session_id);

        let content =
            fs::read_to_string(dir_path).map_err(|e| ManagerError::new(e.to_string().as_str()))?;

        parse_content(&content).map_err(|e| ManagerError::new(e.to_string().as_str()))
    }

    fn save(&self, session_id: &str, info: SessionInfo) -> Result<(), ManagerError> {
        {
            let mut cache = self.cache.lock().unwrap();
            cache.cache_set(session_id.to_string(), info.clone());
            tracing::info!("saved session {} to {}", session_id, info.name);
        }

        let dir_path = build_path(session_id);

        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(dir_path)
            .map_err(|e| ManagerError::new(e.to_string().as_str()))?;

        file.write_all(
            format!("{} {} {} {}", info.name, info.tag, info.scheme, info.host).as_bytes(),
        )
        .map_err(|e| ManagerError::new(e.to_string().as_str()))?;
        Ok(())
    }
}

fn build_path(session_id: &str) -> String {
    let mut path = PathBuf::from(SESSION_DIR_PATH);
    path.push(session_id);
    path.display().to_string()
}

fn parse_content(content: &str) -> Result<SessionInfo, Box<dyn Error>> {
    let split = content.split(" ").collect::<Vec<&str>>();
    if split.len() != 4 {
        return Err("[LocalManager] Invalid input".into());
    }
    Ok(SessionInfo {
        name: split.get(0).unwrap_or(&"").to_string(),
        tag: split.get(1).unwrap_or(&"").to_string(),
        scheme: split.get(2).unwrap_or(&"").to_string(),
        host: split.get(3).unwrap_or(&"").to_string(),
    })
}

fn load_sessions(expiration: Duration) -> Result<Vec<(String, SessionInfo)>, Box<dyn Error>> {
    let mut res = Vec::new();
    for entry in fs::read_dir(SESSION_DIR_PATH)? {
        let entry = entry?;
        if let Ok(metadata) = entry.metadata() {
            let time = metadata.modified()?;
            let now = SystemTime::now();
            // If the session was created more than expiration, clean it up.
            if now.duration_since(time).unwrap_or(Duration::MAX) > expiration {
                clean_up_session(entry.path())?;
                continue;
            };

            if entry.file_type()?.is_dir() {
                continue;
            }

            let content = fs::read_to_string(&entry.path())?;

            let session_id = entry.file_name().to_string_lossy().into_owned();
            let info = parse_content(&content)?;

            tracing::info!("loaded session {}", session_id);

            res.push((session_id, info));
        }
    }

    Ok(res)
}

fn clean_up_session(path: PathBuf) -> Result<(), Box<dyn Error>> {
    fs::remove_file(path)?;
    Ok(())
}

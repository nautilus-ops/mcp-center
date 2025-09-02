use std::sync::Arc;
use crate::DBClient;

pub struct ApiKeyDBHandler {
    client: Arc<DBClient>,
}

impl ApiKeyDBHandler {
    pub fn new(client: Arc<DBClient>) -> Self {
        ApiKeyDBHandler { client }
    }
}
#[derive(Clone)]
pub enum Event {
    #[allow(dead_code)]
    Delete { mcp_name: String, tag: String },
    CreateOrUpdate {
        mcp_name: String,
        tag: String,
        endpoint: String,
    },
}

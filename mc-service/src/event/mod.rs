#[derive(Clone)]
pub enum Event {
    Delete {
        mcp_name: String,
        tag: String,
    },
    CreateOrUpdate {
        mcp_name: String,
        tag: String,
        endpoint: String,
    },
}

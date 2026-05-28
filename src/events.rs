use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
pub struct GatewayEvent {
    pub id: String,
    pub platform: String,
    pub event_type: String,
    pub channel_id: Option<String>,
    pub author: Option<EventAuthor>,
    pub content: Option<String>,
    pub timestamp: String,
    pub raw: serde_json::Value,
}

#[derive(Clone, Debug, Serialize)]
pub struct EventAuthor {
    pub id: String,
    pub username: String,
    pub display_name: Option<String>,
    pub is_bot: bool,
}

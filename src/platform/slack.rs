use async_trait::async_trait;
use reqwest::Client;
use serde_json::{json, Value};
use std::time::Duration;

use super::{Platform, PlatformError, PlatformResult};

const BASE: &str = "https://slack.com/api";

pub struct Slack {
    client: Client,
}

impl Slack {
    pub fn from_env() -> Self {
        let token = std::env::var("SLACK_BOT_TOKEN")
            .expect("SLACK_BOT_TOKEN must be set when PLATFORM=slack");

        let client = Client::builder()
            .timeout(Duration::from_secs(15))
            .default_headers({
                let mut h = reqwest::header::HeaderMap::new();
                h.insert(
                    reqwest::header::AUTHORIZATION,
                    format!("Bearer {token}").parse().unwrap(),
                );
                h.insert(
                    reqwest::header::CONTENT_TYPE,
                    "application/json".parse().unwrap(),
                );
                h
            })
            .build()
            .expect("Failed to build reqwest client");

        Self { client }
    }

    async fn post(&self, method: &str, body: Value) -> PlatformResult {
        let url = format!("{BASE}/{method}");
        let resp = self.client.post(&url).json(&body).send().await.map_err(|e| {
            PlatformError {
                status: 502,
                message: format!("Slack request failed: {e}"),
            }
        })?;

        let status = resp.status().as_u16();
        let data: Value = resp.json().await.map_err(|e| PlatformError {
            status: 502,
            message: format!("Invalid JSON from Slack: {e}"),
        })?;

        if !(200..300).contains(&status) {
            return Err(PlatformError {
                status,
                message: data.to_string(),
            });
        }

        // Slack returns 200 even on errors — check the `ok` field.
        if data["ok"].as_bool() != Some(true) {
            let err = data["error"].as_str().unwrap_or("unknown_error");
            return Err(PlatformError {
                status: 400,
                message: err.to_string(),
            });
        }

        Ok(data)
    }
}

#[async_trait]
impl Platform for Slack {
    async fn send_message(
        &self,
        channel_id: &str,
        content: &str,
        reply_to: Option<&str>,
    ) -> PlatformResult {
        let mut body = json!({
            "channel": channel_id,
            "text": content
        });
        if let Some(ts) = reply_to {
            body["thread_ts"] = json!(ts);
        }
        self.post("chat.postMessage", body).await
    }

    async fn edit_message(
        &self,
        channel_id: &str,
        message_id: &str,
        content: &str,
    ) -> PlatformResult {
        self.post(
            "chat.update",
            json!({
                "channel": channel_id,
                "ts": message_id,
                "text": content
            }),
        )
        .await
    }

    async fn add_reaction(
        &self,
        channel_id: &str,
        message_id: &str,
        emoji: &str,
    ) -> PlatformResult {
        // Slack emoji names don't include colons — strip them if present.
        let name = emoji.trim_matches(':');
        self.post(
            "reactions.add",
            json!({
                "channel": channel_id,
                "timestamp": message_id,
                "name": name
            }),
        )
        .await
    }

    async fn get_messages(&self, channel_id: &str, limit: u64) -> PlatformResult {
        self.post(
            "conversations.history",
            json!({
                "channel": channel_id,
                "limit": limit
            }),
        )
        .await
    }

    async fn get_channel_info(&self, channel_id: &str) -> PlatformResult {
        self.post(
            "conversations.info",
            json!({ "channel": channel_id }),
        )
        .await
    }
}

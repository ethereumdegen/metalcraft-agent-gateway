use async_trait::async_trait;
use reqwest::Client;
use serde_json::{json, Value};
use std::time::Duration;

use super::{Platform, PlatformError, PlatformResult};

const BASE: &str = "https://discord.com/api/v10";

pub struct Discord {
    client: Client,
}

impl Discord {
    pub fn from_env() -> Self {
        let token = std::env::var("DISCORD_BOT_TOKEN")
            .expect("DISCORD_BOT_TOKEN must be set when PLATFORM=discord");

        let client = Client::builder()
            .timeout(Duration::from_secs(15))
            .user_agent("DiscordBot (metalcraft-agent-gateway, 0.1)")
            .default_headers({
                let mut h = reqwest::header::HeaderMap::new();
                h.insert(
                    reqwest::header::AUTHORIZATION,
                    format!("Bot {token}").parse().unwrap(),
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

    async fn request(
        &self,
        method: reqwest::Method,
        path: &str,
        body: Option<Value>,
    ) -> PlatformResult {
        let url = format!("{BASE}{path}");
        let mut req = self.client.request(method, &url);
        if let Some(b) = body {
            req = req.json(&b);
        }

        let resp = req.send().await.map_err(|e| PlatformError {
            status: 502,
            message: format!("Discord request failed: {e}"),
        })?;

        let status = resp.status().as_u16();
        let text = resp.text().await.unwrap_or_default();

        if !(200..300).contains(&status) {
            return Err(PlatformError {
                status,
                message: text,
            });
        }

        if text.is_empty() {
            Ok(json!({ "ok": true }))
        } else {
            serde_json::from_str(&text).map_err(|_| PlatformError {
                status: 502,
                message: format!("Invalid JSON from Discord: {text}"),
            })
        }
    }
}

#[async_trait]
impl Platform for Discord {
    async fn send_message(
        &self,
        channel_id: &str,
        content: &str,
        reply_to: Option<&str>,
    ) -> PlatformResult {
        let mut body = json!({ "content": content });
        if let Some(ref_id) = reply_to {
            body["message_reference"] = json!({ "message_id": ref_id });
        }
        self.request(
            reqwest::Method::POST,
            &format!("/channels/{channel_id}/messages"),
            Some(body),
        )
        .await
    }

    async fn edit_message(
        &self,
        channel_id: &str,
        message_id: &str,
        content: &str,
    ) -> PlatformResult {
        self.request(
            reqwest::Method::PATCH,
            &format!("/channels/{channel_id}/messages/{message_id}"),
            Some(json!({ "content": content })),
        )
        .await
    }

    async fn add_reaction(
        &self,
        channel_id: &str,
        message_id: &str,
        emoji: &str,
    ) -> PlatformResult {
        self.request(
            reqwest::Method::PUT,
            &format!("/channels/{channel_id}/messages/{message_id}/reactions/{emoji}/@me"),
            None,
        )
        .await
    }

    async fn get_messages(&self, channel_id: &str, limit: u64) -> PlatformResult {
        self.request(
            reqwest::Method::GET,
            &format!("/channels/{channel_id}/messages?limit={limit}"),
            None,
        )
        .await
    }

    async fn get_channel_info(&self, channel_id: &str) -> PlatformResult {
        self.request(
            reqwest::Method::GET,
            &format!("/channels/{channel_id}"),
            None,
        )
        .await
    }
}

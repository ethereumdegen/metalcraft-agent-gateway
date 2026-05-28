use serenity::async_trait;
use serenity::model::channel::{Message, Reaction};
use serenity::model::gateway::Ready;
use serenity::prelude::*;
use tokio::sync::broadcast;

use crate::events::{EventAuthor, GatewayEvent};

struct DiscordHandler {
    event_tx: broadcast::Sender<GatewayEvent>,
}

#[async_trait]
impl EventHandler for DiscordHandler {
    async fn message(&self, _ctx: Context, msg: Message) {
        let event = GatewayEvent {
            id: uuid::Uuid::new_v4().to_string(),
            platform: "discord".into(),
            event_type: "message_create".into(),
            channel_id: Some(msg.channel_id.to_string()),
            author: Some(EventAuthor {
                id: msg.author.id.to_string(),
                username: msg.author.name.clone(),
                display_name: msg.author.global_name.clone(),
                is_bot: msg.author.bot,
            }),
            content: Some(msg.content.clone()),
            timestamp: msg.timestamp.to_rfc3339().unwrap_or_else(|| chrono::Utc::now().to_rfc3339()),
            raw: serde_json::to_value(&msg).unwrap_or_default(),
        };

        if let Err(e) = self.event_tx.send(event) {
            tracing::debug!("No subscribers for discord message event: {e}");
        }
    }

    async fn reaction_add(&self, ctx: Context, reaction: Reaction) {
        let emoji_str = reaction.emoji.to_string();

        // Try to resolve user info to detect bots
        let author = if let Some(uid) = reaction.user_id {
            let (username, is_bot) = match uid.to_user(&ctx).await {
                Ok(user) => (user.name.clone(), user.bot),
                Err(_) => (String::new(), false),
            };
            Some(EventAuthor {
                id: uid.to_string(),
                username,
                display_name: None,
                is_bot,
            })
        } else {
            None
        };

        let event = GatewayEvent {
            id: uuid::Uuid::new_v4().to_string(),
            platform: "discord".into(),
            event_type: "reaction_add".into(),
            channel_id: Some(reaction.channel_id.to_string()),
            author,
            content: Some(emoji_str),
            timestamp: chrono::Utc::now().to_rfc3339(),
            raw: serde_json::to_value(&reaction).unwrap_or_default(),
        };

        if let Err(e) = self.event_tx.send(event) {
            tracing::debug!("No subscribers for discord reaction event: {e}");
        }
    }

    async fn ready(&self, _ctx: Context, ready: Ready) {
        tracing::info!("Discord listener connected as {}", ready.user.name);
    }
}

/// Start the serenity client for inbound Discord events with auto-reconnect.
/// This function blocks indefinitely — call via `tokio::spawn`.
pub async fn start(token: String, event_tx: broadcast::Sender<GatewayEvent>) {
    let intents = GatewayIntents::GUILD_MESSAGES
        | GatewayIntents::MESSAGE_CONTENT
        | GatewayIntents::GUILD_MESSAGE_REACTIONS;

    loop {
        let mut client = match Client::builder(&token, intents)
            .event_handler(DiscordHandler {
                event_tx: event_tx.clone(),
            })
            .await
        {
            Ok(c) => c,
            Err(e) => {
                tracing::error!("Failed to create Discord listener client: {e}");
                tokio::time::sleep(std::time::Duration::from_secs(10)).await;
                continue;
            }
        };

        tracing::info!("Starting Discord listener...");
        if let Err(e) = client.start().await {
            tracing::error!("Discord listener disconnected: {e}, reconnecting in 10s...");
        } else {
            tracing::warn!("Discord listener stopped unexpectedly, reconnecting in 10s...");
        }

        tokio::time::sleep(std::time::Duration::from_secs(10)).await;
    }
}

use std::sync::Arc;

use axum::{
    Router,
    body::Bytes,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::post,
    Json,
};
use hmac::{Hmac, Mac};
use sha2::Sha256;
use serde_json::{json, Value};

use crate::AppState;
use crate::events::{EventAuthor, GatewayEvent};

type S = Arc<AppState>;

pub fn router() -> Router<S> {
    Router::new()
        .route("/slack", post(slack_webhook))
        .route("/github", post(github_webhook))
}

// ── Slack Webhook ───────────────────────────────────────────────────────

async fn slack_webhook(
    State(state): State<S>,
    Json(body): Json<Value>,
) -> impl IntoResponse {
    // Handle url_verification challenge
    if body.get("type").and_then(|t| t.as_str()) == Some("url_verification") {
        let challenge = body
            .get("challenge")
            .and_then(|c| c.as_str())
            .unwrap_or_default();
        return (StatusCode::OK, Json(json!({ "challenge": challenge }))).into_response();
    }

    // Process event callback
    if let Some(event) = body.get("event") {
        let slack_type = event
            .get("type")
            .and_then(|t| t.as_str())
            .unwrap_or("unknown");

        let event_type = match slack_type {
            "message" => "message_create",
            "reaction_added" => "reaction_add",
            "reaction_removed" => "reaction_remove",
            "app_mention" => "app_mention",
            other => other,
        };

        let channel_id = event.get("channel").and_then(|c| c.as_str()).map(String::from);
        let text = event.get("text").and_then(|t| t.as_str()).map(String::from);
        let user_id = event.get("user").and_then(|u| u.as_str()).map(String::from);

        let author = user_id.map(|uid| EventAuthor {
            id: uid,
            username: String::new(),
            display_name: None,
            is_bot: event
                .get("bot_id")
                .map(|_| true)
                .unwrap_or(false),
        });

        let gateway_event = GatewayEvent {
            id: uuid::Uuid::new_v4().to_string(),
            platform: "slack".into(),
            event_type: event_type.into(),
            channel_id,
            author,
            content: text,
            timestamp: event
                .get("ts")
                .and_then(|t| t.as_str())
                .map(String::from)
                .unwrap_or_else(|| chrono::Utc::now().to_rfc3339()),
            raw: body.clone(),
        };

        let _ = state.event_tx.send(gateway_event);
    }

    (StatusCode::OK, Json(json!({ "ok": true }))).into_response()
}

// ── GitHub Webhook ──────────────────────────────────────────────────────

async fn github_webhook(
    State(state): State<S>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    // Verify HMAC signature if secret is configured
    if let Ok(secret) = std::env::var("GITHUB_WEBHOOK_SECRET") {
        if !secret.is_empty() {
            let signature = headers
                .get("x-hub-signature-256")
                .and_then(|v| v.to_str().ok())
                .unwrap_or_default();

            if !verify_github_signature(&secret, &body, signature) {
                return (
                    StatusCode::UNAUTHORIZED,
                    Json(json!({ "error": "Invalid signature" })),
                );
            }
        }
    }

    let gh_event = headers
        .get("x-github-event")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown");

    let payload: Value = serde_json::from_slice(&body).unwrap_or_default();

    let action = payload
        .get("action")
        .and_then(|a| a.as_str())
        .unwrap_or_default();

    let event_type = match (gh_event, action) {
        ("pull_request", "opened") => "pr_opened",
        ("pull_request", "closed") => {
            if payload
                .get("pull_request")
                .and_then(|pr| pr.get("merged"))
                .and_then(|m| m.as_bool())
                .unwrap_or(false)
            {
                "pr_merged"
            } else {
                "pr_closed"
            }
        }
        ("pull_request", action) => action,
        ("issue_comment", _) => "issue_comment",
        ("push", _) => "push",
        ("issues", "opened") => "issue_opened",
        ("issues", "closed") => "issue_closed",
        (other, _) => other,
    };

    // Extract author info from sender
    let author = payload.get("sender").map(|sender| EventAuthor {
        id: sender
            .get("id")
            .and_then(|id| id.as_u64())
            .map(|id| id.to_string())
            .unwrap_or_default(),
        username: sender
            .get("login")
            .and_then(|l| l.as_str())
            .unwrap_or_default()
            .to_string(),
        display_name: None,
        is_bot: sender
            .get("type")
            .and_then(|t| t.as_str())
            .map(|t| t == "Bot")
            .unwrap_or(false),
    });

    // Extract repo as channel_id
    let channel_id = payload
        .get("repository")
        .and_then(|r| r.get("full_name"))
        .and_then(|n| n.as_str())
        .map(String::from);

    let gateway_event = GatewayEvent {
        id: uuid::Uuid::new_v4().to_string(),
        platform: "github".into(),
        event_type: event_type.into(),
        channel_id,
        author,
        content: None,
        timestamp: chrono::Utc::now().to_rfc3339(),
        raw: payload,
    };

    let _ = state.event_tx.send(gateway_event);

    (StatusCode::OK, Json(json!({ "ok": true })))
}

fn verify_github_signature(secret: &str, body: &[u8], signature: &str) -> bool {
    let sig_hex = match signature.strip_prefix("sha256=") {
        Some(hex) => hex,
        None => return false,
    };

    let sig_bytes = match hex::decode(sig_hex) {
        Ok(bytes) => bytes,
        Err(_) => return false,
    };

    let mut mac =
        Hmac::<Sha256>::new_from_slice(secret.as_bytes()).expect("HMAC can take key of any size");
    mac.update(body);

    mac.verify_slice(&sig_bytes).is_ok()
}

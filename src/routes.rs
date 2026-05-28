use std::sync::Arc;

use axum::{
    Router,
    extract::{Path, Query, State},
    http::StatusCode,
    middleware,
    response::IntoResponse,
    routing::{get, patch, post, put},
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::{AppState, auth, platform::PlatformError};

type S = Arc<AppState>;

pub fn router() -> Router<S> {
    Router::new()
        .route("/messages", post(send_message))
        .route("/messages/{message_id}", patch(edit_message))
        .route(
            "/messages/{message_id}/reactions",
            put(add_reaction),
        )
        .route(
            "/channels/{channel_id}/messages",
            get(get_messages),
        )
        .route("/channels/{channel_id}", get(get_channel_info))
        .layer(middleware::from_fn(auth::require_api_key))
}

fn platform_err(e: PlatformError) -> impl IntoResponse {
    (
        StatusCode::from_u16(e.status).unwrap_or(StatusCode::BAD_GATEWAY),
        Json(json!({ "error": e.message })),
    )
}

// ── POST /messages ──────────────────────────────────────────────────────

#[derive(Deserialize)]
struct SendMessageBody {
    channel_id: String,
    content: String,
    message_reference_id: Option<String>,
}

async fn send_message(
    State(state): State<S>,
    Json(body): Json<SendMessageBody>,
) -> Result<Json<Value>, impl IntoResponse> {
    state
        .platform
        .send_message(
            &body.channel_id,
            &body.content,
            body.message_reference_id.as_deref(),
        )
        .await
        .map(Json)
        .map_err(platform_err)
}

// ── PATCH /messages/:message_id ─────────────────────────────────────────

#[derive(Deserialize)]
struct EditMessageBody {
    channel_id: String,
    content: String,
}

async fn edit_message(
    State(state): State<S>,
    Path(message_id): Path<String>,
    Json(body): Json<EditMessageBody>,
) -> Result<Json<Value>, impl IntoResponse> {
    state
        .platform
        .edit_message(&body.channel_id, &message_id, &body.content)
        .await
        .map(Json)
        .map_err(platform_err)
}

// ── PUT /messages/:message_id/reactions ──────────────────────────────────

#[derive(Deserialize)]
struct AddReactionBody {
    channel_id: String,
    emoji: String,
}

async fn add_reaction(
    State(state): State<S>,
    Path(message_id): Path<String>,
    Json(body): Json<AddReactionBody>,
) -> Result<Json<Value>, impl IntoResponse> {
    state
        .platform
        .add_reaction(&body.channel_id, &message_id, &body.emoji)
        .await
        .map(Json)
        .map_err(platform_err)
}

// ── GET /channels/:channel_id/messages?limit=N ──────────────────────────

#[derive(Deserialize)]
struct GetMessagesQuery {
    limit: Option<u64>,
}

async fn get_messages(
    State(state): State<S>,
    Path(channel_id): Path<String>,
    Query(q): Query<GetMessagesQuery>,
) -> Result<Json<Value>, impl IntoResponse> {
    let limit = q.limit.unwrap_or(10).min(50);
    state
        .platform
        .get_messages(&channel_id, limit)
        .await
        .map(Json)
        .map_err(platform_err)
}

// ── GET /channels/:channel_id ───────────────────────────────────────────

async fn get_channel_info(
    State(state): State<S>,
    Path(channel_id): Path<String>,
) -> Result<Json<Value>, impl IntoResponse> {
    state
        .platform
        .get_channel_info(&channel_id)
        .await
        .map(Json)
        .map_err(platform_err)
}

mod auth;
mod discord_listener;
mod events;
mod platform;
mod routes;
mod subscribers;
mod webhooks;

use std::collections::HashMap;
use std::sync::Arc;
use axum::Router;
use tokio::sync::broadcast;
use tower_http::trace::TraceLayer;

use events::GatewayEvent;
use subscribers::SubscriberStore;

pub struct AppState {
    pub platforms: HashMap<String, Box<dyn platform::Platform>>,
    pub default_platform: Option<String>,
    pub event_tx: broadcast::Sender<GatewayEvent>,
    pub subscriber_store: SubscriberStore,
}

impl AppState {
    /// Resolve which platform to use: explicit field > default > error.
    pub fn resolve(&self, requested: Option<&str>) -> Result<&dyn platform::Platform, platform::PlatformError> {
        let name = requested
            .or(self.default_platform.as_deref())
            .ok_or_else(|| platform::PlatformError {
                status: 400,
                message: "No 'platform' field in request and no default PLATFORM configured".into(),
            })?;

        self.platforms.get(name).map(|b| b.as_ref()).ok_or_else(|| platform::PlatformError {
            status: 400,
            message: format!("Platform '{name}' is not configured (missing token?)"),
        })
    }
}

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();

    let mut platforms: HashMap<String, Box<dyn platform::Platform>> = HashMap::new();

    // Register each platform whose token is present.
    if std::env::var("DISCORD_BOT_TOKEN").is_ok() {
        tracing::info!("Discord platform enabled");
        platforms.insert("discord".into(), Box::new(platform::discord::Discord::from_env()));
    }
    if std::env::var("SLACK_BOT_TOKEN").is_ok() {
        tracing::info!("Slack platform enabled");
        platforms.insert("slack".into(), Box::new(platform::slack::Slack::from_env()));
    }

    if platforms.is_empty() {
        tracing::error!("No platform tokens configured. Set DISCORD_BOT_TOKEN and/or SLACK_BOT_TOKEN.");
        std::process::exit(1);
    }

    // PLATFORM env var sets the default (used when requests omit the field).
    let default_platform = std::env::var("PLATFORM").ok().filter(|p| platforms.contains_key(p));

    // If only one platform is configured, default to it automatically.
    let default_platform = default_platform.or_else(|| {
        if platforms.len() == 1 {
            platforms.keys().next().cloned()
        } else {
            None
        }
    });

    if let Some(ref dp) = default_platform {
        tracing::info!("Default platform: {dp}");
    } else {
        tracing::info!("No default platform — requests must include 'platform' field");
    }

    // Set up broadcast channel for pub/sub events.
    let (event_tx, _) = broadcast::channel::<GatewayEvent>(256);

    // Open SQLite subscriber store.
    let db_path = std::env::var("GATEWAY_DB_PATH").unwrap_or_else(|_| "./gateway.db".into());
    let subscriber_store = SubscriberStore::new(&db_path);
    tracing::info!("Subscriber store opened at {db_path}");

    // Spawn the event dispatcher.
    let http_client = reqwest::Client::new();
    subscribers::spawn_dispatcher(event_tx.subscribe(), subscriber_store.clone(), http_client);

    // Spawn Discord listener (serenity websocket) if token is available.
    if let Ok(token) = std::env::var("DISCORD_BOT_TOKEN") {
        let tx = event_tx.clone();
        tokio::spawn(async move {
            discord_listener::start(token, tx).await;
        });
        tracing::info!("Discord event listener spawned");
    }

    let state = Arc::new(AppState {
        platforms,
        default_platform,
        event_tx,
        subscriber_store,
    });

    let app = Router::new()
        .nest("/api/v1", routes::router())
        .nest("/api/v1/webhooks", webhooks::router())
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(3000);

    let listener = tokio::net::TcpListener::bind(("0.0.0.0", port))
        .await
        .expect("Failed to bind");

    tracing::info!("Listening on 0.0.0.0:{port}");
    axum::serve(listener, app).await.expect("Server error");
}

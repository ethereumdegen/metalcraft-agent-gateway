mod auth;
mod platform;
mod routes;

use std::sync::Arc;
use axum::Router;
use tower_http::trace::TraceLayer;

pub struct AppState {
    pub platform: Box<dyn platform::Platform>,
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

    let platform_name =
        std::env::var("PLATFORM").unwrap_or_else(|_| "discord".into());

    let platform: Box<dyn platform::Platform> = match platform_name.as_str() {
        "discord" => Box::new(platform::discord::Discord::from_env()),
        "slack" => Box::new(platform::slack::Slack::from_env()),
        other => {
            tracing::error!("Unknown PLATFORM={other}, expected 'discord' or 'slack'");
            std::process::exit(1);
        }
    };

    tracing::info!("Starting gateway with platform={platform_name}");

    let state = Arc::new(AppState { platform });

    let app = Router::new()
        .nest("/api/v1", routes::router())
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

use std::sync::Arc;

use rusqlite::params;
use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, Mutex};

use crate::events::GatewayEvent;

/// A webhook subscriber persisted in SQLite.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Subscriber {
    pub id: String,
    pub url: String,
    pub events: Vec<String>,
    pub platforms: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secret: Option<String>,
    pub created_at: String,
}

/// Thread-safe wrapper around a SQLite connection for subscriber CRUD.
#[derive(Clone)]
pub struct SubscriberStore {
    conn: Arc<Mutex<rusqlite::Connection>>,
}

impl SubscriberStore {
    /// Open (or create) the SQLite database and run migrations.
    pub fn new(path: &str) -> Self {
        let conn = rusqlite::Connection::open(path).expect("Failed to open subscriber database");
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS subscribers (
                id         TEXT PRIMARY KEY,
                url        TEXT NOT NULL,
                events     TEXT NOT NULL,
                platforms  TEXT,
                secret     TEXT,
                created_at TEXT NOT NULL
            );",
        )
        .expect("Failed to create subscribers table");

        Self {
            conn: Arc::new(Mutex::new(conn)),
        }
    }

    /// Insert a new subscriber and return it.
    pub async fn add(
        &self,
        url: String,
        events: Vec<String>,
        platforms: Option<Vec<String>>,
        secret: Option<String>,
    ) -> Subscriber {
        let sub = Subscriber {
            id: uuid::Uuid::new_v4().to_string(),
            url,
            events,
            platforms,
            secret,
            created_at: chrono::Utc::now().to_rfc3339(),
        };
        let conn = self.conn.lock().await;
        conn.execute(
            "INSERT INTO subscribers (id, url, events, platforms, secret, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                sub.id,
                sub.url,
                serde_json::to_string(&sub.events).unwrap(),
                sub.platforms.as_ref().map(|p| serde_json::to_string(p).unwrap()),
                sub.secret,
                sub.created_at,
            ],
        )
        .expect("Failed to insert subscriber");
        sub
    }

    /// List all subscribers.
    pub async fn list(&self) -> Vec<Subscriber> {
        let conn = self.conn.lock().await;
        let mut stmt = conn
            .prepare("SELECT id, url, events, platforms, secret, created_at FROM subscribers")
            .expect("Failed to prepare list query");

        stmt.query_map([], |row| {
            let events_json: String = row.get(2)?;
            let platforms_json: Option<String> = row.get(3)?;
            Ok(Subscriber {
                id: row.get(0)?,
                url: row.get(1)?,
                events: serde_json::from_str(&events_json).unwrap_or_default(),
                platforms: platforms_json.and_then(|p| serde_json::from_str(&p).ok()),
                secret: row.get(4)?,
                created_at: row.get(5)?,
            })
        })
        .expect("Failed to query subscribers")
        .filter_map(|r| r.ok())
        .collect()
    }

    /// Remove a subscriber by id. Returns true if a row was deleted.
    pub async fn remove(&self, id: &str) -> bool {
        let conn = self.conn.lock().await;
        let deleted = conn
            .execute("DELETE FROM subscribers WHERE id = ?1", params![id])
            .unwrap_or(0);
        deleted > 0
    }

    /// Return subscribers whose event and platform filters match the given event.
    pub async fn get_matching(&self, event: &GatewayEvent) -> Vec<Subscriber> {
        self.list()
            .await
            .into_iter()
            .filter(|sub| {
                // Check event filter
                let event_match =
                    sub.events.contains(&"*".to_string()) || sub.events.contains(&event.event_type);

                // Check platform filter
                let platform_match = match &sub.platforms {
                    None => true,
                    Some(platforms) => platforms.contains(&event.platform),
                };

                event_match && platform_match
            })
            .collect()
    }
}

/// Spawn a long-running task that reads events from the broadcast channel
/// and dispatches them to matching subscribers via HTTP POST.
pub fn spawn_dispatcher(
    mut rx: broadcast::Receiver<GatewayEvent>,
    store: SubscriberStore,
    client: reqwest::Client,
) {
    tokio::spawn(async move {
        loop {
            match rx.recv().await {
                Ok(event) => {
                    let subscribers = store.get_matching(&event).await;
                    for sub in subscribers {
                        let client = client.clone();
                        let event = event.clone();
                        tokio::spawn(async move {
                            let mut req = client
                                .post(&sub.url)
                                .json(&event)
                                .timeout(std::time::Duration::from_secs(10));

                            if let Some(ref secret) = sub.secret {
                                req = req.bearer_auth(secret);
                            }

                            if let Err(e) = req.send().await {
                                tracing::warn!(
                                    subscriber_id = %sub.id,
                                    url = %sub.url,
                                    error = %e,
                                    "Failed to dispatch event to subscriber"
                                );
                            }
                        });
                    }
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!("Dispatcher lagged, skipped {n} events");
                }
                Err(broadcast::error::RecvError::Closed) => {
                    tracing::info!("Broadcast channel closed, dispatcher exiting");
                    break;
                }
            }
        }
    });
}

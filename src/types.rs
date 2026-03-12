use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use tokio::sync::broadcast;
use tokio::sync::oneshot;
use rusqlite::Connection;
use tokio::sync::Mutex;

#[derive(Debug)]
pub enum InterceptAction {
    ForwardOriginal,
    ForwardModified { body: String },
    Reject,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionRecord {
    pub id: String,
    pub pid: Option<i64>,
    pub cwd: Option<String>,
    pub project_name: Option<String>,
    pub started_at: String,
    pub last_seen_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestRecord {
    pub id: String,
    pub session_id: Option<String>,
    pub timestamp: String,
    pub method: String,
    pub path: String,
    pub request_headers: String,  // JSON, x-api-key excluded
    pub request_body: String,     // JSON
    pub response_status: Option<i64>,
    pub response_headers: Option<String>,  // JSON
    pub response_body: Option<String>,     // JSON
    pub is_streaming: bool,
    pub input_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
    pub duration_ms: Option<i64>,
    pub status: String,  // pending | complete | error
    pub starred: bool,
    pub memo: String,
    pub agent_type: String,   // "main", "explore", "plan", "audit", "sub" etc.
    pub agent_task: String,   // short description of what the sub-agent is doing
}

#[derive(Debug, Clone, Serialize)]
pub struct DashboardEvent {
    pub event_type: String,  // "request_update" | "session_update"
    pub data: serde_json::Value,
}

pub struct AppState {
    pub db: Arc<Mutex<Connection>>,
    pub event_tx: broadcast::Sender<DashboardEvent>,
    pub upstream_url: String,
    pub intercept_enabled: AtomicBool,
    pub intercepted: std::sync::Mutex<HashMap<String, oneshot::Sender<InterceptAction>>>,
}

impl AppState {
    /// Production constructor — forwards to real Anthropic API.
    pub fn new(db: Connection, event_tx: broadcast::Sender<DashboardEvent>) -> Arc<Self> {
        Self::with_upstream(db, event_tx, "https://api.anthropic.com".to_string())
    }

    /// Constructor with configurable upstream URL (used in tests).
    pub fn with_upstream(
        db: Connection,
        event_tx: broadcast::Sender<DashboardEvent>,
        upstream_url: String,
    ) -> Arc<Self> {
        Arc::new(AppState {
            db: Arc::new(Mutex::new(db)),
            event_tx,
            upstream_url,
            intercept_enabled: AtomicBool::new(false),
            intercepted: std::sync::Mutex::new(HashMap::new()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn make_state(upstream: &str) -> Arc<AppState> {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::init_db(&conn).unwrap();
        let (tx, _) = broadcast::channel(4);
        AppState::with_upstream(conn, tx, upstream.to_string())
    }

    #[test]
    fn new_uses_anthropic_upstream() {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::init_db(&conn).unwrap();
        let (tx, _) = broadcast::channel(4);
        let state = AppState::new(conn, tx);
        assert_eq!(state.upstream_url, "https://api.anthropic.com");
    }

    #[test]
    fn with_upstream_sets_custom_url() {
        let state = make_state("http://localhost:9999");
        assert_eq!(state.upstream_url, "http://localhost:9999");
    }

    #[test]
    fn session_record_clone_and_serialize() {
        let s = SessionRecord {
            id: "id-1".to_string(),
            pid: Some(42),
            cwd: Some("/tmp".to_string()),
            project_name: Some("proj".to_string()),
            started_at: "2024-01-01T00:00:00Z".to_string(),
            last_seen_at: "2024-01-01T01:00:00Z".to_string(),
        };
        let cloned = s.clone();
        let json = serde_json::to_string(&cloned).unwrap();
        assert!(json.contains("id-1"));
        assert!(json.contains("proj"));
    }

    #[test]
    fn request_record_is_streaming_default_false() {
        let r = RequestRecord {
            id: "r1".to_string(),
            session_id: None,
            timestamp: "ts".to_string(),
            method: "POST".to_string(),
            path: "/v1/messages".to_string(),
            request_headers: "{}".to_string(),
            request_body: "{}".to_string(),
            response_status: None,
            response_headers: None,
            response_body: None,
            is_streaming: false,
            input_tokens: None,
            output_tokens: None,
            duration_ms: None,
            status: "pending".to_string(),
            starred: false,
            memo: String::new(),
            agent_type: "main".to_string(),
            agent_task: String::new(),
        };
        let json = serde_json::to_string(&r).unwrap();
        assert!(json.contains("pending"));
        assert!(json.contains("is_streaming"));
    }

    #[test]
    fn dashboard_event_serializes_event_type() {
        let ev = DashboardEvent {
            event_type: "request_update".to_string(),
            data: serde_json::json!({"id": "x"}),
        };
        let json = serde_json::to_string(&ev).unwrap();
        assert!(json.contains("request_update"));
    }
}

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use tokio::sync::broadcast;
use tokio::sync::oneshot;
use tokio::sync::RwLock;
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
    pub routing_category: String,
    pub routed_to_url: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DashboardEvent {
    pub event_type: String,  // "request_update" | "session_update"
    pub data: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingConfig {
    pub enabled:             bool,
    pub classifier_base_url: String,
    pub classifier_api_key:  String,
    pub classifier_model:    String,
    pub classifier_prompt:   String,
}

impl Default for RoutingConfig {
    fn default() -> Self {
        RoutingConfig {
            enabled: false,
            classifier_base_url: "https://api.anthropic.com".to_string(),
            classifier_api_key: String::new(),
            classifier_model: "claude-haiku-4-5-20251001".to_string(),
            classifier_prompt: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingRule {
    #[serde(default)]
    pub id:             String,
    pub priority:       i64,
    pub enabled:        bool,
    pub category:       String,
    #[serde(default)]
    pub description:    String,
    pub target_url:     String,
    #[serde(default)]
    pub api_key:        String,
    #[serde(default)]
    pub prompt_override: String,
    pub model_override: String,
    pub label:          String,
}

pub struct AppState {
    pub db: Arc<Mutex<Connection>>,
    pub event_tx: broadcast::Sender<DashboardEvent>,
    pub upstream_url: String,
    pub intercept_enabled: AtomicBool,
    pub intercepted: std::sync::Mutex<HashMap<String, oneshot::Sender<InterceptAction>>>,
    pub routing_config: RwLock<RoutingConfig>,
    pub routing_rules: RwLock<Vec<RoutingRule>>,
}

impl AppState {
    /// Production constructor — loads routing config/rules from DB.
    pub fn new(db: Connection, event_tx: broadcast::Sender<DashboardEvent>) -> Arc<Self> {
        let routing_config = crate::db::get_routing_config(&db).unwrap_or_default();
        let routing_rules = crate::db::get_routing_rules(&db).unwrap_or_default();
        Arc::new(AppState {
            db: Arc::new(Mutex::new(db)),
            event_tx,
            upstream_url: "https://api.anthropic.com".to_string(),
            intercept_enabled: AtomicBool::new(false),
            intercepted: std::sync::Mutex::new(HashMap::new()),
            routing_config: RwLock::new(routing_config),
            routing_rules: RwLock::new(routing_rules),
        })
    }

    /// Constructor with configurable upstream URL (used in tests).
    #[allow(dead_code)]
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
            routing_config: RwLock::new(RoutingConfig::default()),
            routing_rules: RwLock::new(Vec::new()),
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
            routing_category: String::new(),
            routed_to_url: String::new(),
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

    #[test]
    fn routing_config_default_values() {
        let cfg = RoutingConfig::default();
        assert!(!cfg.enabled);
        assert_eq!(cfg.classifier_base_url, "https://api.anthropic.com");
        assert_eq!(cfg.classifier_model, "claude-haiku-4-5-20251001");
        assert!(cfg.classifier_prompt.is_empty());
    }

    #[test]
    fn routing_rule_serialize_round_trip() {
        let rule = RoutingRule {
            id: "rule-1".to_string(),
            priority: 10,
            enabled: true,
            category: "code_gen".to_string(),
            description: "Writing new code".to_string(),
            target_url: "https://openai.com".to_string(),
            api_key: String::new(),
            prompt_override: String::new(),
            model_override: "gpt-4".to_string(),
            label: "GPT-4 for code".to_string(),
        };
        let json = serde_json::to_string(&rule).unwrap();
        let back: RoutingRule = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, "rule-1");
        assert_eq!(back.priority, 10);
        assert_eq!(back.model_override, "gpt-4");
    }

    #[test]
    fn appstate_with_upstream_has_empty_routing() {
        let state = make_state("http://mock");
        // routing_config should be default (disabled)
        let config = state.routing_config.try_read().unwrap();
        assert!(!config.enabled);
        let rules = state.routing_rules.try_read().unwrap();
        assert!(rules.is_empty());
    }
}

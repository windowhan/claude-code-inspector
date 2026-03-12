use std::sync::Arc;
use std::sync::atomic::Ordering;

use anyhow::{anyhow, Result};
use tokio::sync::oneshot;

use crate::types::{AppState, InterceptAction};

pub fn should_intercept(state: &AppState) -> bool {
    state.intercept_enabled.load(Ordering::Relaxed)
}

pub fn toggle(state: &AppState) -> bool {
    let prev = state.intercept_enabled.load(Ordering::Relaxed);
    state.intercept_enabled.store(!prev, Ordering::Relaxed);
    !prev
}

pub fn register(state: &Arc<AppState>, request_id: &str) -> oneshot::Receiver<InterceptAction> {
    let (tx, rx) = oneshot::channel();
    let mut map = state.intercepted.lock().unwrap();
    map.insert(request_id.to_string(), tx);
    rx
}

#[allow(dead_code)]
pub fn resolve(state: &Arc<AppState>, request_id: &str, action: InterceptAction) -> Result<()> {
    let mut map = state.intercepted.lock().unwrap();
    let sender = map.remove(request_id)
        .ok_or_else(|| anyhow!("request_id not found: {}", request_id))?;
    sender.send(action)
        .map_err(|_| anyhow!("receiver dropped for request_id: {}", request_id))
}

#[allow(dead_code)]
pub fn list_pending(state: &Arc<AppState>) -> Vec<String> {
    let map = state.intercepted.lock().unwrap();
    map.keys().cloned().collect()
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use rusqlite::Connection;
    use tokio::sync::broadcast;

    fn make_state() -> Arc<AppState> {
        let conn = Connection::open_in_memory().unwrap();
        db::init_db(&conn).unwrap();
        let (tx, _) = broadcast::channel(4);
        AppState::with_upstream(conn, tx, "http://mock".to_string())
    }

    #[test]
    fn should_intercept_disabled_by_default() {
        let state = make_state();
        assert!(!should_intercept(&state));
    }

    #[test]
    fn toggle_enables_and_disables() {
        let state = make_state();
        assert!(!should_intercept(&state));
        let new = toggle(&state);
        assert!(new);
        assert!(should_intercept(&state));
        let new = toggle(&state);
        assert!(!new);
        assert!(!should_intercept(&state));
    }

    #[tokio::test]
    async fn register_and_resolve_forward_original() {
        let state = make_state();
        let rx = register(&state, "r1");
        resolve(&state, "r1", InterceptAction::ForwardOriginal).unwrap();
        let action = rx.await.unwrap();
        assert!(matches!(action, InterceptAction::ForwardOriginal));
    }

    #[tokio::test]
    async fn register_and_resolve_forward_modified() {
        let state = make_state();
        let rx = register(&state, "r2");
        let body = r#"{"model":"new"}"#.to_string();
        resolve(&state, "r2", InterceptAction::ForwardModified { body: body.clone() }).unwrap();
        let action = rx.await.unwrap();
        match action {
            InterceptAction::ForwardModified { body: b } => assert_eq!(b, body),
            _ => panic!("expected ForwardModified"),
        }
    }

    #[tokio::test]
    async fn register_and_resolve_reject() {
        let state = make_state();
        let rx = register(&state, "r3");
        resolve(&state, "r3", InterceptAction::Reject).unwrap();
        let action = rx.await.unwrap();
        assert!(matches!(action, InterceptAction::Reject));
    }

    #[test]
    fn resolve_nonexistent_returns_error() {
        let state = make_state();
        let result = resolve(&state, "nonexistent", InterceptAction::ForwardOriginal);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[test]
    fn list_pending_returns_registered_ids() {
        let state = make_state();
        let _rx1 = register(&state, "a");
        let _rx2 = register(&state, "b");
        let mut pending = list_pending(&state);
        pending.sort();
        assert_eq!(pending, vec!["a".to_string(), "b".to_string()]);
    }

    #[test]
    fn list_pending_empty_initially() {
        let state = make_state();
        assert!(list_pending(&state).is_empty());
    }

    #[test]
    fn list_pending_after_resolve_removes_entry() {
        let state = make_state();
        let _rx = register(&state, "x");
        assert_eq!(list_pending(&state).len(), 1);
        resolve(&state, "x", InterceptAction::ForwardOriginal).unwrap();
        assert!(list_pending(&state).is_empty());
    }
}

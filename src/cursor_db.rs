//! Cursor DB watcher — polls Cursor's local SQLite databases for AI chat data.
//!
//! Cursor stores all AI chat data in:
//!   ~/Library/Application Support/Cursor/User/globalStorage/state.vscdb
//! Table: cursorDiskKV
//! Keys:  bubbleId:{conversation_id}:{bubble_id}

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;

use chrono::Utc;
use rusqlite::Connection as SqliteConn;
use serde::Deserialize;
use serde_json::Value;
use tokio::time::{Duration, interval};
use tracing::{debug, warn};

use crate::db;
use crate::types::{AppState, DashboardEvent, RequestRecord, SessionRecord};

const POLL_INTERVAL_SECS: u64 = 3;

/// Returns the path to Cursor's global state.vscdb.
fn cursor_db_path() -> Option<PathBuf> {
    let home = dirs_next::home_dir()?;
    Some(home.join("Library/Application Support/Cursor/User/globalStorage/state.vscdb"))
}

/// Run `pgrep -x Cursor` and return all matching PIDs.
fn cursor_pids() -> Vec<u32> {
    let output = std::process::Command::new("pgrep")
        .args(["-x", "Cursor"])
        .output();
    match output {
        Ok(out) if out.status.success() => {
            String::from_utf8_lossy(&out.stdout)
                .lines()
                .filter_map(|l| l.trim().parse::<u32>().ok())
                .collect()
        }
        _ => vec![],
    }
}

#[derive(Debug, Deserialize)]
struct TokenCount {
    #[serde(rename = "inputTokens", default)]
    input_tokens: i64,
    #[serde(rename = "outputTokens", default)]
    output_tokens: i64,
}

#[derive(Debug, Deserialize)]
struct AttachedFile {
    #[serde(rename = "relativeWorkspacePath", default)]
    relative_workspace_path: String,
}

#[derive(Debug, Deserialize)]
struct Bubble {
    #[serde(rename = "type")]
    bubble_type: i64,
    #[serde(default)]
    text: String,
    #[serde(rename = "bubbleId", default)]
    #[allow(dead_code)]
    bubble_id: String,
    #[serde(rename = "tokenCount")]
    token_count: Option<TokenCount>,
    #[serde(rename = "attachedFileCodeChunksMetadataOnly", default)]
    attached_files: Vec<AttachedFile>,
    #[serde(rename = "toolFormerData")]
    tool_former_data: Option<serde_json::Value>,
    #[serde(rename = "modelInfo")]
    model_info: Option<serde_json::Value>,
    #[serde(rename = "contextWindowStatusAtCreation")]
    context_window: Option<serde_json::Value>,
    #[serde(rename = "createdAt", default)]
    #[allow(dead_code)]
    created_at: String,
}

/// Fetch all `bubbleId:*` keys from the Cursor global state DB.
/// Returns Vec of (key, json_value).
fn fetch_bubble_keys(db_path: &PathBuf) -> Vec<(String, String)> {
    let conn = match SqliteConn::open_with_flags(
        db_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    ) {
        Ok(c) => c,
        Err(e) => {
            debug!("cursor_db: cannot open {:?}: {e}", db_path);
            return vec![];
        }
    };

    let mut stmt = match conn.prepare(
        "SELECT key, value FROM cursorDiskKV WHERE key LIKE 'bubbleId:%' ORDER BY ROWID ASC",
    ) {
        Ok(s) => s,
        Err(e) => {
            debug!("cursor_db: prepare failed: {e}");
            return vec![];
        }
    };

    let rows = stmt.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    });

    match rows {
        Ok(mapped) => mapped.filter_map(|r| r.ok()).collect(),
        Err(e) => {
            debug!("cursor_db: query failed: {e}");
            vec![]
        }
    }
}

/// Given a Cursor PID, find the active workspace path via lsof.
/// Looks for lines matching `workspaceStorage/*/state.vscdb` in lsof output,
/// then reads the workspace.json to get the folder.
fn find_workspace_for_pid(pid: u32) -> Option<(String, String)> {
    let output = std::process::Command::new("lsof")
        .args(["-p", &pid.to_string()])
        .output()
        .ok()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let ws_path = stdout
        .lines()
        .find(|l| l.contains("workspaceStorage") && l.ends_with("state.vscdb"))?;

    // Extract path from lsof line — path starts at the first '/' after the columns.
    // Can't use split_whitespace().last() because "Application Support" has a space.
    let db_file = ws_path.find('/').map(|i| &ws_path[i..])?.trim();
    let db_path = std::path::Path::new(db_file);
    let hash_dir = db_path.parent()?;
    let workspace_json = hash_dir.join("workspace.json");

    let content = std::fs::read_to_string(&workspace_json).ok()?;
    let v: Value = serde_json::from_str(&content).ok()?;
    let folder = v.get("folder")?.as_str()?.to_string();

    // Strip "file://" prefix if present
    let folder = if folder.starts_with("file://") {
        folder[7..].to_string()
    } else {
        folder
    };

    let project_name = std::path::Path::new(&folder)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| folder.clone());

    Some((folder, project_name))
}

/// Derive a stable session_id from the workspace CWD.
/// Falls back to pid-based ID when cwd is empty.
fn workspace_session_id(cwd: &str, fallback_pid: u32) -> String {
    if cwd.is_empty() {
        return format!("cursor-{fallback_pid}");
    }
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut h = DefaultHasher::new();
    cwd.hash(&mut h);
    format!("cursor-{:016x}", h.finish())
}

/// Main watch loop — spawned as a background task.
pub async fn watch(state: Arc<AppState>) {
    let mut ticker = interval(Duration::from_secs(POLL_INTERVAL_SECS));
    // key = "conversation_id:bubble_id"
    let mut seen: HashSet<String> = HashSet::new();
    // conversation_id -> Vec<bubble_id> for pairing user↔AI
    let mut conv_user_bubbles: HashMap<String, Vec<String>> = HashMap::new();
    // conversation_id -> accumulated tool call entries before the AI text response
    let mut conv_tool_calls: HashMap<String, Vec<serde_json::Value>> = HashMap::new();

    loop {
        ticker.tick().await;

        let pids = cursor_pids();
        if pids.is_empty() {
            continue;
        }

        let db_path = match cursor_db_path() {
            Some(p) => p,
            None => continue,
        };

        if !db_path.exists() {
            continue;
        }

        // Collect unique workspaces across all PIDs (multiple Cursor processes share one workspace)
        let mut workspaces: Vec<(String, String, u32)> = vec![]; // (cwd, project_name, first_pid)
        for &pid in &pids {
            let (cwd, project_name) = find_workspace_for_pid(pid)
                .unwrap_or_else(|| (String::new(), format!("Cursor (pid {pid})")));
            // Deduplicate by cwd — only keep first PID per workspace
            if !workspaces.iter().any(|(w, _, _)| *w == cwd) {
                workspaces.push((cwd, project_name, pid));
            }
        }

        // Upsert one session per unique workspace, using a stable cwd-based session_id
        for (cwd, project_name, pid) in &workspaces {
            let session_id = workspace_session_id(cwd, *pid);
            let session = SessionRecord {
                id: session_id.clone(),
                pid: Some(*pid as i64),
                cwd: Some(cwd.clone()),
                project_name: Some(project_name.clone()),
                started_at: Utc::now().to_rfc3339(),
                last_seen_at: Utc::now().to_rfc3339(),
            };
            let db = state.db.lock().await;
            if let Err(e) = db::upsert_session(&db, &session) {
                warn!("cursor_db: upsert_session failed: {e}");
            }
        }

        // Use the first workspace's session_id and cwd for bubble attribution
        let (primary_cwd, primary_session_id) = workspaces
            .first()
            .map(|(cwd, _, pid)| (cwd.clone(), workspace_session_id(cwd, *pid)))
            .unwrap_or_else(|| (String::new(), "cursor-unknown".to_string()));

        // Fetch all bubble keys
        let rows = fetch_bubble_keys(&db_path);

        // Collect new bubbles grouped by conversation
        // key format: bubbleId:{conversation_id}:{bubble_id}
        let mut new_bubbles: Vec<(String, String, Bubble)> = vec![];
        for (key, value) in &rows {
            let parts: Vec<&str> = key.splitn(3, ':').collect();
            if parts.len() != 3 || parts[0] != "bubbleId" {
                continue;
            }
            let conversation_id = parts[1];
            let bubble_id = parts[2];
            let dedup_key = format!("{conversation_id}:{bubble_id}");
            if seen.contains(&dedup_key) {
                continue;
            }

            match serde_json::from_str::<Bubble>(value) {
                Ok(bubble) => {
                    // Don't mark incomplete AI response bubbles as seen — Cursor may still
                    // be streaming the response. We'll retry next tick until text arrives.
                    if bubble.bubble_type == 2
                        && bubble.text.is_empty()
                        && bubble.tool_former_data.is_none()
                    {
                        continue;
                    }
                    seen.insert(dedup_key);
                    new_bubbles.push((conversation_id.to_string(), bubble_id.to_string(), bubble));
                }
                Err(e) => {
                    debug!("cursor_db: parse bubble {key}: {e}");
                    seen.insert(dedup_key); // don't retry broken entries
                }
            }
        }

        if new_bubbles.is_empty() {
            continue;
        }

        let session_id = primary_session_id;

        let db = state.db.lock().await;

        for (conversation_id, bubble_id, bubble) in new_bubbles {
            match bubble.bubble_type {
                1 => {
                    // User message → insert as RequestRecord
                    let file_paths: Vec<&str> = bubble
                        .attached_files
                        .iter()
                        .map(|f| f.relative_workspace_path.as_str())
                        .collect();
                    let model_name = bubble
                        .model_info
                        .as_ref()
                        .and_then(|m| m.get("modelName"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let headers_json = serde_json::json!({
                        "attached_files": file_paths,
                        "model": model_name,
                        "context_window": bubble.context_window
                    })
                    .to_string();

                    let req = RequestRecord {
                        id: bubble_id.clone(),
                        session_id: Some(session_id.clone()),
                        timestamp: Utc::now().to_rfc3339(),
                        method: "CHAT".to_string(),
                        path: "/cursor/chat".to_string(),
                        request_headers: headers_json,
                        request_body: bubble.text.clone(),
                        response_status: None,
                        response_headers: None,
                        response_body: None,
                        is_streaming: false,
                        input_tokens: bubble
                            .token_count
                            .as_ref()
                            .map(|t| t.input_tokens),
                        output_tokens: None,
                        duration_ms: None,
                        status: "pending".to_string(),
                        starred: false,
                        memo: String::new(),
                        agent_type: "cursor".to_string(),
                        // Store conversation_id so we can recover matches after restart
                        agent_task: conversation_id.clone(),
                        routing_category: String::new(),
                        routed_to_url: String::new(),
                        source: "cursor".to_string(),
                        target_host: "cursor.sh".to_string(),
                    };

                    if let Err(e) = db::insert_request(&db, &req) {
                        // Duplicate key is fine — just skip
                        debug!("cursor_db: insert_request {bubble_id}: {e}");
                    }

                    conv_user_bubbles
                        .entry(conversation_id.clone())
                        .or_default()
                        .push(bubble_id.clone());
                }
                2 => {
                    if let Some(tfd) = bubble.tool_former_data {
                        // Tool call bubble — accumulate for this conversation
                        let name = tfd
                            .get("name")
                            .and_then(|v| v.as_str())
                            .unwrap_or("unknown")
                            .to_string();
                        let args: serde_json::Value = tfd
                            .get("rawArgs")
                            .and_then(|v| v.as_str())
                            .and_then(|s| serde_json::from_str(s).ok())
                            .unwrap_or(serde_json::Value::Null);
                        let status = tfd
                            .get("status")
                            .and_then(|v| v.as_str())
                            .unwrap_or("completed")
                            .to_string();
                        // Record file access for tools that operate on files
                        if let Some(path) = args.get("path").and_then(|v| v.as_str()) {
                            if let Some(user_bubbles) = conv_user_bubbles.get(&conversation_id) {
                                if let Some(request_id) = user_bubbles.last() {
                                    let read_range = {
                                        let offset = args.get("offset").and_then(|v| v.as_i64());
                                        let limit = args.get("limit").and_then(|v| v.as_i64());
                                        match (offset, limit) {
                                            (Some(o), Some(l)) => format!("offset:{o},limit:{l}"),
                                            (Some(o), None) => format!("offset:{o}"),
                                            _ => String::new(),
                                        }
                                    };
                                    // Resolve relative paths to absolute using workspace CWD
                                    let abs_path = if path.starts_with('/') {
                                        path.to_string()
                                    } else if !primary_cwd.is_empty() {
                                        format!("{}/{}", primary_cwd.trim_end_matches('/'), path)
                                    } else {
                                        path.to_string()
                                    };
                                    let _ = db::insert_file_access(
                                        &db,
                                        &session_id,
                                        request_id,
                                        &abs_path,
                                        "read",
                                        &read_range,
                                        &Utc::now().to_rfc3339(),
                                    );
                                }
                            }
                        }

                        conv_tool_calls
                            .entry(conversation_id.clone())
                            .or_default()
                            .push(serde_json::json!({
                                "type": "tool",
                                "name": name,
                                "args": args,
                                "status": status
                            }));
                    } else if !bubble.text.is_empty() {
                        // AI text response — build timeline and store
                        let tools = conv_tool_calls
                            .remove(&conversation_id)
                            .unwrap_or_default();
                        let mut timeline = tools;
                        timeline.push(serde_json::json!({
                            "type": "text",
                            "content": bubble.text
                        }));
                        let resp_json = serde_json::to_string(&timeline)
                            .unwrap_or_else(|_| bubble.text.clone());

                        // Try in-memory map first; fall back to DB query for conversations
                        // we haven't seen in this session (e.g. after server restart).
                        let user_bubble_id: Option<String> = conv_user_bubbles
                            .get(&conversation_id)
                            .and_then(|v| v.last())
                            .cloned()
                            .or_else(|| {
                                db::find_pending_cursor_request_by_conversation(
                                    &db,
                                    &conversation_id,
                                )
                                .unwrap_or(None)
                            });

                        if let Some(user_bubble_id) = user_bubble_id {
                            let output_tokens = bubble
                                .token_count
                                .as_ref()
                                .map(|t| t.output_tokens);
                            match db::update_cursor_response(
                                &db,
                                &user_bubble_id,
                                &resp_json,
                                output_tokens,
                            ) {
                                Ok(()) => {
                                    let _ = state.event_tx.send(DashboardEvent {
                                        event_type: "request_update".to_string(),
                                        data: serde_json::json!({ "id": user_bubble_id }),
                                    });
                                }
                                Err(e) => {
                                    warn!(
                                        "cursor_db: update_cursor_response {user_bubble_id}: {e}"
                                    );
                                }
                            }
                        } else {
                            warn!(
                                "cursor_db: no pending request for conversation {conversation_id} (bubble {bubble_id})"
                            );
                        }
                    }
                }
                other => {
                    debug!("cursor_db: unknown bubble type {other} for bubble {bubble_id}");
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_db_path_returns_some() {
        // On macOS this should always return Some
        let path = cursor_db_path();
        assert!(path.is_some());
        let p = path.unwrap();
        assert!(p.to_string_lossy().contains("Cursor"));
        assert!(p.to_string_lossy().ends_with("state.vscdb"));
    }

    #[test]
    fn cursor_pids_returns_vec() {
        // May be empty if Cursor is not running — just verify it doesn't panic
        let pids = cursor_pids();
        // All returned values must be valid PIDs (> 0)
        for pid in &pids {
            assert!(*pid > 0);
        }
    }

    #[test]
    fn bubble_deserialize_user() {
        let json = r#"{
            "_v": 3,
            "type": 1,
            "text": "hello world",
            "bubbleId": "abc-123",
            "tokenCount": {"inputTokens": 10, "outputTokens": 0},
            "attachedFileCodeChunksMetadataOnly": [{"relativeWorkspacePath": "src/main.rs"}]
        }"#;
        let b: Bubble = serde_json::from_str(json).unwrap();
        assert_eq!(b.bubble_type, 1);
        assert_eq!(b.text, "hello world");
        assert_eq!(b.attached_files.len(), 1);
        assert_eq!(b.attached_files[0].relative_workspace_path, "src/main.rs");
        assert_eq!(b.token_count.unwrap().input_tokens, 10);
    }

    #[test]
    fn bubble_deserialize_ai() {
        let json = r#"{
            "_v": 3,
            "type": 2,
            "text": "AI response here",
            "bubbleId": "def-456",
            "tokenCount": {"inputTokens": 0, "outputTokens": 42},
            "attachedFileCodeChunksMetadataOnly": []
        }"#;
        let b: Bubble = serde_json::from_str(json).unwrap();
        assert_eq!(b.bubble_type, 2);
        assert_eq!(b.text, "AI response here");
        assert_eq!(b.token_count.unwrap().output_tokens, 42);
    }

    #[test]
    fn bubble_deserialize_missing_fields() {
        // Minimal bubble with only required fields
        let json = r#"{"type": 1, "text": "hi"}"#;
        let b: Bubble = serde_json::from_str(json).unwrap();
        assert_eq!(b.bubble_type, 1);
        assert_eq!(b.text, "hi");
        assert!(b.token_count.is_none());
        assert!(b.attached_files.is_empty());
        assert!(b.tool_former_data.is_none());
        assert!(b.model_info.is_none());
        assert!(b.context_window.is_none());
    }

    #[test]
    fn bubble_deserialize_tool_call() {
        let json = r#"{
            "_v": 3,
            "type": 2,
            "text": "",
            "bubbleId": "tool-001",
            "toolFormerData": {
                "toolCallId": "call-abc",
                "name": "read_file_v2",
                "rawArgs": "{\"path\":\"src/main.rs\",\"offset\":0}",
                "status": "completed"
            },
            "attachedFileCodeChunksMetadataOnly": []
        }"#;
        let b: Bubble = serde_json::from_str(json).unwrap();
        assert_eq!(b.bubble_type, 2);
        assert!(b.tool_former_data.is_some());
        let tfd = b.tool_former_data.unwrap();
        assert_eq!(tfd.get("name").and_then(|v| v.as_str()), Some("read_file_v2"));
        assert_eq!(tfd.get("status").and_then(|v| v.as_str()), Some("completed"));
    }

    #[test]
    fn bubble_deserialize_model_info() {
        let json = r#"{
            "_v": 3,
            "type": 1,
            "text": "hello",
            "bubbleId": "model-001",
            "modelInfo": {"modelName": "claude-3-5-sonnet"},
            "contextWindowStatusAtCreation": {"maxTokens": 200000},
            "attachedFileCodeChunksMetadataOnly": []
        }"#;
        let b: Bubble = serde_json::from_str(json).unwrap();
        assert_eq!(b.bubble_type, 1);
        let mi = b.model_info.unwrap();
        assert_eq!(mi.get("modelName").and_then(|v| v.as_str()), Some("claude-3-5-sonnet"));
        assert!(b.context_window.is_some());
    }
}

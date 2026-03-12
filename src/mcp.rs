/// MCP (Model Context Protocol) server over stdio.
///
/// This allows Claude Code to query the inspector directly:
///   claude mcp add claude-inspector -- claude-code-hook mcp
///
/// Available tools:
///   list_sessions   - all tracked sessions with stats
///   list_requests   - recent requests (optional session_id, limit)
///   get_request     - full request+response detail by id
use std::io::{self, BufRead, Write};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tracing::{debug, error};

use crate::db;
use crate::types::AppState;

// ── JSON-RPC types ────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct JsonRpcRequest {
    id: Option<Value>,
    method: String,
    params: Option<Value>,
}

#[derive(Debug, Serialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: &'static str,
    pub id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

#[derive(Debug, Serialize)]
pub struct JsonRpcError {
    pub code: i64,
    pub message: String,
}

impl JsonRpcResponse {
    pub fn ok(id: Value, result: Value) -> Self {
        JsonRpcResponse { jsonrpc: "2.0", id, result: Some(result), error: None }
    }
    pub fn err(id: Value, code: i64, message: impl Into<String>) -> Self {
        JsonRpcResponse {
            jsonrpc: "2.0",
            id,
            result: None,
            error: Some(JsonRpcError { code, message: message.into() }),
        }
    }
}

// ── MCP Capabilities ──────────────────────────────────────────────────────────

fn server_info() -> Value {
    json!({
        "name": "claude-code-inspector",
        "version": env!("CARGO_PKG_VERSION"),
    })
}

fn capabilities() -> Value {
    json!({ "tools": {} })
}

fn tool_list() -> Value {
    json!({
        "tools": [
            {
                "name": "list_sessions",
                "description": "List all Claude Code sessions tracked by the inspector, with request counts and token usage.",
                "inputSchema": {
                    "type": "object",
                    "properties": {},
                    "required": []
                }
            },
            {
                "name": "list_requests",
                "description": "List recent API requests. Optionally filter by session_id and control pagination.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "session_id": { "type": "string", "description": "Filter to a specific session" },
                        "limit": { "type": "integer", "description": "Max results (default 20)" },
                        "offset": { "type": "integer", "description": "Pagination offset" }
                    },
                    "required": []
                }
            },
            {
                "name": "get_request",
                "description": "Get the full detail of a specific request including messages and response.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "id": { "type": "string", "description": "Request UUID" }
                    },
                    "required": ["id"]
                }
            }
        ]
    })
}

// ── Tool handlers ─────────────────────────────────────────────────────────────

async fn handle_list_sessions(state: &AppState) -> Value {
    let db = state.db.lock().await;
    match db::get_session_stats(&db) {
        Ok(sessions) => json!({
            "content": [{
                "type": "text",
                "text": serde_json::to_string_pretty(&sessions).unwrap_or_default()
            }]
        }),
        Err(e) => tool_error(e.to_string()),
    }
}

async fn handle_list_requests(state: &AppState, params: Option<&Value>) -> Value {
    let session_id = params.and_then(|p| p.get("session_id")).and_then(|v| v.as_str()).map(|s| s.to_string());
    let limit  = params.and_then(|p| p.get("limit")).and_then(|v| v.as_i64()).unwrap_or(20);
    let offset = params.and_then(|p| p.get("offset")).and_then(|v| v.as_i64()).unwrap_or(0);

    let db = state.db.lock().await;
    match db::get_requests(&db, session_id.as_deref(), limit, offset) {
        Ok(reqs) => {
            let summary: Vec<Value> = reqs.iter().map(|r| json!({
                "id": r.id,
                "session_id": r.session_id,
                "timestamp": r.timestamp,
                "method": r.method,
                "path": r.path,
                "status": r.status,
                "response_status": r.response_status,
                "is_streaming": r.is_streaming,
                "input_tokens": r.input_tokens,
                "output_tokens": r.output_tokens,
                "duration_ms": r.duration_ms,
            })).collect();
            json!({
                "content": [{
                    "type": "text",
                    "text": serde_json::to_string_pretty(&summary).unwrap_or_default()
                }]
            })
        }
        Err(e) => tool_error(e.to_string()),
    }
}

async fn handle_get_request(state: &AppState, params: Option<&Value>) -> Value {
    let id = match params.and_then(|p| p.get("id")).and_then(|v| v.as_str()) {
        Some(id) => id.to_string(),
        None => return tool_error("Missing required parameter: id"),
    };

    let db = state.db.lock().await;
    match db::get_request_by_id(&db, &id) {
        Ok(Some(req)) => {
            let req_json: Value = serde_json::from_str(&req.request_body).unwrap_or(Value::Null);
            let resp_json: Value = req.response_body.as_deref()
                .and_then(|b| serde_json::from_str(b).ok())
                .unwrap_or(Value::Null);

            let detail = json!({
                "id": req.id,
                "session_id": req.session_id,
                "timestamp": req.timestamp,
                "status": req.status,
                "response_status": req.response_status,
                "is_streaming": req.is_streaming,
                "input_tokens": req.input_tokens,
                "output_tokens": req.output_tokens,
                "duration_ms": req.duration_ms,
                "request": req_json,
                "response": resp_json,
            });
            json!({
                "content": [{
                    "type": "text",
                    "text": serde_json::to_string_pretty(&detail).unwrap_or_default()
                }]
            })
        }
        Ok(None) => tool_error(format!("Request not found: {id}")),
        Err(e)   => tool_error(e.to_string()),
    }
}

fn tool_error(msg: impl Into<String>) -> Value {
    json!({
        "content": [{ "type": "text", "text": msg.into() }],
        "isError": true
    })
}

// ── Dispatch (extracted for testability) ─────────────────────────────────────

/// Process a single JSON-RPC request and return the response.
/// This is the core logic, extracted so tests can call it without stdin/stdout.
pub async fn dispatch_message(
    method: &str,
    id: Value,
    params: Option<&Value>,
    state: &Arc<AppState>,
) -> Option<JsonRpcResponse> {
    match method {
        "initialize" => Some(JsonRpcResponse::ok(id, json!({
            "protocolVersion": "2024-11-05",
            "serverInfo": server_info(),
            "capabilities": capabilities(),
        }))),
        "notifications/initialized" => None,
        "tools/list" => Some(JsonRpcResponse::ok(id, tool_list())),
        "tools/call" => {
            let tool_name = params
                .and_then(|p| p.get("name"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let tool_params = params.and_then(|p| p.get("arguments"));

            let result = match tool_name {
                "list_sessions" => handle_list_sessions(state).await,
                "list_requests" => handle_list_requests(state, tool_params).await,
                "get_request"   => handle_get_request(state, tool_params).await,
                other           => tool_error(format!("Unknown tool: {other}")),
            };
            Some(JsonRpcResponse::ok(id, result))
        }
        "ping" => Some(JsonRpcResponse::ok(id, json!({}))),
        other => Some(JsonRpcResponse::err(id, -32601, format!("Method not found: {other}"))),
    }
}

// ── Main stdio loop ───────────────────────────────────────────────────────────

pub async fn run_mcp_server(state: Arc<AppState>) -> anyhow::Result<()> {
    let stdin  = io::stdin();
    let stdout = io::stdout();
    let mut out = io::BufWriter::new(stdout.lock());

    eprintln!("[claude-code-inspector] MCP server started (stdio)");

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) if l.trim().is_empty() => continue,
            Ok(l) => l,
            Err(e) => { error!("stdin read error: {e}"); break; }
        };

        debug!("MCP recv: {line}");

        let req: JsonRpcRequest = match serde_json::from_str(&line) {
            Ok(r) => r,
            Err(e) => {
                let resp = JsonRpcResponse::err(Value::Null, -32700, format!("Parse error: {e}"));
                writeln!(out, "{}", serde_json::to_string(&resp)?)?;
                out.flush()?;
                continue;
            }
        };

        let id     = req.id.clone().unwrap_or(Value::Null);
        let params = req.params.as_ref();

        if let Some(resp) = dispatch_message(&req.method, id, params, &state).await {
            let resp_str = serde_json::to_string(&resp)?;
            debug!("MCP send: {resp_str}");
            writeln!(out, "{resp_str}")?;
            out.flush()?;
        }
    }

    Ok(())
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::types::{RequestRecord, SessionRecord};
    use rusqlite::Connection;
    use tokio::sync::broadcast;

    fn make_state() -> Arc<AppState> {
        let conn = Connection::open_in_memory().unwrap();
        db::init_db(&conn).unwrap();
        let (tx, _) = broadcast::channel(4);
        AppState::with_upstream(conn, tx, "http://mock".to_string())
    }

    fn seed_session(state: &Arc<AppState>) -> String {
        let id = "sess-1".to_string();
        let db = state.db.try_lock().unwrap();
        db::upsert_session(&db, &SessionRecord {
            id: id.clone(),
            pid: Some(1),
            cwd: Some("/tmp/proj".to_string()),
            project_name: Some("proj".to_string()),
            started_at: "2024-01-01T00:00:00Z".to_string(),
            last_seen_at: "2024-01-01T00:00:00Z".to_string(),
        }).unwrap();
        id
    }

    fn seed_request(state: &Arc<AppState>, req_id: &str, session_id: &str) {
        let db = state.db.try_lock().unwrap();
        db::insert_request(&db, &RequestRecord {
            id: req_id.to_string(),
            session_id: Some(session_id.to_string()),
            timestamp: "2024-01-01T00:00:00Z".to_string(),
            method: "POST".to_string(),
            path: "/v1/messages".to_string(),
            request_headers: "{}".to_string(),
            request_body: r#"{"model":"claude","messages":[]}"#.to_string(),
            response_status: None,
            response_headers: None,
            response_body: None,
            is_streaming: false,
            input_tokens: None,
            output_tokens: None,
            duration_ms: None,
            status: "pending".to_string(),
        }).unwrap();
        // Populate response fields (insert_request only stores base fields)
        db::update_request_complete(
            &db, req_id, 200, "{}", r#"{"content":"hi"}"#,
            Some(5), Some(3), 100, "complete",
        ).unwrap();
    }

    #[tokio::test]
    async fn initialize_returns_protocol_version() {
        let state = make_state();
        let resp = dispatch_message("initialize", json!(1), None, &state).await.unwrap();
        assert!(resp.error.is_none());
        let result = resp.result.unwrap();
        assert_eq!(result["protocolVersion"], "2024-11-05");
        assert!(result["serverInfo"]["name"].as_str().unwrap().contains("inspector"));
        assert!(result["capabilities"]["tools"].is_object());
    }

    #[tokio::test]
    async fn notifications_initialized_returns_none() {
        let state = make_state();
        let resp = dispatch_message("notifications/initialized", json!(1), None, &state).await;
        assert!(resp.is_none());
    }

    #[tokio::test]
    async fn tools_list_returns_three_tools() {
        let state = make_state();
        let resp = dispatch_message("tools/list", json!(1), None, &state).await.unwrap();
        let tools = resp.result.unwrap()["tools"].as_array().unwrap().clone();
        assert_eq!(tools.len(), 3);
        let names: Vec<&str> = tools.iter()
            .map(|t| t["name"].as_str().unwrap())
            .collect();
        assert!(names.contains(&"list_sessions"));
        assert!(names.contains(&"list_requests"));
        assert!(names.contains(&"get_request"));
    }

    #[tokio::test]
    async fn ping_returns_empty_result() {
        let state = make_state();
        let resp = dispatch_message("ping", json!(42), None, &state).await.unwrap();
        assert!(resp.error.is_none());
        assert_eq!(resp.id, json!(42));
    }

    #[tokio::test]
    async fn unknown_method_returns_error_32601() {
        let state = make_state();
        let resp = dispatch_message("not/a/method", json!(1), None, &state).await.unwrap();
        let err = resp.error.unwrap();
        assert_eq!(err.code, -32601);
        assert!(err.message.contains("not/a/method"));
    }

    #[tokio::test]
    async fn tool_list_sessions_empty() {
        let state = make_state();
        let params = json!({"name": "list_sessions", "arguments": {}});
        let resp = dispatch_message("tools/call", json!(1), Some(&params), &state).await.unwrap();
        assert!(resp.error.is_none());
        let text = resp.result.unwrap()["content"][0]["text"].as_str().unwrap().to_string();
        // Should be valid JSON (empty array)
        let arr: Vec<serde_json::Value> = serde_json::from_str(&text).unwrap();
        assert!(arr.is_empty());
    }

    #[tokio::test]
    async fn tool_list_sessions_with_data() {
        let state = make_state();
        seed_session(&state);

        let params = json!({"name": "list_sessions", "arguments": {}});
        let resp = dispatch_message("tools/call", json!(1), Some(&params), &state).await.unwrap();
        let text = resp.result.unwrap()["content"][0]["text"].as_str().unwrap().to_string();
        let arr: Vec<serde_json::Value> = serde_json::from_str(&text).unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["project_name"], "proj");
    }

    #[tokio::test]
    async fn tool_list_requests_returns_summary() {
        let state = make_state();
        let sid = seed_session(&state);
        seed_request(&state, "req-1", &sid);

        let params = json!({"name": "list_requests", "arguments": {}});
        let resp = dispatch_message("tools/call", json!(1), Some(&params), &state).await.unwrap();
        let text = resp.result.unwrap()["content"][0]["text"].as_str().unwrap().to_string();
        let arr: Vec<serde_json::Value> = serde_json::from_str(&text).unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["id"], "req-1");
        assert_eq!(arr[0]["status"], "complete");
        // Summary must NOT include full request/response bodies
        assert!(arr[0].get("request_body").is_none());
    }

    #[tokio::test]
    async fn tool_list_requests_filters_by_session() {
        let state = make_state();
        let sid = seed_session(&state);
        seed_request(&state, "req-1", &sid);
        seed_request(&state, "req-2", "other-session");

        let params = json!({"name": "list_requests", "arguments": {"session_id": sid}});
        let resp = dispatch_message("tools/call", json!(1), Some(&params), &state).await.unwrap();
        let text = resp.result.unwrap()["content"][0]["text"].as_str().unwrap().to_string();
        let arr: Vec<serde_json::Value> = serde_json::from_str(&text).unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["id"], "req-1");
    }

    #[tokio::test]
    async fn tool_get_request_found() {
        let state = make_state();
        let sid = seed_session(&state);
        seed_request(&state, "req-1", &sid);

        let params = json!({"name": "get_request", "arguments": {"id": "req-1"}});
        let resp = dispatch_message("tools/call", json!(1), Some(&params), &state).await.unwrap();
        let text = resp.result.unwrap()["content"][0]["text"].as_str().unwrap().to_string();
        let detail: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(detail["id"], "req-1");
        assert_eq!(detail["status"], "complete");
        assert_eq!(detail["input_tokens"], 5);
        assert_eq!(detail["output_tokens"], 3);
        // Full request + response must be present
        assert!(detail["request"].is_object());
        assert!(detail["response"].is_object());
    }

    #[tokio::test]
    async fn tool_get_request_not_found() {
        let state = make_state();
        let params = json!({"name": "get_request", "arguments": {"id": "does-not-exist"}});
        let resp = dispatch_message("tools/call", json!(1), Some(&params), &state).await.unwrap();
        let result = resp.result.unwrap();
        assert_eq!(result["isError"], true);
        assert!(result["content"][0]["text"].as_str().unwrap().contains("not found"));
    }

    #[tokio::test]
    async fn tool_get_request_missing_id_param() {
        let state = make_state();
        let params = json!({"name": "get_request", "arguments": {}});
        let resp = dispatch_message("tools/call", json!(1), Some(&params), &state).await.unwrap();
        let result = resp.result.unwrap();
        assert_eq!(result["isError"], true);
        assert!(result["content"][0]["text"].as_str().unwrap().contains("Missing required parameter"));
    }

    #[tokio::test]
    async fn tool_call_unknown_tool_returns_error() {
        let state = make_state();
        let params = json!({"name": "nonexistent_tool", "arguments": {}});
        let resp = dispatch_message("tools/call", json!(1), Some(&params), &state).await.unwrap();
        let result = resp.result.unwrap();
        assert_eq!(result["isError"], true);
        assert!(result["content"][0]["text"].as_str().unwrap().contains("Unknown tool"));
    }

    #[test]
    fn jsonrpc_response_ok_has_no_error() {
        let r = JsonRpcResponse::ok(json!(1), json!({"x": 1}));
        assert!(r.error.is_none());
        assert!(r.result.is_some());
        assert_eq!(r.jsonrpc, "2.0");
    }

    #[test]
    fn jsonrpc_response_err_has_no_result() {
        let r = JsonRpcResponse::err(json!(1), -32600, "bad request");
        assert!(r.result.is_none());
        let err = r.error.unwrap();
        assert_eq!(err.code, -32600);
        assert_eq!(err.message, "bad request");
    }
}

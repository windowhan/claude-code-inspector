use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Instant;

use bytes::Bytes;
use chrono::Utc;
use futures::StreamExt;
use http_body_util::{BodyExt, Full};
use hyper::{Request, Response, StatusCode};
use tokio::sync::oneshot;
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::db;
use crate::intercept;
use crate::routing;
use crate::session::{resolve_session, SessionCache};
use crate::sse_tee::{parse_sse_content, SseTeeStream};
use crate::supervisor;
use crate::types::{AppState, DashboardEvent, InterceptAction, RequestRecord, SessionRecord};

pub async fn handle_request(
    req: Request<hyper::body::Incoming>,
    state: Arc<AppState>,
    peer_addr: SocketAddr,
    session_cache: SessionCache,
) -> Result<Response<Full<Bytes>>, hyper::Error> {
    match handle_inner(req, Arc::clone(&state), peer_addr, session_cache).await {
        Ok(resp) => Ok(resp),
        Err(e) => {
            error!("Proxy error: {e}");
            // Try to mark any pending request as error using the error message
            // (request_id is embedded in the error via context if available)
            cleanup_pending_on_error(&state).await;
            Ok(Response::builder()
                .status(StatusCode::BAD_GATEWAY)
                .body(Full::new(Bytes::from(format!("Proxy error: {e}"))))
                .unwrap())
        }
    }
}

/// Mark very recent pending requests as error when proxy encounters an unrecoverable error.
/// This prevents requests from being stuck in pending state forever.
async fn cleanup_pending_on_error(state: &AppState) {
    let db = state.db.lock().await;
    if let Err(e) = db.execute(
        "UPDATE requests SET status = 'error' WHERE status = 'pending' AND timestamp > datetime('now', '-10 seconds')",
        [],
    ) {
        warn!("Failed to cleanup pending requests: {e}");
    }
}

async fn handle_inner(
    req: Request<hyper::body::Incoming>,
    state: Arc<AppState>,
    peer_addr: SocketAddr,
    session_cache: SessionCache,
) -> anyhow::Result<Response<Full<Bytes>>> {
    let start = Instant::now();
    let request_id = Uuid::new_v4().to_string();
    let timestamp = Utc::now().to_rfc3339();

    // Identify session
    let session_info = resolve_session(peer_addr, &session_cache).await;

    // If we know the CWD, try to reuse an existing session for that directory.
    // This groups subagents/background tasks running from the same project under one session.
    let session_id = {
        let db = state.db.lock().await;
        if let Some(cwd) = &session_info.cwd {
            db::find_session_id_by_cwd(&db, cwd)
                .ok()
                .flatten()
                .unwrap_or_else(|| session_info.session_id.clone())
        } else {
            session_info.session_id.clone()
        }
    };

    // Upsert session record
    {
        let now = Utc::now().to_rfc3339();
        let session_record = SessionRecord {
            id: session_id.clone(),
            pid: session_info.pid,
            cwd: session_info.cwd.clone(),
            project_name: session_info.project_name.clone(),
            started_at: now.clone(),
            last_seen_at: now,
        };
        let db = state.db.lock().await;
        if let Err(e) = db::upsert_session(&db, &session_record) {
            warn!("Failed to upsert session: {e}");
        }
    }

    let method = req.method().clone();
    let path = req.uri().path_and_query().map(|p| p.as_str()).unwrap_or("/").to_string();

    // Collect headers, filtering:
    //   - x-api-key: forwarded upstream but never logged
    //   - hop-by-hop headers (host, connection, …): must NOT be forwarded;
    //     reqwest sets the correct Host for the upstream URL automatically.
    //     Passing host: 127.0.0.1:7878 would cause Cloudflare to return 403.
    const HOP_BY_HOP: &[&str] = &[
        "host", "connection", "keep-alive", "transfer-encoding",
        "te", "trailers", "upgrade", "proxy-authorization", "proxy-authenticate",
        // Strip accept-encoding so Anthropic returns uncompressed SSE.
        // If we forward gzip, the raw_sse bytes are compressed and unparseable.
        "accept-encoding",
        // Strip content-length so reqwest recalculates it from the actual body.
        // Essential when intercept modifies the body to a different size.
        "content-length",
    ];

    let headers = req.headers().clone();

    // Extract incoming API key for use as classifier fallback
    let incoming_api_key = headers
        .get("x-api-key")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    let mut log_headers: HashMap<String, String> = HashMap::new();
    let mut upstream_headers: HashMap<String, String> = HashMap::new();

    for (name, value) in &headers {
        let name_str = name.as_str().to_lowercase();
        let val_str = value.to_str().unwrap_or("").to_string();
        if HOP_BY_HOP.contains(&name_str.as_str()) {
            continue;
        }
        upstream_headers.insert(name_str.clone(), val_str.clone());
        if name_str != "x-api-key" {
            log_headers.insert(name_str, val_str);
        }
    }

    // Read request body
    let mut body_bytes = req.into_body().collect().await?.to_bytes();
    let body_str = String::from_utf8_lossy(&body_bytes).to_string();

    // Determine if streaming and detect agent type
    let body_json = serde_json::from_str::<serde_json::Value>(&body_str).ok();
    let is_streaming = body_json.as_ref()
        .and_then(|v| v.get("stream").and_then(|s| s.as_bool()))
        .unwrap_or(false);
    let (agent_type, agent_task) = detect_agent_type(&body_str);

    info!(
        request_id = %request_id,
        method = %method,
        path = %path,
        is_streaming = is_streaming,
        project = ?session_info.project_name,
        "Proxying request"
    );

    // Insert pending record into DB
    let req_record = RequestRecord {
        id: request_id.clone(),
        session_id: Some(session_id.clone()),
        timestamp: timestamp.clone(),
        method: method.to_string(),
        path: path.clone(),
        request_headers: serde_json::to_string(&log_headers).unwrap_or_default(),
        request_body: body_str.clone(),
        response_status: None,
        response_headers: None,
        response_body: None,
        is_streaming,
        input_tokens: None,
        output_tokens: None,
        duration_ms: None,
        status: "pending".to_string(),
        starred: false,
        memo: String::new(),
        agent_type: agent_type.clone(),
        agent_task: agent_task.clone(),
        routing_category: String::new(),
        routed_to_url: String::new(),
    };

    {
        let db = state.db.lock().await;
        if let Err(e) = db::insert_request(&db, &req_record) {
            warn!("Failed to insert request: {e}");
        }
    }

    // Emit pending event
    emit_event(&state, "request_update", serde_json::json!({
        "id": request_id,
        "session_id": session_id,
        "project_name": session_info.project_name,
        "status": "pending",
        "method": method.to_string(),
        "path": path,
        "timestamp": timestamp,
        "is_streaming": is_streaming,
        "agent_type": agent_type,
        "agent_task": agent_task,
    }));

    // ── Intercept checkpoint ─────────────────────────────────────────────────
    if intercept::should_intercept(&state) {
        // Update DB status to intercepted
        {
            let db = state.db.lock().await;
            let _ = db::update_request_status(&db, &request_id, "intercepted");
        }

        // Emit intercepted event
        emit_event(&state, "request_intercepted", serde_json::json!({
            "id": request_id,
            "session_id": session_id,
            "project_name": session_info.project_name,
            "status": "intercepted",
            "method": method.to_string(),
            "path": path,
            "timestamp": timestamp,
            "is_streaming": is_streaming,
            "request_body": body_str,
        }));

        // Register and wait for user decision (60s timeout)
        let rx = intercept::register(&state, &request_id);
        let action = match tokio::time::timeout(
            tokio::time::Duration::from_secs(60),
            rx,
        ).await {
            Ok(Ok(action)) => action,
            Ok(Err(_)) => {
                warn!(request_id = %request_id, "Intercept channel dropped, forwarding original");
                InterceptAction::ForwardOriginal
            }
            Err(_) => {
                warn!(request_id = %request_id, "Intercept timeout (60s), forwarding original");
                // Clean up the sender from the map
                let mut map = state.intercepted.lock().unwrap();
                map.remove(&request_id);
                InterceptAction::ForwardOriginal
            }
        };

        match action {
            InterceptAction::ForwardOriginal => {
                // Continue with original body_bytes
            }
            InterceptAction::ForwardModified { body } => {
                // Update DB with modified body
                {
                    let db = state.db.lock().await;
                    let _ = db::update_request_body(&db, &request_id, &body);
                }
                body_bytes = Bytes::from(body);
            }
            InterceptAction::Reject => {
                // Update DB status to rejected
                {
                    let db = state.db.lock().await;
                    let _ = db::update_request_status(&db, &request_id, "rejected");
                }
                emit_event(&state, "request_update", serde_json::json!({
                    "id": request_id,
                    "session_id": session_id,
                    "project_name": session_info.project_name,
                    "status": "rejected",
                }));
                return Ok(Response::builder()
                    .status(StatusCode::FORBIDDEN)
                    .body(Full::new(Bytes::from(r#"{"error":"Request rejected by inspector"}"#)))
                    .unwrap());
            }
        }

        // Restore status to pending for normal flow
        {
            let db = state.db.lock().await;
            let _ = db::update_request_status(&db, &request_id, "pending");
        }
    }

    // ── Routing decision ─────────────────────────────────────────────────────
    let (resolved_base_url, final_body_bytes, routing_category, routed_to_url, rule_api_key) = {
        let config = state.routing_config.read().await;
        if config.enabled {
            let body_val = serde_json::from_slice::<serde_json::Value>(&body_bytes)
                .unwrap_or_default();
            let rules = state.routing_rules.read().await;
            let category = routing::classify_intent(&config, &incoming_api_key, &body_val, &rules).await;
            match routing::match_rule(&rules, &category) {
                Some(rule) => {
                    let mut out_body = body_bytes.clone();
                    if !rule.prompt_override.is_empty() {
                        out_body = Bytes::from(routing::apply_prompt_override(&out_body, &rule.prompt_override));
                    }
                    if !rule.model_override.is_empty() {
                        out_body = Bytes::from(routing::apply_model_override(&out_body, &rule.model_override));
                    }
                    let routed = if rule.target_url != state.upstream_url {
                        rule.target_url.clone()
                    } else {
                        String::new()
                    };
                    (rule.target_url.clone(), out_body, category, routed, rule.api_key.clone())
                }
                None => (state.upstream_url.clone(), body_bytes.clone(), category, String::new(), String::new()),
            }
        } else {
            (state.upstream_url.clone(), body_bytes.clone(), String::new(), String::new(), String::new())
        }
    };

    // Override API key headers if the matched rule specifies its own key
    if !rule_api_key.is_empty() {
        upstream_headers.insert("x-api-key".to_string(), rule_api_key.clone());
        upstream_headers.insert("authorization".to_string(), format!("Bearer {rule_api_key}"));
    }

    // Update routing fields in DB if routing produced a category
    if !routing_category.is_empty() || !routed_to_url.is_empty() {
        let db = state.db.lock().await;
        let _ = db.execute(
            "UPDATE requests SET routing_category = ?1, routed_to_url = ?2 WHERE id = ?3",
            rusqlite::params![routing_category, routed_to_url, request_id],
        );
    }

    // Forward to upstream (URL from AppState — configurable for testing)
    let upstream_url = format!("{resolved_base_url}{path}");
    let client = reqwest::Client::builder()
        .use_rustls_tls()
        .danger_accept_invalid_certs(resolved_base_url.starts_with("http://"))
        .build()?;

    let mut req_builder = client.request(method.clone(), &upstream_url);
    for (k, v) in &upstream_headers {
        req_builder = req_builder.header(k, v);
    }
    req_builder = req_builder.body(final_body_bytes.to_vec());

    let upstream_resp = req_builder.send().await?;
    let status = upstream_resp.status();
    let resp_headers = upstream_resp.headers().clone();

    let mut resp_log_headers: HashMap<String, String> = HashMap::new();
    for (name, value) in &resp_headers {
        resp_log_headers.insert(name.as_str().to_string(), value.to_str().unwrap_or("").to_string());
    }

    let content_type = resp_headers
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();

    let is_sse = content_type.contains("text/event-stream");

    if is_sse {
        let stream = upstream_resp.bytes_stream();
        let (done_tx, done_rx) = oneshot::channel::<Vec<u8>>();
        let tee_stream = SseTeeStream::new(stream, done_tx);

        // chunk_tx is moved (not cloned) into the spawn so the channel closes when the
        // stream ends, allowing chunk_rx.recv() to return None and unblock the collector.
        let (chunk_tx, chunk_rx) = tokio::sync::mpsc::channel::<Result<Bytes, std::io::Error>>(64);

        let mut pinned = Box::pin(tee_stream);
        tokio::spawn(async move {
            while let Some(chunk) = pinned.next().await {
                match chunk {
                    Ok(bytes) => {
                        if chunk_tx.send(Ok(bytes)).await.is_err() {
                            break;
                        }
                    }
                    Err(e) => {
                        let _ = chunk_tx
                            .send(Err(std::io::Error::new(std::io::ErrorKind::Other, e.to_string())))
                            .await;
                        break;
                    }
                }
            }
            // chunk_tx dropped here → channel closes → chunk_rx.recv() returns None
        });

        let state_clone = Arc::clone(&state);
        let request_id_clone = request_id.clone();
        let session_id_clone = session_id.clone();
        let body_str_clone = body_str.clone();
        let timestamp_clone = timestamp.clone();
        let project_name = session_info.project_name.clone();
        let resp_headers_json = serde_json::to_string(&resp_log_headers).unwrap_or_default();
        let status_code = status.as_u16() as i64;

        tokio::spawn(async move {
            match done_rx.await {
                Ok(raw_bytes) => {
                    let duration_ms = start.elapsed().as_millis() as i64;
                    let (content, input_tokens, output_tokens) = parse_sse_content(&raw_bytes);
                    let body_json = serde_json::to_string(&serde_json::json!({
                        "accumulated_content": content,
                        "raw_sse": String::from_utf8_lossy(&raw_bytes).to_string(),
                    }))
                    .unwrap_or_default();

                    let db = state_clone.db.lock().await;
                    if let Err(e) = db::update_request_complete(
                        &db,
                        &request_id_clone,
                        status_code,
                        &resp_headers_json,
                        &body_json,
                        input_tokens,
                        output_tokens,
                        duration_ms,
                        "complete",
                    ) {
                        warn!("Failed to update SSE request: {e}");
                    }
                    // Extract and store file accesses
                    {
                        let accesses = supervisor::extract_file_accesses(&body_str_clone);
                        for (path, atype) in &accesses {
                            let _ = db::insert_file_access(&db, &session_id_clone, &request_id_clone, path, atype, &timestamp_clone);
                        }
                    }

                    drop(db);

                    emit_event(&state_clone, "request_update", serde_json::json!({
                        "id": request_id_clone,
                        "session_id": session_id_clone,
                        "project_name": project_name,
                        "status": "complete",
                        "response_status": status_code,
                        "input_tokens": input_tokens,
                        "output_tokens": output_tokens,
                        "duration_ms": duration_ms,
                        "is_streaming": true,
                    }));
                }
                Err(e) => {
                    warn!("SSE done channel error: {e}");
                    let db = state_clone.db.lock().await;
                    let _ = db::update_request_status(&db, &request_id_clone, "error");
                }
            }
        });

        let mut all_bytes = Vec::new();
        let mut rx = chunk_rx;
        while let Some(chunk) = rx.recv().await {
            match chunk {
                Ok(b) => all_bytes.extend_from_slice(&b),
                Err(_) => break,
            }
        }

        let mut resp_builder = Response::builder().status(status);
        for (k, v) in &resp_log_headers {
            resp_builder = resp_builder.header(k, v);
        }
        Ok(resp_builder.body(Full::new(Bytes::from(all_bytes)))?)
    } else {
        let resp_bytes = upstream_resp.bytes().await?;
        let resp_body_str = String::from_utf8_lossy(&resp_bytes).to_string();
        let duration_ms = start.elapsed().as_millis() as i64;

        let (input_tokens, output_tokens) =
            serde_json::from_str::<serde_json::Value>(&resp_body_str)
                .ok()
                .map(|v| {
                    let usage = v.get("usage");
                    let base = usage.and_then(|u| u.get("input_tokens")).and_then(|t| t.as_i64()).unwrap_or(0);
                    let cache_create = usage.and_then(|u| u.get("cache_creation_input_tokens")).and_then(|t| t.as_i64()).unwrap_or(0);
                    let cache_read = usage.and_then(|u| u.get("cache_read_input_tokens")).and_then(|t| t.as_i64()).unwrap_or(0);
                    let total_input = base + cache_create + cache_read;
                    let inp = if total_input > 0 { Some(total_input) } else { None };
                    let out = usage.and_then(|u| u.get("output_tokens")).and_then(|t| t.as_i64());
                    (inp, out)
                })
                .unwrap_or((None, None));

        let status_str = if status.is_success() { "complete" } else { "error" };

        {
            let db = state.db.lock().await;
            if let Err(e) = db::update_request_complete(
                &db,
                &request_id,
                status.as_u16() as i64,
                &serde_json::to_string(&resp_log_headers).unwrap_or_default(),
                &resp_body_str,
                input_tokens,
                output_tokens,
                duration_ms,
                status_str,
            ) {
                warn!("Failed to update request: {e}");
            }

            // Extract and store file accesses
            {
                let accesses = supervisor::extract_file_accesses(&body_str);
                for (path, atype) in &accesses {
                    let _ = db::insert_file_access(&db, &session_id, &request_id, path, atype, &timestamp);
                }
            }
        }

        emit_event(&state, "request_update", serde_json::json!({
            "id": request_id,
            "session_id": session_id,
            "project_name": session_info.project_name,
            "status": status_str,
            "response_status": status.as_u16(),
            "input_tokens": input_tokens,
            "output_tokens": output_tokens,
            "duration_ms": duration_ms,
            "is_streaming": false,
        }));

        let mut resp_builder = Response::builder().status(status);
        for (k, v) in &resp_log_headers {
            resp_builder = resp_builder.header(k, v);
        }
        Ok(resp_builder.body(Full::new(resp_bytes))?)
    }
}

fn emit_event(state: &AppState, event_type: &str, data: serde_json::Value) {
    let event = DashboardEvent {
        event_type: event_type.to_string(),
        data,
    };
    let _ = state.event_tx.send(event);
}

/// Detect agent type and task description from request body.
/// Returns (agent_type, agent_task).
///
/// Key distinction: the main agent has the Agent tool description in its system
/// prompt (containing "Launch a new agent to handle complex"), while sub-agents
/// do NOT have this tool. Sub-agent type is then refined by looking at their
/// specific system prompt content.
fn detect_agent_type(body_str: &str) -> (String, String) {
    // Main agent always has the Agent tool available
    let has_agent_tool = body_str.contains("Launch a new agent to handle complex");

    if has_agent_tool {
        return ("main".to_string(), String::new());
    }

    // This is a sub-agent — determine which kind
    let agent_type = if body_str.contains("Fast agent specialized for exploring") {
        "explore"
    } else if body_str.contains("Software architect agent for designing") {
        "plan"
    } else if body_str.contains("Performs ultra-granular per-function deep analysis") {
        "audit"
    } else if body_str.contains("configure the user's Claude Code status line") {
        "statusline"
    } else if body_str.contains("Claude Code (the CLI tool)") && body_str.contains("claude-code-guide") {
        "guide"
    } else {
        "sub"
    };

    let task = extract_agent_task(body_str);
    (agent_type.to_string(), task)
}

/// Extract a short task description from the sub-agent's first user message.
/// Skips <system-reminder> blocks to find the actual task prompt.
fn extract_agent_task(body_str: &str) -> String {
    let v = match serde_json::from_str::<serde_json::Value>(body_str) {
        Ok(v) => v,
        Err(_) => return String::new(),
    };
    let messages = match v.get("messages").and_then(|m| m.as_array()) {
        Some(m) => m,
        None => return String::new(),
    };
    // For sub-agents, the first user message contains the task
    let user_msg = messages.iter().find(|m| {
        m.get("role").and_then(|r| r.as_str()) == Some("user")
    });
    let text = match user_msg {
        Some(m) => {
            if let Some(s) = m.get("content").and_then(|c| c.as_str()) {
                s.to_string()
            } else if let Some(arr) = m.get("content").and_then(|c| c.as_array()) {
                // Find the last text block that isn't a system-reminder
                arr.iter().rev()
                    .filter_map(|b| {
                        let t = b.get("text").and_then(|t| t.as_str()).unwrap_or("");
                        if t.starts_with("<system-reminder>") || t.is_empty() {
                            None
                        } else {
                            Some(t.to_string())
                        }
                    })
                    .next()
                    .unwrap_or_default()
            } else {
                String::new()
            }
        }
        None => return String::new(),
    };
    // Trim and truncate to ~120 chars for display
    let trimmed = text.trim();
    if trimmed.len() <= 120 {
        trimmed.to_string()
    } else {
        format!("{}…", &trimmed[..trimmed.char_indices().take(120).last().map(|(i,_)| i).unwrap_or(120)])
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::session::new_session_cache;
    use bytes::Bytes;
    use http_body_util::Full;
    use hyper::service::service_fn;
    use hyper::server::conn::http1;
    use hyper_util::rt::TokioIo;
    use rusqlite::Connection;
    use tokio::net::TcpListener;
    use tokio::sync::broadcast;

    // ── Mock upstream helpers ─────────────────────────────────────────────────

    /// Spawn a one-shot HTTP server. The `handler` closure returns (status, content_type, body).
    async fn spawn_mock_upstream(
        handler: impl Fn() -> (u16, &'static str, &'static str) + Send + Sync + 'static,
    ) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            if let Ok((stream, _)) = listener.accept().await {
                let io = TokioIo::new(stream);
                let _ = http1::Builder::new()
                    .serve_connection(io, service_fn(move |_req| {
                        let (status, ct, body) = handler();
                        async move {
                            Ok::<_, hyper::Error>(
                                hyper::Response::builder()
                                    .status(status)
                                    .header("content-type", ct)
                                    .body(Full::new(Bytes::from(body)))
                                    .unwrap(),
                            )
                        }
                    }))
                    .await;
            }
        });

        format!("http://{addr}")
    }

    // ── Proxy server helpers ──────────────────────────────────────────────────

    /// Start a full proxy server pointing at `upstream_url`, return (proxy_url, state).
    async fn spawn_proxy(upstream_url: String) -> (String, Arc<AppState>) {
        let conn = Connection::open_in_memory().unwrap();
        db::init_db(&conn).unwrap();
        let (tx, _) = broadcast::channel(16);
        let state = AppState::with_upstream(conn, tx, upstream_url);

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let state_clone = Arc::clone(&state);

        tokio::spawn(async move {
            // Accept a fixed number of connections in tests
            for _ in 0..3 {
                let Ok((stream, peer_addr)) = listener.accept().await else { break };
                let state = Arc::clone(&state_clone);
                let cache = new_session_cache();
                let io = TokioIo::new(stream);
                tokio::spawn(async move {
                    let _ = http1::Builder::new()
                        .serve_connection(io, service_fn(move |req| {
                            let state = Arc::clone(&state);
                            let cache = cache.clone();
                            async move { handle_request(req, state, peer_addr, cache).await }
                        }))
                        .await;
                });
            }
        });

        (format!("http://{addr}"), state)
    }

    const MOCK_JSON_BODY: &str = r#"{
        "id": "msg_test",
        "type": "message",
        "role": "assistant",
        "content": [{"type": "text", "text": "Hi!"}],
        "model": "claude-haiku-4-5-20251001",
        "stop_reason": "end_turn",
        "usage": {"input_tokens": 10, "output_tokens": 5}
    }"#;

    const MOCK_SSE_BODY: &str = concat!(
        "event: message_start\n",
        "data: {\"type\":\"message_start\",\"message\":{\"usage\":{\"input_tokens\":8,\"output_tokens\":0}}}\n\n",
        "event: content_block_delta\n",
        "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Hello\"}}\n\n",
        "event: message_delta\n",
        "data: {\"type\":\"message_delta\",\"usage\":{\"output_tokens\":4}}\n\n",
        "data: [DONE]\n\n",
    );

    // ── Non-streaming tests ───────────────────────────────────────────────────

    #[tokio::test]
    async fn non_streaming_request_proxied_and_stored() {
        let upstream = spawn_mock_upstream(|| (200, "application/json", MOCK_JSON_BODY)).await;
        let (proxy_url, state) = spawn_proxy(upstream).await;

        let resp = reqwest::Client::new()
            .post(format!("{proxy_url}/v1/messages"))
            .header("content-type", "application/json")
            .header("x-api-key", "test-key")
            .body(r#"{"model":"claude","messages":[{"role":"user","content":"hi"}],"max_tokens":10}"#)
            .send()
            .await
            .unwrap();

        assert_eq!(resp.status(), 200);
        let body: serde_json::Value = serde_json::from_str(&resp.text().await.unwrap()).unwrap();
        assert_eq!(body["usage"]["input_tokens"], 10);

        // Wait for async DB write to finish
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        let db = state.db.lock().await;
        let reqs = db::get_requests(&db, None, 10, 0).unwrap();
        assert_eq!(reqs.len(), 1);
        assert_eq!(reqs[0].status, "complete");
        assert_eq!(reqs[0].input_tokens, Some(10));
        assert_eq!(reqs[0].output_tokens, Some(5));
        assert_eq!(reqs[0].method, "POST");
        assert_eq!(reqs[0].path, "/v1/messages");
    }

    #[tokio::test]
    async fn api_key_excluded_from_logged_headers() {
        let upstream = spawn_mock_upstream(|| (200, "application/json", MOCK_JSON_BODY)).await;
        let (proxy_url, state) = spawn_proxy(upstream).await;

        reqwest::Client::new()
            .post(format!("{proxy_url}/v1/messages"))
            .header("content-type", "application/json")
            .header("x-api-key", "super-secret-key")
            .body(r#"{"model":"claude","messages":[],"max_tokens":1}"#)
            .send()
            .await
            .unwrap();

        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        let db = state.db.lock().await;
        let reqs = db::get_requests(&db, None, 10, 0).unwrap();
        assert_eq!(reqs.len(), 1);

        // x-api-key must NOT appear in stored headers
        let stored_headers: serde_json::Value =
            serde_json::from_str(&reqs[0].request_headers).unwrap();
        assert!(stored_headers.get("x-api-key").is_none(), "x-api-key must not be stored");
    }

    #[tokio::test]
    async fn error_response_stored_with_error_status() {
        let upstream = spawn_mock_upstream(|| (401, "application/json", r#"{"error":"Unauthorized"}"#)).await;
        let (proxy_url, state) = spawn_proxy(upstream).await;

        let resp = reqwest::Client::new()
            .post(format!("{proxy_url}/v1/messages"))
            .header("content-type", "application/json")
            .body(r#"{"model":"claude","messages":[],"max_tokens":1}"#)
            .send()
            .await
            .unwrap();

        assert_eq!(resp.status(), 401);

        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        let db = state.db.lock().await;
        let reqs = db::get_requests(&db, None, 10, 0).unwrap();
        assert_eq!(reqs[0].status, "error");
        assert_eq!(reqs[0].response_status, Some(401));
    }

    // ── Streaming (SSE) tests ─────────────────────────────────────────────────

    #[tokio::test]
    async fn streaming_request_proxied_and_stored() {
        let upstream =
            spawn_mock_upstream(|| (200, "text/event-stream", MOCK_SSE_BODY)).await;
        let (proxy_url, state) = spawn_proxy(upstream).await;

        let resp = reqwest::Client::new()
            .post(format!("{proxy_url}/v1/messages"))
            .header("content-type", "application/json")
            .body(r#"{"model":"claude","messages":[{"role":"user","content":"hi"}],"max_tokens":10,"stream":true}"#)
            .send()
            .await
            .unwrap();

        assert_eq!(resp.status(), 200);
        let body_text = resp.text().await.unwrap();
        assert!(body_text.contains("message_start"), "SSE body must be forwarded");

        // Wait for background SSE-parse task
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        let db = state.db.lock().await;
        let reqs = db::get_requests(&db, None, 10, 0).unwrap();
        assert_eq!(reqs.len(), 1);
        assert!(reqs[0].is_streaming);
        assert_eq!(reqs[0].status, "complete");
        assert_eq!(reqs[0].input_tokens, Some(8));
        assert_eq!(reqs[0].output_tokens, Some(4));

        // Accumulated content must be stored
        let body: serde_json::Value =
            serde_json::from_str(reqs[0].response_body.as_deref().unwrap()).unwrap();
        assert_eq!(body["accumulated_content"], "Hello");
    }

    #[tokio::test]
    async fn upstream_unreachable_returns_bad_gateway() {
        let (proxy_url, _state) = spawn_proxy("http://127.0.0.1:1".to_string()).await;

        let resp = reqwest::Client::new()
            .post(format!("{proxy_url}/v1/messages"))
            .header("content-type", "application/json")
            .body(r#"{"model":"claude","messages":[],"max_tokens":1}"#)
            .send()
            .await
            .unwrap();

        assert_eq!(resp.status(), 502);
    }

    // ── Intercept test helpers ──────────────────────────────────────────────

    /// Wait until at least one request appears in the intercept map.
    async fn wait_for_intercept(state: &Arc<AppState>) {
        for _ in 0..50 {
            {
                let map = state.intercepted.lock().unwrap();
                if !map.is_empty() { return; }
            }
            tokio::time::sleep(tokio::time::Duration::from_millis(20)).await;
        }
        panic!("Timed out waiting for intercepted request");
    }

    // ── Intercept tests ──────────────────────────────────────────────────────

    #[tokio::test]
    async fn intercept_forward_original_passes_to_upstream() {
        let upstream = spawn_mock_upstream(|| (200, "application/json", MOCK_JSON_BODY)).await;
        let (proxy_url, state) = spawn_proxy(upstream).await;

        // Enable intercept
        state.intercept_enabled.store(true, std::sync::atomic::Ordering::Relaxed);

        // Spawn the proxy request in background
        let proxy_url_clone = proxy_url.clone();
        let client_task = tokio::spawn(async move {
            reqwest::Client::new()
                .post(format!("{proxy_url_clone}/v1/messages"))
                .header("content-type", "application/json")
                .body(r#"{"model":"claude","messages":[],"max_tokens":1}"#)
                .send()
                .await
                .unwrap()
        });

        // Wait for the request to be intercepted
        wait_for_intercept(&state).await;

        // Check it's in intercepted state
        {
            let db = state.db.lock().await;
            let reqs = db::get_requests(&db, None, 10, 0).unwrap();
            assert_eq!(reqs.len(), 1);
            assert_eq!(reqs[0].status, "intercepted");
        }

        // Forward original
        {
            let mut map = state.intercepted.lock().unwrap();
            let ids: Vec<String> = map.keys().cloned().collect();
            assert_eq!(ids.len(), 1);
            let sender = map.remove(&ids[0]).unwrap();
            sender.send(crate::types::InterceptAction::ForwardOriginal).unwrap();
        }

        let resp = client_task.await.unwrap();
        assert_eq!(resp.status(), 200);
        let body: serde_json::Value = serde_json::from_str(&resp.text().await.unwrap()).unwrap();
        assert_eq!(body["usage"]["input_tokens"], 10);
    }

    #[tokio::test]
    async fn intercept_reject_returns_403() {
        let upstream = spawn_mock_upstream(|| (200, "application/json", MOCK_JSON_BODY)).await;
        let (proxy_url, state) = spawn_proxy(upstream).await;

        state.intercept_enabled.store(true, std::sync::atomic::Ordering::Relaxed);

        let proxy_url_clone = proxy_url.clone();
        let client_task = tokio::spawn(async move {
            reqwest::Client::new()
                .post(format!("{proxy_url_clone}/v1/messages"))
                .header("content-type", "application/json")
                .body(r#"{"model":"claude","messages":[],"max_tokens":1}"#)
                .send()
                .await
                .unwrap()
        });

        wait_for_intercept(&state).await;

        // Reject
        {
            let mut map = state.intercepted.lock().unwrap();
            let ids: Vec<String> = map.keys().cloned().collect();
            assert_eq!(ids.len(), 1);
            let sender = map.remove(&ids[0]).unwrap();
            sender.send(crate::types::InterceptAction::Reject).unwrap();
        }

        let resp = client_task.await.unwrap();
        assert_eq!(resp.status(), 403);

        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        let db = state.db.lock().await;
        let reqs = db::get_requests(&db, None, 10, 0).unwrap();
        assert_eq!(reqs[0].status, "rejected");
    }

    #[tokio::test]
    async fn intercept_forward_modified_sends_new_body() {
        // Use a mock that echoes the request body back
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            if let Ok((stream, _)) = listener.accept().await {
                let io = TokioIo::new(stream);
                let _ = http1::Builder::new()
                    .serve_connection(io, service_fn(|req: Request<hyper::body::Incoming>| async {
                        use http_body_util::BodyExt;
                        let body = req.into_body().collect().await.unwrap().to_bytes();
                        let body_str = String::from_utf8_lossy(&body).to_string();
                        // Parse and return the model field
                        let parsed: serde_json::Value = serde_json::from_str(&body_str).unwrap_or_default();
                        let resp_body = serde_json::json!({
                            "model": parsed.get("model").and_then(|m| m.as_str()).unwrap_or("unknown"),
                            "usage": {"input_tokens": 1, "output_tokens": 1}
                        });
                        Ok::<_, hyper::Error>(
                            hyper::Response::builder()
                                .status(200)
                                .header("content-type", "application/json")
                                .body(Full::new(Bytes::from(serde_json::to_string(&resp_body).unwrap())))
                                .unwrap()
                        )
                    }))
                    .await;
            }
        });

        let upstream_url = format!("http://{addr}");
        let (proxy_url, state) = spawn_proxy(upstream_url).await;
        state.intercept_enabled.store(true, std::sync::atomic::Ordering::Relaxed);

        let proxy_url_clone = proxy_url.clone();
        let client_task = tokio::spawn(async move {
            reqwest::Client::new()
                .post(format!("{proxy_url_clone}/v1/messages"))
                .header("content-type", "application/json")
                .body(r#"{"model":"original-model","messages":[],"max_tokens":1}"#)
                .send()
                .await
                .unwrap()
        });

        wait_for_intercept(&state).await;

        // Forward modified with a new model
        {
            let mut map = state.intercepted.lock().unwrap();
            let ids: Vec<String> = map.keys().cloned().collect();
            assert_eq!(ids.len(), 1);
            let sender = map.remove(&ids[0]).unwrap();
            sender.send(crate::types::InterceptAction::ForwardModified {
                body: r#"{"model":"modified-model","messages":[],"max_tokens":1}"#.to_string(),
            }).unwrap();
        }

        let resp = client_task.await.unwrap();
        assert_eq!(resp.status(), 200);
        let body: serde_json::Value = serde_json::from_str(&resp.text().await.unwrap()).unwrap();
        assert_eq!(body["model"], "modified-model");
    }

    #[tokio::test]
    async fn intercept_disabled_passes_through() {
        let upstream = spawn_mock_upstream(|| (200, "application/json", MOCK_JSON_BODY)).await;
        let (proxy_url, state) = spawn_proxy(upstream).await;

        // Intercept is disabled by default
        assert!(!state.intercept_enabled.load(std::sync::atomic::Ordering::Relaxed));

        let resp = reqwest::Client::new()
            .post(format!("{proxy_url}/v1/messages"))
            .header("content-type", "application/json")
            .body(r#"{"model":"claude","messages":[],"max_tokens":1}"#)
            .send()
            .await
            .unwrap();

        assert_eq!(resp.status(), 200);
        // No intercept map entries
        let map = state.intercepted.lock().unwrap();
        assert!(map.is_empty());
    }

    #[test]
    fn detect_agent_type_main_has_agent_tool() {
        let body = r#"{"model":"claude-opus","system":"Launch a new agent to handle complex tasks","messages":[{"role":"user","content":"hello"}]}"#;
        assert_eq!(detect_agent_type(body).0, "main");
    }

    #[test]
    fn detect_agent_type_main_even_with_explore_mention() {
        // Main agent's system prompt mentions sub-agent types but also has Agent tool
        let body = r#"{"system":"Launch a new agent to handle complex tasks. Fast agent specialized for exploring"}"#;
        assert_eq!(detect_agent_type(body).0, "main");
    }

    #[test]
    fn detect_agent_type_explore() {
        let body = r#"{"model":"claude-haiku","system":"Fast agent specialized for exploring codebases","messages":[{"role":"user","content":"find auth code"}]}"#;
        let (t, task) = detect_agent_type(body);
        assert_eq!(t, "explore");
        assert_eq!(task, "find auth code");
    }

    #[test]
    fn detect_agent_type_plan() {
        let body = r#"{"system":"Software architect agent for designing plans","messages":[{"role":"user","content":"plan the migration"}]}"#;
        assert_eq!(detect_agent_type(body).0, "plan");
    }

    #[test]
    fn detect_agent_type_audit() {
        let body = r#"{"system":"Performs ultra-granular per-function deep analysis","messages":[{"role":"user","content":"analyze crypto"}]}"#;
        assert_eq!(detect_agent_type(body).0, "audit");
    }

    #[test]
    fn detect_agent_type_generic_sub() {
        // No Agent tool, no known sub-agent keywords → generic sub
        let body = r#"{"model":"claude-opus","messages":[{"role":"user","content":"write the report"}]}"#;
        let (t, task) = detect_agent_type(body);
        assert_eq!(t, "sub");
        assert_eq!(task, "write the report");
    }

    #[test]
    fn detect_agent_task_skips_system_reminders() {
        let body = r#"{"messages":[{"role":"user","content":[{"type":"text","text":"<system-reminder>ignore</system-reminder>"},{"type":"text","text":"actual task here"}]}]}"#;
        let (_, task) = detect_agent_type(body);
        assert_eq!(task, "actual task here");
    }

    #[tokio::test]
    async fn routing_disabled_passes_to_default_upstream() {
        // With routing disabled (default), requests go to the configured upstream
        let upstream = spawn_mock_upstream(|| (200, "application/json", MOCK_JSON_BODY)).await;
        let (proxy_url, state) = spawn_proxy(upstream.clone()).await;

        // Routing is disabled by default
        {
            let config = state.routing_config.read().await;
            assert!(!config.enabled);
        }

        let resp = reqwest::Client::new()
            .post(format!("{proxy_url}/v1/messages"))
            .header("content-type", "application/json")
            .header("x-api-key", "test-key")
            .body(r#"{"model":"claude","messages":[{"role":"user","content":"hi"}],"max_tokens":10}"#)
            .send()
            .await
            .unwrap();

        assert_eq!(resp.status(), 200);
        let body: serde_json::Value = serde_json::from_str(&resp.text().await.unwrap()).unwrap();
        assert_eq!(body["usage"]["input_tokens"], 10);

        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
        let db = state.db.lock().await;
        let reqs = db::get_requests(&db, None, 10, 0).unwrap();
        assert_eq!(reqs.len(), 1);
        // routing_category should be empty when routing is disabled
        assert_eq!(reqs[0].routing_category, "");
    }

    #[tokio::test]
    async fn routing_with_enabled_config_updates_db_fields() {
        // Setup a mock classifier that returns "code_gen"
        let classifier_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let classifier_addr = classifier_listener.local_addr().unwrap();
        tokio::spawn(async move {
            // Accept multiple connections for classifier calls
            for _ in 0..5 {
                let Ok((stream, _)) = classifier_listener.accept().await else { break };
                let io = TokioIo::new(stream);
                let _ = http1::Builder::new()
                    .serve_connection(io, service_fn(|_req: hyper::Request<hyper::body::Incoming>| async {
                        Ok::<_, hyper::Error>(
                            hyper::Response::builder()
                                .status(200)
                                .header("content-type", "application/json")
                                .body(Full::new(Bytes::from(r#"{"choices":[{"message":{"content":"code_gen"}}]}"#)))
                                .unwrap()
                        )
                    }))
                    .await;
            }
        });

        let upstream = spawn_mock_upstream(|| (200, "application/json", MOCK_JSON_BODY)).await;
        let (proxy_url, state) = spawn_proxy(upstream.clone()).await;

        // Enable routing with classifier URL pointing to mock
        {
            let mut config = state.routing_config.write().await;
            config.enabled = true;
            config.classifier_base_url = format!("http://{classifier_addr}");
            config.classifier_api_key = "test-key".to_string();
        }

        // Add a routing rule for code_gen that routes to the same upstream (no redirect)
        {
            let mut rules = state.routing_rules.write().await;
            rules.push(crate::types::RoutingRule {
                id: "r1".to_string(),
                priority: 10,
                enabled: true,
                category: "code_gen".to_string(),
                description: String::new(),
                target_url: upstream.clone(),
                api_key: String::new(),
                prompt_override: String::new(),
                model_override: String::new(),
                label: "test".to_string(),
            });
        }

        let resp = reqwest::Client::new()
            .post(format!("{proxy_url}/v1/messages"))
            .header("content-type", "application/json")
            .header("x-api-key", "test-key")
            .body(r#"{"model":"claude","messages":[{"role":"user","content":"write some code"}],"max_tokens":10}"#)
            .send()
            .await
            .unwrap();

        assert_eq!(resp.status(), 200);

        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        let db = state.db.lock().await;
        let reqs = db::get_requests(&db, None, 10, 0).unwrap();
        assert_eq!(reqs.len(), 1);
        assert_eq!(reqs[0].routing_category, "code_gen");
    }
}

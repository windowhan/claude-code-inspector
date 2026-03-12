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
use crate::session::{resolve_session, SessionCache};
use crate::sse_tee::{parse_sse_content, SseTeeStream};
use crate::types::{AppState, DashboardEvent, RequestRecord, SessionRecord};

pub async fn handle_request(
    req: Request<hyper::body::Incoming>,
    state: Arc<AppState>,
    peer_addr: SocketAddr,
    session_cache: SessionCache,
) -> Result<Response<Full<Bytes>>, hyper::Error> {
    match handle_inner(req, state, peer_addr, session_cache).await {
        Ok(resp) => Ok(resp),
        Err(e) => {
            error!("Proxy error: {e}");
            Ok(Response::builder()
                .status(StatusCode::BAD_GATEWAY)
                .body(Full::new(Bytes::from(format!("Proxy error: {e}"))))
                .unwrap())
        }
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
    ];

    let headers = req.headers().clone();
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
    let body_bytes = req.into_body().collect().await?.to_bytes();
    let body_str = String::from_utf8_lossy(&body_bytes).to_string();

    // Determine if streaming
    let is_streaming = serde_json::from_str::<serde_json::Value>(&body_str)
        .ok()
        .and_then(|v| v.get("stream").and_then(|s| s.as_bool()))
        .unwrap_or(false);

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
    }));

    // Forward to upstream (URL from AppState — configurable for testing)
    let upstream_url = format!("{}{path}", state.upstream_url);
    let client = reqwest::Client::builder()
        .use_rustls_tls()
        .danger_accept_invalid_certs(state.upstream_url.starts_with("http://"))
        .build()?;

    let mut req_builder = client.request(method.clone(), &upstream_url);
    for (k, v) in &upstream_headers {
        req_builder = req_builder.header(k, v);
    }
    req_builder = req_builder.body(body_bytes.to_vec());

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
                    let inp = v.get("usage")
                        .and_then(|u| u.get("input_tokens"))
                        .and_then(|t| t.as_i64());
                    let out = v.get("usage")
                        .and_then(|u| u.get("output_tokens"))
                        .and_then(|t| t.as_i64());
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
}

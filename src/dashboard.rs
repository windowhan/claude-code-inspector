use std::sync::Arc;
use std::convert::Infallible;

use bytes::Bytes;
use futures::StreamExt;
use http_body_util::{Full, BodyExt, StreamBody};
use hyper::body::Frame;
use hyper::{Request, Response, StatusCode};
use tracing::warn;
use uuid::Uuid;

use crate::db;
use crate::functions;
use crate::intercept;
use crate::routing;
use crate::supervisor;
use crate::supervisor_llm;
use crate::types::{AppState, InterceptAction, RoutingConfig, RoutingRule};

// A boxed body type that can be either Full<Bytes> or a streaming body
type BoxBody = http_body_util::combinators::BoxBody<Bytes, Infallible>;

/// Built frontend assets (populated by `npm run build` → `src/assets/dist/`)
static DIST: include_dir::Dir<'_> = include_dir::include_dir!("$CARGO_MANIFEST_DIR/src/assets/dist");

/// Embedded single-file fallback dashboard (always compiled in)
const FALLBACK_HTML: &str = include_str!("assets/dashboard.html");

fn full_to_box(resp: Response<Full<Bytes>>) -> Response<BoxBody> {
    resp.map(|b| b.map_err(|never| match never {}).boxed())
}

pub async fn handle_dashboard(
    req: Request<hyper::body::Incoming>,
    state: Arc<AppState>,
) -> Result<Response<BoxBody>, Infallible> {
    let method = req.method().as_str().to_uppercase();
    let path  = req.uri().path().to_string();
    let query = req.uri().query().unwrap_or("").to_string();

    // Read body for methods that may have a request body
    let request_body = if method == "POST" || method == "PUT" || method == "PATCH" {
        let body = req.into_body().collect().await.ok()
            .map(|b| b.to_bytes().to_vec())
            .unwrap_or_default();
        Some(body)
    } else {
        // Consume the body to avoid leaking the Incoming
        drop(req);
        None
    };

    Ok(match (method.as_str(), path.as_str()) {
        ("GET", "/api/sessions") => full_to_box(serve_sessions(&state).await),
        ("DELETE", p) if p.starts_with("/api/sessions/") => {
            full_to_box(serve_delete_session(&state, p.trim_start_matches("/api/sessions/")).await)
        }
        ("GET", "/api/requests") => full_to_box(serve_requests(&state, &query).await),
        ("POST", p) if p.starts_with("/api/requests/") && p.ends_with("/star") => {
            let id = p.trim_start_matches("/api/requests/").trim_end_matches("/star");
            full_to_box(serve_toggle_star(&state, id).await)
        }
        ("POST", p) if p.starts_with("/api/requests/") && p.ends_with("/memo") => {
            let id = p.trim_start_matches("/api/requests/").trim_end_matches("/memo");
            let body = request_body.as_deref().unwrap_or(&[]);
            full_to_box(serve_set_memo(&state, id, body).await)
        }
        ("GET", p) if p.starts_with("/api/requests/") => {
            full_to_box(serve_request_detail(&state, p.trim_start_matches("/api/requests/")).await)
        }
        // Intercept API
        ("GET", "/api/intercept/status") => full_to_box(serve_intercept_status(&state)),
        ("POST", "/api/intercept/toggle") => full_to_box(serve_intercept_toggle(&state)),
        ("GET", "/api/intercept/pending") => full_to_box(serve_intercept_pending(&state)),
        ("POST", p) if p.starts_with("/api/intercept/") && p.ends_with("/forward") => {
            let id = p.trim_start_matches("/api/intercept/").trim_end_matches("/forward");
            full_to_box(serve_intercept_forward(&state, id))
        }
        ("POST", p) if p.starts_with("/api/intercept/") && p.ends_with("/forward-modified") => {
            let id = p.trim_start_matches("/api/intercept/").trim_end_matches("/forward-modified");
            let body = request_body.as_deref().unwrap_or(&[]);
            full_to_box(serve_intercept_forward_modified(&state, id, body))
        }
        ("POST", p) if p.starts_with("/api/intercept/") && p.ends_with("/reject") => {
            let id = p.trim_start_matches("/api/intercept/").trim_end_matches("/reject");
            full_to_box(serve_intercept_reject(&state, id))
        }
        // Routing API
        ("GET", "/api/routing/config") => full_to_box(serve_routing_config_get(&state).await),
        ("POST", "/api/routing/config") => full_to_box(serve_routing_config_save(&state, request_body.as_deref().unwrap_or(&[])).await),
        ("GET", "/api/routing/rules") => full_to_box(serve_routing_rules_list(&state).await),
        ("POST", "/api/routing/rules") => full_to_box(serve_routing_rules_create(&state, request_body.as_deref().unwrap_or(&[])).await),
        ("PUT", p) if p.starts_with("/api/routing/rules/") => {
            let id = p.trim_start_matches("/api/routing/rules/");
            full_to_box(serve_routing_rules_update(&state, id, request_body.as_deref().unwrap_or(&[])).await)
        }
        ("DELETE", p) if p.starts_with("/api/routing/rules/") => {
            let id = p.trim_start_matches("/api/routing/rules/");
            full_to_box(serve_routing_rules_delete(&state, id).await)
        }
        ("POST", "/api/routing/reorder") => full_to_box(serve_routing_reorder(&state, request_body.as_deref().unwrap_or(&[])).await),
        ("POST", "/api/routing/test") => full_to_box(serve_routing_test(&state, request_body.as_deref().unwrap_or(&[])).await),
        // Supervisor API
        ("GET", p) if p.starts_with("/api/supervisor/summary/") => {
            let sid = p.trim_start_matches("/api/supervisor/summary/");
            full_to_box(serve_supervisor_summary(&state, sid).await)
        }
        ("GET", p) if p.starts_with("/api/supervisor/coverage/") => {
            let sid = p.trim_start_matches("/api/supervisor/coverage/");
            full_to_box(serve_supervisor_coverage(&state, sid).await)
        }
        ("GET", p) if p.starts_with("/api/supervisor/patterns/") => {
            let sid = p.trim_start_matches("/api/supervisor/patterns/");
            full_to_box(serve_supervisor_patterns(&state, sid).await)
        }
        // Supervisor LLM API
        ("GET", "/api/supervisor/config") => full_to_box(serve_supervisor_config_get(&state).await),
        ("POST", "/api/supervisor/config") => full_to_box(serve_supervisor_config_save(Arc::clone(&state), request_body.as_deref().unwrap_or(&[])).await),
        ("GET", p) if p.starts_with("/api/supervisor/goals/") => {
            let sid = p.trim_start_matches("/api/supervisor/goals/");
            full_to_box(serve_supervisor_goal_get(&state, sid).await)
        }
        ("POST", p) if p.starts_with("/api/supervisor/goals/") => {
            let sid = p.trim_start_matches("/api/supervisor/goals/");
            full_to_box(serve_supervisor_goal_set(&state, sid, request_body.as_deref().unwrap_or(&[])).await)
        }
        ("DELETE", p) if p.starts_with("/api/supervisor/goals/") => {
            let sid = p.trim_start_matches("/api/supervisor/goals/");
            full_to_box(serve_supervisor_goal_delete(&state, sid).await)
        }
        ("POST", "/api/supervisor/refine-goal") => full_to_box(serve_supervisor_refine_goal(&state, request_body.as_deref().unwrap_or(&[])).await),
        ("GET", p) if p.starts_with("/api/supervisor/analyses/") => {
            let sid = p.trim_start_matches("/api/supervisor/analyses/");
            full_to_box(serve_supervisor_analyses(&state, sid, &query).await)
        }
        // Summarizer API
        ("GET", "/api/summarizer/config") => full_to_box(serve_summarizer_config_get(&state).await),
        ("POST", "/api/summarizer/config") => full_to_box(serve_summarizer_config_save(&state, request_body.as_deref().unwrap_or(&[])).await),
        ("POST", "/api/summarize") => full_to_box(serve_summarize(&state, request_body.as_deref().unwrap_or(&[])).await),
        ("POST", "/api/summarizer/models") => full_to_box(serve_summarizer_models(&state, request_body.as_deref().unwrap_or(&[])).await),
        // Files API (Code Viewer)
        ("GET", p) if p.starts_with("/api/files/tree/") => {
            let sid = p.trim_start_matches("/api/files/tree/");
            full_to_box(serve_file_tree(&state, sid).await)
        }
        ("GET", p) if p.starts_with("/api/files/content/") => {
            let sid = p.trim_start_matches("/api/files/content/");
            full_to_box(serve_file_content(&state, sid, &query).await)
        }
        ("GET", p) if p.starts_with("/api/files/requests/") => {
            let sid = p.trim_start_matches("/api/files/requests/");
            full_to_box(serve_file_requests(&state, sid, &query).await)
        }
        ("GET", "/events") => serve_sse_stream(&state),
        ("GET", _) => full_to_box(serve_static(&path)),
        _ => full_to_box(Response::builder().status(StatusCode::METHOD_NOT_ALLOWED)
            .body(Full::new(Bytes::from("Method Not Allowed"))).unwrap()),
    })
}

// ── Static asset serving ──────────────────────────────────────────────────────

fn serve_static(path: &str) -> Response<Full<Bytes>> {
    let asset_path = if path == "/" { "index.html" } else { path.trim_start_matches('/') };

    if let Some(file) = DIST.get_file(asset_path) {
        let mime = guess_mime(asset_path);
        return Response::builder()
            .status(200)
            .header("content-type", mime)
            .body(Full::new(Bytes::from(file.contents())))
            .unwrap();
    }

    // SPA fallback: serve index.html for unknown paths (client-side routing)
    if let Some(index) = DIST.get_file("index.html") {
        return Response::builder()
            .status(200)
            .header("content-type", "text/html; charset=utf-8")
            .body(Full::new(Bytes::from(index.contents())))
            .unwrap();
    }

    // Last resort: compiled-in single-file dashboard
    Response::builder()
        .status(200)
        .header("content-type", "text/html; charset=utf-8")
        .body(Full::new(Bytes::from(FALLBACK_HTML)))
        .unwrap()
}

fn guess_mime(path: &str) -> &'static str {
    match path.rsplit('.').next().unwrap_or("") {
        "html"  => "text/html; charset=utf-8",
        "js"    => "application/javascript",
        "css"   => "text/css",
        "svg"   => "image/svg+xml",
        "ico"   => "image/x-icon",
        "json"  => "application/json",
        "woff2" => "font/woff2",
        _       => "application/octet-stream",
    }
}

// ── API handlers ──────────────────────────────────────────────────────────────

fn json_response(data: serde_json::Value) -> Response<Full<Bytes>> {
    Response::builder()
        .status(200)
        .header("content-type", "application/json")
        .header("access-control-allow-origin", "*")
        .body(Full::new(Bytes::from(serde_json::to_string(&data).unwrap_or_default())))
        .unwrap()
}

fn json_error(msg: &str) -> Response<Full<Bytes>> {
    Response::builder()
        .status(StatusCode::BAD_REQUEST)
        .header("content-type", "application/json")
        .header("access-control-allow-origin", "*")
        .body(Full::new(Bytes::from(serde_json::json!({"error": msg}).to_string())))
        .unwrap()
}

async fn serve_sessions(state: &AppState) -> Response<Full<Bytes>> {
    let db = state.db.lock().await;
    match db::get_session_stats(&db) {
        Ok(s)  => json_response(serde_json::Value::Array(s)),
        Err(e) => { warn!("get_session_stats: {e}"); json_response(serde_json::json!({"error": e.to_string()})) }
    }
}

async fn serve_requests(state: &AppState, query: &str) -> Response<Full<Bytes>> {
    let params     = parse_query(query);
    let session_id = params.get("session_id").map(|s| s.as_str());
    let limit      = params.get("limit").and_then(|v| v.parse::<i64>().ok()).unwrap_or(50);
    let offset     = params.get("offset").and_then(|v| v.parse::<i64>().ok()).unwrap_or(0);
    let starred    = params.get("starred").map(|v| v == "1" || v == "true").unwrap_or(false);
    let search     = params.get("search").cloned().unwrap_or_default();
    let source     = params.get("source").map(|s| s.as_str()).filter(|s| !s.is_empty());

    let db = state.db.lock().await;
    if starred {
        match db::get_starred_requests(&db, limit, offset) {
            Ok(r)  => json_response(serde_json::to_value(&r).unwrap_or_default()),
            Err(e) => { warn!("get_starred_requests: {e}"); json_response(serde_json::json!({"error": e.to_string()})) }
        }
    } else if !search.is_empty() {
        match db::search_requests(&db, &search, session_id, limit, offset) {
            Ok(r)  => json_response(serde_json::to_value(&r).unwrap_or_default()),
            Err(e) => { warn!("search_requests: {e}"); json_response(serde_json::json!({"error": e.to_string()})) }
        }
    } else {
        match db::get_requests(&db, session_id, source, limit, offset) {
            Ok(r)  => json_response(serde_json::to_value(&r).unwrap_or_default()),
            Err(e) => { warn!("get_requests: {e}"); json_response(serde_json::json!({"error": e.to_string()})) }
        }
    }
}

async fn serve_delete_session(state: &AppState, id: &str) -> Response<Full<Bytes>> {
    let db = state.db.lock().await;
    match db::delete_session(&db, id) {
        Ok(())  => json_response(serde_json::json!({"ok": true})),
        Err(e) => { warn!("delete_session({id}): {e}"); json_response(serde_json::json!({"error": e.to_string()})) }
    }
}

async fn serve_toggle_star(state: &AppState, id: &str) -> Response<Full<Bytes>> {
    let db = state.db.lock().await;
    let current = db::get_request_by_id(&db, id)
        .ok()
        .flatten()
        .map(|r| r.starred)
        .unwrap_or(false);
    match db::set_request_starred(&db, id, !current) {
        Ok(()) => json_response(serde_json::json!({"starred": !current})),
        Err(e) => { warn!("set_request_starred({id}): {e}"); json_response(serde_json::json!({"error": e.to_string()})) }
    }
}

async fn serve_set_memo(state: &AppState, id: &str, body: &[u8]) -> Response<Full<Bytes>> {
    let memo = serde_json::from_slice::<serde_json::Value>(body)
        .ok()
        .and_then(|v| v.get("memo").and_then(|m| m.as_str()).map(|s| s.to_string()))
        .unwrap_or_default();
    let db = state.db.lock().await;
    match db::set_request_memo(&db, id, &memo) {
        Ok(()) => json_response(serde_json::json!({"memo": memo})),
        Err(e) => { warn!("set_request_memo({id}): {e}"); json_response(serde_json::json!({"error": e.to_string()})) }
    }
}

async fn serve_request_detail(state: &AppState, id: &str) -> Response<Full<Bytes>> {
    let db = state.db.lock().await;
    match db::get_request_by_id(&db, id) {
        Ok(Some(r)) => json_response(serde_json::to_value(&r).unwrap_or_default()),
        Ok(None)    => Response::builder().status(StatusCode::NOT_FOUND).body(Full::new(Bytes::from("Not Found"))).unwrap(),
        Err(e)      => { warn!("get_request_by_id({id}): {e}"); json_response(serde_json::json!({"error": e.to_string()})) }
    }
}

fn serve_sse_stream(state: &AppState) -> Response<BoxBody> {
    let rx = state.event_tx.subscribe();
    let stream = tokio_stream::wrappers::BroadcastStream::new(rx)
        .filter_map(|result| async {
            match result {
                Ok(ev) => {
                    let data = serde_json::to_string(&ev).unwrap_or_default();
                    let chunk = format!("event: {}\ndata: {}\n\n", ev.event_type, data);
                    Some(Ok(Frame::data(Bytes::from(chunk))))
                }
                Err(_) => None, // lagged — skip
            }
        });
    let body = StreamBody::new(stream).map_err(|never: Infallible| match never {}).boxed();
    Response::builder()
        .status(200)
        .header("content-type", "text/event-stream")
        .header("cache-control", "no-cache")
        .header("access-control-allow-origin", "*")
        .body(body)
        .unwrap()
}

// ── Intercept handlers ────────────────────────────────────────────────────────

fn serve_intercept_status(state: &AppState) -> Response<Full<Bytes>> {
    let enabled = intercept::should_intercept(state);
    json_response(serde_json::json!({ "enabled": enabled }))
}

fn serve_intercept_toggle(state: &AppState) -> Response<Full<Bytes>> {
    let new_state = intercept::toggle(state);
    json_response(serde_json::json!({ "enabled": new_state }))
}

fn serve_intercept_pending(state: &AppState) -> Response<Full<Bytes>> {
    // Need Arc for list_pending; we wrap in a temporary Arc reference
    // Actually, list_pending takes &Arc<AppState>, but we only have &AppState here.
    // We'll access the mutex directly.
    let map = state.intercepted.lock().unwrap();
    let ids: Vec<String> = map.keys().cloned().collect();
    json_response(serde_json::json!(ids))
}

fn serve_intercept_forward(state: &AppState, id: &str) -> Response<Full<Bytes>> {
    let mut map = state.intercepted.lock().unwrap();
    if let Some(sender) = map.remove(id) {
        let _ = sender.send(InterceptAction::ForwardOriginal);
        json_response(serde_json::json!({ "ok": true }))
    } else {
        Response::builder()
            .status(StatusCode::NOT_FOUND)
            .header("content-type", "application/json")
            .header("access-control-allow-origin", "*")
            .body(Full::new(Bytes::from(r#"{"error":"request not found or already resolved"}"#)))
            .unwrap()
    }
}

fn serve_intercept_forward_modified(state: &AppState, id: &str, body: &[u8]) -> Response<Full<Bytes>> {
    let body_str = String::from_utf8_lossy(body).to_string();
    let mut map = state.intercepted.lock().unwrap();
    if let Some(sender) = map.remove(id) {
        let _ = sender.send(InterceptAction::ForwardModified { body: body_str });
        json_response(serde_json::json!({ "ok": true }))
    } else {
        Response::builder()
            .status(StatusCode::NOT_FOUND)
            .header("content-type", "application/json")
            .header("access-control-allow-origin", "*")
            .body(Full::new(Bytes::from(r#"{"error":"request not found or already resolved"}"#)))
            .unwrap()
    }
}

fn serve_intercept_reject(state: &AppState, id: &str) -> Response<Full<Bytes>> {
    let mut map = state.intercepted.lock().unwrap();
    if let Some(sender) = map.remove(id) {
        let _ = sender.send(InterceptAction::Reject);
        json_response(serde_json::json!({ "ok": true }))
    } else {
        Response::builder()
            .status(StatusCode::NOT_FOUND)
            .header("content-type", "application/json")
            .header("access-control-allow-origin", "*")
            .body(Full::new(Bytes::from(r#"{"error":"request not found or already resolved"}"#)))
            .unwrap()
    }
}

// ── Routing handlers ──────────────────────────────────────────────────────────

async fn serve_routing_config_get(state: &AppState) -> Response<Full<Bytes>> {
    let config = state.routing_config.read().await;
    json_response(serde_json::to_value(&*config).unwrap_or_default())
}

async fn serve_routing_config_save(state: &AppState, body: &[u8]) -> Response<Full<Bytes>> {
    let config: RoutingConfig = match serde_json::from_slice(body) {
        Ok(c) => c,
        Err(e) => return json_error(&e.to_string()),
    };
    {
        let db = state.db.lock().await;
        if let Err(e) = db::save_routing_config(&db, &config) {
            return json_error(&e.to_string());
        }
    }
    *state.routing_config.write().await = config.clone();
    json_response(serde_json::to_value(&config).unwrap_or_default())
}

async fn serve_routing_rules_list(state: &AppState) -> Response<Full<Bytes>> {
    let rules = state.routing_rules.read().await;
    json_response(serde_json::to_value(&*rules).unwrap_or_default())
}

async fn serve_routing_rules_create(state: &AppState, body: &[u8]) -> Response<Full<Bytes>> {
    let mut rule: RoutingRule = match serde_json::from_slice(body) {
        Ok(r) => r,
        Err(e) => return json_error(&e.to_string()),
    };
    // Generate a new UUID for the id
    rule.id = Uuid::new_v4().to_string();
    // Set priority based on existing rules count
    let priority = {
        let rules = state.routing_rules.read().await;
        (rules.len() + 1) as i64 * 10
    };
    rule.priority = priority;

    {
        let db = state.db.lock().await;
        if let Err(e) = db::insert_routing_rule(&db, &rule) {
            return json_error(&e.to_string());
        }
    }
    let rule_val = serde_json::to_value(&rule).unwrap_or_default();
    state.routing_rules.write().await.push(rule);
    json_response(rule_val)
}

async fn serve_routing_rules_update(state: &AppState, id: &str, body: &[u8]) -> Response<Full<Bytes>> {
    let mut rule: RoutingRule = match serde_json::from_slice(body) {
        Ok(r) => r,
        Err(e) => return json_error(&e.to_string()),
    };
    rule.id = id.to_string();

    {
        let db = state.db.lock().await;
        if let Err(e) = db::update_routing_rule(&db, &rule) {
            return json_error(&e.to_string());
        }
    }
    {
        let mut rules = state.routing_rules.write().await;
        if let Some(existing) = rules.iter_mut().find(|r| r.id == id) {
            *existing = rule.clone();
        }
    }
    json_response(serde_json::to_value(&rule).unwrap_or_default())
}

async fn serve_routing_rules_delete(state: &AppState, id: &str) -> Response<Full<Bytes>> {
    {
        let db = state.db.lock().await;
        if let Err(e) = db::delete_routing_rule(&db, id) {
            return json_error(&e.to_string());
        }
    }
    state.routing_rules.write().await.retain(|r| r.id != id);
    json_response(serde_json::json!({"ok": true}))
}

async fn serve_routing_reorder(state: &AppState, body: &[u8]) -> Response<Full<Bytes>> {
    let parsed: serde_json::Value = match serde_json::from_slice(body) {
        Ok(v) => v,
        Err(e) => return json_error(&e.to_string()),
    };
    let ids: Vec<String> = match parsed.get("ids").and_then(|v| v.as_array()) {
        Some(arr) => arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect(),
        None => return json_error("ids field required"),
    };
    {
        let db = state.db.lock().await;
        if let Err(e) = db::reorder_routing_rules(&db, &ids) {
            return json_error(&e.to_string());
        }
        // Reload rules from DB
        match db::get_routing_rules(&db) {
            Ok(rules) => {
                *state.routing_rules.write().await = rules;
            }
            Err(e) => return json_error(&e.to_string()),
        }
    }
    let rules = state.routing_rules.read().await;
    json_response(serde_json::to_value(&*rules).unwrap_or_default())
}

async fn serve_routing_test(state: &AppState, body: &[u8]) -> Response<Full<Bytes>> {
    let parsed: serde_json::Value = match serde_json::from_slice(body) {
        Ok(v) => v,
        Err(e) => return json_error(&e.to_string()),
    };
    let prompt = parsed.get("prompt").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let system = parsed.get("system").and_then(|v| v.as_str()).unwrap_or("").to_string();

    let config = state.routing_config.read().await.clone();
    if !config.enabled && config.classifier_api_key.is_empty() {
        return json_error("routing not enabled or no classifier key configured");
    }

    // Build messages from prompt
    let mut messages = Vec::new();
    if !system.is_empty() {
        messages.push(serde_json::json!({"role": "user", "content": system}));
    }
    if !prompt.is_empty() {
        messages.push(serde_json::json!({"role": "user", "content": prompt}));
    }
    if messages.is_empty() {
        messages.push(serde_json::json!({"role": "user", "content": "test"}));
    }
    let request_body = serde_json::json!({"messages": messages});

    let rules = state.routing_rules.read().await;
    let category = routing::classify_intent(&config, "", &request_body, &rules).await;
    json_response(serde_json::json!({"category": category}))
}

// ── Supervisor API handlers ──────────────────────────────────────────────────

fn get_session_counts(stats: &[serde_json::Value], session_id: &str) -> (i64, i64) {
    for s in stats {
        if s.get("id").and_then(|v| v.as_str()) == Some(session_id) {
            let rc = s.get("request_count").and_then(|v| v.as_i64()).unwrap_or(0);
            let pc = s.get("pending_count").and_then(|v| v.as_i64()).unwrap_or(0);
            return (rc, pc);
        }
    }
    (0, 0)
}

async fn serve_supervisor_summary(state: &AppState, session_id: &str) -> Response<Full<Bytes>> {
    let db = state.db.lock().await;
    let stats = db::get_session_stats(&db).unwrap_or_default();
    let (req_count, pending_count) = get_session_counts(&stats, session_id);

    if let Ok(Some(cached)) = db::get_supervisor_cache(&db, session_id, "get_session_summary", req_count, pending_count) {
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(&cached) {
            return json_response(val);
        }
    }

    match db::get_requests(&db, Some(session_id), None, 10000, 0) {
        Ok(reqs) => {
            let summary = supervisor::build_session_summary(&reqs);
            let text = serde_json::to_string(&summary).unwrap_or_default();
            let _ = db::set_supervisor_cache(&db, session_id, "get_session_summary", req_count, pending_count, &text);
            json_response(summary)
        }
        Err(e) => json_error(&e.to_string()),
    }
}

async fn serve_supervisor_coverage(state: &AppState, session_id: &str) -> Response<Full<Bytes>> {
    let db = state.db.lock().await;
    let stats = db::get_session_stats(&db).unwrap_or_default();
    let (req_count, pending_count) = get_session_counts(&stats, session_id);

    if let Ok(Some(cached)) = db::get_supervisor_cache(&db, session_id, "get_file_coverage", req_count, pending_count) {
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(&cached) {
            return json_response(val);
        }
    }

    match db::get_file_access_by_session(&db, session_id) {
        Ok(accesses) => {
            let mut file_map: std::collections::HashMap<String, serde_json::Value> = std::collections::HashMap::new();
            for fa in &accesses {
                let entry = file_map.entry(fa.file_path.clone()).or_insert_with(|| serde_json::json!({
                    "file_path": fa.file_path,
                    "access_types": [],
                    "access_count": 0,
                    "has_full_read": false,
                    "read_ranges": [],
                    "first_accessed": fa.timestamp.clone(),
                    "last_accessed": fa.timestamp.clone(),
                }));
                entry["access_count"] = serde_json::json!(entry["access_count"].as_i64().unwrap_or(0) + 1);
                entry["last_accessed"] = serde_json::json!(fa.timestamp);
                if let Some(types) = entry["access_types"].as_array_mut() {
                    let atype = serde_json::json!(fa.access_type);
                    if !types.contains(&atype) {
                        types.push(atype);
                    }
                }
                if fa.access_type == "read" {
                    // "full" or empty = no offset/limit specified (default: reads up to 2000 lines)
                    let is_default_read = fa.read_range == "full" || fa.read_range.is_empty();
                    if is_default_read {
                        // Mark as "default" — actual full/partial determined later using total_lines
                        if let Some(ranges) = entry["read_ranges"].as_array_mut() {
                            let r = serde_json::json!("default");
                            if !ranges.contains(&r) {
                                ranges.push(r);
                            }
                        }
                    } else if !fa.read_range.is_empty() {
                        if let Some(ranges) = entry["read_ranges"].as_array_mut() {
                            let r = serde_json::json!(fa.read_range);
                            if !ranges.contains(&r) {
                                ranges.push(r);
                            }
                        }
                    }
                }
            }
            // Enrich with exact line-level coverage using a bitset approach
            const DEFAULT_READ_LIMIT: usize = 2000;
            let mut files: Vec<serde_json::Value> = file_map.into_values().collect();
            for f in &mut files {
                let path = f["file_path"].as_str().unwrap_or("");
                let total_lines = std::fs::read_to_string(path)
                    .map(|c| c.lines().count())
                    .unwrap_or(0);
                f["total_lines"] = serde_json::json!(total_lines as i64);

                if total_lines == 0 {
                    f["has_full_read"] = serde_json::json!(false);
                    f["lines_read"] = serde_json::json!(0);
                    continue;
                }

                // Build a covered-lines bitset from all read ranges
                let mut covered = vec![false; total_lines];

                if let Some(ranges) = f["read_ranges"].as_array() {
                    for r in ranges {
                        if let Some(s) = r.as_str() {
                            let (start, end) = if s == "default" {
                                // Default read: lines 1..min(total, 2000)
                                (0usize, total_lines.min(DEFAULT_READ_LIMIT))
                            } else {
                                // Parse "offset:N,limit:M"
                                let mut offset_val: usize = 0;
                                let mut limit_val: usize = total_lines;
                                for part in s.split(',') {
                                    if let Some(v) = part.strip_prefix("offset:") {
                                        offset_val = v.parse().unwrap_or(0);
                                    }
                                    if let Some(v) = part.strip_prefix("limit:") {
                                        limit_val = v.parse().unwrap_or(total_lines);
                                    }
                                }
                                (offset_val, (offset_val + limit_val).min(total_lines))
                            };
                            for i in start..end {
                                covered[i] = true;
                            }
                        }
                    }
                }

                let lines_read = covered.iter().filter(|&&c| c).count();
                let is_full = lines_read == total_lines;

                f["has_full_read"] = serde_json::json!(is_full);
                f["lines_read"] = serde_json::json!(lines_read as i64);
            }
            let result = serde_json::json!({
                "file_count": files.len(),
                "total_accesses": accesses.len(),
                "files": files,
            });
            let text = serde_json::to_string(&result).unwrap_or_default();
            let _ = db::set_supervisor_cache(&db, session_id, "get_file_coverage", req_count, pending_count, &text);
            json_response(result)
        }
        Err(e) => json_error(&e.to_string()),
    }
}

async fn serve_supervisor_patterns(state: &AppState, session_id: &str) -> Response<Full<Bytes>> {
    let db = state.db.lock().await;
    let stats = db::get_session_stats(&db).unwrap_or_default();
    let (req_count, pending_count) = get_session_counts(&stats, session_id);

    if let Ok(Some(cached)) = db::get_supervisor_cache(&db, session_id, "detect_patterns", req_count, pending_count) {
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(&cached) {
            return json_response(val);
        }
    }

    let reqs = db::get_requests(&db, Some(session_id), None, 10000, 0).unwrap_or_default();
    let accesses = db::get_file_access_by_session(&db, session_id).unwrap_or_default();
    let patterns = supervisor::detect_patterns(&reqs, &accesses);
    let result = supervisor::patterns_to_json(&patterns);
    let text = serde_json::to_string(&result).unwrap_or_default();
    let _ = db::set_supervisor_cache(&db, session_id, "detect_patterns", req_count, pending_count, &text);
    json_response(result)
}

// ── Supervisor LLM API ────────────────────────────────────────────────────────

async fn serve_supervisor_config_get(state: &AppState) -> Response<Full<Bytes>> {
    let db = state.db.lock().await;
    let config = db::get_supervisor_config(&db).unwrap_or_default();
    // Never return api_key plaintext — expose only a boolean presence flag
    let safe = serde_json::json!({
        "enabled": config.enabled,
        "provider": config.provider,
        "base_url": config.base_url,
        "api_key_set": !config.api_key.is_empty(),
        "model": config.model,
        "interval_minutes": config.interval_minutes,
        "discord_webhook_url": config.discord_webhook_url,
    });
    json_response(safe)
}

async fn serve_supervisor_config_save(state: Arc<AppState>, body: &[u8]) -> Response<Full<Bytes>> {
    let val: serde_json::Value = match serde_json::from_slice(body) {
        Ok(v) => v,
        Err(_) => return json_error("Invalid JSON"),
    };

    let mut config = {
        let db = state.db.lock().await;
        db::get_supervisor_config(&db).unwrap_or_default()
    };

    if let Some(v) = val.get("enabled").and_then(|v| v.as_bool()) { config.enabled = v; }
    if let Some(v) = val.get("provider").and_then(|v| v.as_str()) { config.provider = v.to_string(); }
    if let Some(v) = val.get("base_url").and_then(|v| v.as_str()) { config.base_url = v.to_string(); }
    if let Some(v) = val.get("api_key").and_then(|v| v.as_str()) { config.api_key = v.to_string(); }
    if let Some(v) = val.get("model").and_then(|v| v.as_str()) { config.model = v.to_string(); }
    if let Some(v) = val.get("interval_minutes").and_then(|v| v.as_i64()) { config.interval_minutes = v; }
    if let Some(v) = val.get("discord_webhook_url").and_then(|v| v.as_str()) {
        if !v.is_empty() && !v.starts_with("https://discord.com/api/webhooks/") {
            return json_error("discord_webhook_url must start with https://discord.com/api/webhooks/");
        }
        config.discord_webhook_url = v.to_string();
    }

    {
        let db = state.db.lock().await;
        if let Err(e) = db::save_supervisor_config(&db, &config) {
            return json_error(&e.to_string());
        }
    }

    // Abort existing supervisor task and restart if config is active
    {
        let mut handle = state.supervisor_handle.lock().await;
        if let Some(h) = handle.take() {
            h.abort();
        }
        let is_active = config.enabled && !config.api_key.is_empty();
        if is_active {
            let state_clone = Arc::clone(&state);
            *handle = Some(tokio::spawn(async move {
                supervisor_llm::run_supervisor_loop(state_clone).await;
            }));
            tracing::info!("Supervisor LLM task started via config save");
        }
    }

    json_response(serde_json::json!({"ok": true}))
}

async fn serve_supervisor_goal_get(state: &AppState, session_id: &str) -> Response<Full<Bytes>> {
    let db = state.db.lock().await;
    match db::get_session_goal(&db, session_id) {
        Ok(Some(goal)) => json_response(serde_json::to_value(&goal).unwrap_or_default()),
        Ok(None) => Response::builder()
            .status(404)
            .header("content-type", "application/json")
            .body(Full::new(Bytes::from(r#"{"error":"Not found"}"#)))
            .unwrap(),
        Err(e) => json_error(&e.to_string()),
    }
}

async fn serve_supervisor_goal_set(state: &AppState, session_id: &str, body: &[u8]) -> Response<Full<Bytes>> {
    let val: serde_json::Value = match serde_json::from_slice(body) {
        Ok(v) => v,
        Err(_) => return json_error("Invalid JSON"),
    };
    let goal = match val.get("goal").and_then(|v| v.as_str()) {
        Some(g) => g.chars().take(500).collect::<String>(),
        None => return json_error("Missing 'goal' field"),
    };
    let refined_goal = val.get("refined_goal").and_then(|v| v.as_str())
        .map(|s| s.chars().take(500).collect::<String>());

    let db = state.db.lock().await;
    match db::set_session_goal(&db, session_id, &goal, refined_goal.as_deref()) {
        Ok(()) => json_response(serde_json::json!({"ok": true})),
        Err(e) => json_error(&e.to_string()),
    }
}

async fn serve_supervisor_goal_delete(state: &AppState, session_id: &str) -> Response<Full<Bytes>> {
    let db = state.db.lock().await;
    match db::delete_session_goal(&db, session_id) {
        Ok(()) => json_response(serde_json::json!({"ok": true})),
        Err(e) => json_error(&e.to_string()),
    }
}

async fn serve_supervisor_refine_goal(state: &AppState, body: &[u8]) -> Response<Full<Bytes>> {
    let val: serde_json::Value = match serde_json::from_slice(body) {
        Ok(v) => v,
        Err(_) => return json_error("Invalid JSON"),
    };
    let goal_text = match val.get("goal").and_then(|v| v.as_str()) {
        Some(g) => g.chars().take(500).collect::<String>(),
        None => return json_error("Missing 'goal' field"),
    };

    let config = {
        let db = state.db.lock().await;
        db::get_supervisor_config(&db).unwrap_or_default()
    };

    if config.api_key.is_empty() {
        return json_error("Supervisor not configured. Set API key in Supervisor settings.");
    }

    let llm_config = crate::llm::LlmConfig {
        provider: config.provider,
        base_url: config.base_url,
        api_key: config.api_key,
        model: config.model,
    };

    match supervisor_llm::refine_goal(&llm_config, &goal_text).await {
        Ok(refinement) => json_response(serde_json::to_value(&refinement).unwrap_or_default()),
        Err(e) => json_error(&format!("Goal refinement failed: {e}")),
    }
}

async fn serve_supervisor_analyses(state: &AppState, session_id: &str, query: &str) -> Response<Full<Bytes>> {
    let limit = query.split('&')
        .find(|p| p.starts_with("limit="))
        .and_then(|p| p.trim_start_matches("limit=").parse::<i64>().ok())
        .unwrap_or(10)
        .clamp(1, 100);

    let db = state.db.lock().await;
    match db::get_supervisor_analyses(&db, session_id, limit) {
        Ok(analyses) => json_response(serde_json::to_value(&analyses).unwrap_or_default()),
        Err(e) => json_error(&e.to_string()),
    }
}

// ── Summarizer API handlers ───────────────────────────────────────────────────

async fn serve_summarizer_config_get(state: &AppState) -> Response<Full<Bytes>> {
    let db = state.db.lock().await;
    match db::get_summarizer_config(&db) {
        Ok(config) => json_response(serde_json::json!({
            "provider": config.provider,
            "base_url": config.base_url,
            "api_key": config.api_key,
            "model": config.model,
            "language": config.language,
            "configured": !config.api_key.is_empty(),
        })),
        Err(e) => json_error(&e.to_string()),
    }
}

async fn serve_summarizer_config_save(state: &AppState, body: &[u8]) -> Response<Full<Bytes>> {
    let val: serde_json::Value = match serde_json::from_slice(body) {
        Ok(v) => v,
        Err(_) => return json_error("Invalid JSON"),
    };
    let db = state.db.lock().await;
    let mut config = db::get_summarizer_config(&db).unwrap_or_else(|_| db::SummarizerConfig {
        provider: "anthropic".to_string(),
        base_url: "https://api.anthropic.com".to_string(),
        api_key: String::new(),
        model: "claude-haiku-4-5-20251001".to_string(),
        language: "English".to_string(),
    });
    if let Some(p) = val.get("provider").and_then(|v| v.as_str()) { config.provider = p.to_string(); }
    if let Some(u) = val.get("base_url").and_then(|v| v.as_str()) { config.base_url = u.to_string(); }
    if let Some(k) = val.get("api_key").and_then(|v| v.as_str()) { config.api_key = k.to_string(); }
    if let Some(m) = val.get("model").and_then(|v| v.as_str()) { config.model = m.to_string(); }
    if let Some(l) = val.get("language").and_then(|v| v.as_str()) { config.language = l.to_string(); }
    match db::save_summarizer_config(&db, &config) {
        Ok(_) => json_response(serde_json::json!({"ok": true})),
        Err(e) => json_error(&e.to_string()),
    }
}

async fn serve_summarize(state: &AppState, body: &[u8]) -> Response<Full<Bytes>> {
    let val: serde_json::Value = match serde_json::from_slice(body) {
        Ok(v) => v,
        Err(_) => return json_error("Invalid JSON"),
    };

    let config = {
        let db = state.db.lock().await;
        db::get_summarizer_config(&db).unwrap_or_else(|_| db::SummarizerConfig {
            provider: String::new(),
            base_url: String::new(),
            api_key: String::new(),
            model: String::new(),
            language: "English".to_string(),
        })
    };

    if config.api_key.is_empty() {
        return json_error("Summarizer not configured. Set API key in ⚙ Settings.");
    }

    let line = val.get("line").and_then(|v| v.as_i64()).unwrap_or(0);
    let requests = val.get("requests").and_then(|v| v.as_array()).cloned().unwrap_or_default();

    // Build compact prompt from request data
    let mut context = format!("Summarize what happened to line {} across these {} LLM requests. Focus on: what was the goal, what tools were used (with full file paths), what was the outcome. Respond in {}.\n\n", line, requests.len(), config.language);
    for (i, r) in requests.iter().enumerate() {
        let agent = r.get("agent_type").and_then(|v| v.as_str()).unwrap_or("?");
        let access = r.get("access_type").and_then(|v| v.as_str()).unwrap_or("?");
        let range = r.get("read_range").and_then(|v| v.as_str()).unwrap_or("");
        let ts = r.get("timestamp").and_then(|v| v.as_str()).unwrap_or("");
        let req_id = r.get("request_id").and_then(|v| v.as_str()).unwrap_or("?");

        let req_body = r.get("request_body").and_then(|v| v.as_str()).unwrap_or("");
        let resp_body = r.get("response_body").and_then(|v| v.as_str()).unwrap_or("");

        // Extract file paths from response body (accurate per-request attribution)
        let file_paths = extract_tool_file_paths(resp_body);

        // Extract last user message as the prompt (skip tool_result-only messages)
        let prompt_text = extract_last_user_text(req_body);

        // Extract response summary
        let resp_text = extract_response_text(resp_body);

        context.push_str(&format!("--- Request {} (id:{} {} {} {} {}) ---\nFiles accessed: {}\nPrompt: {}\nResponse: {}\n\n",
            i + 1, &req_id[..req_id.len().min(8)], ts, agent, access, range,
            if file_paths.is_empty() { "none".to_string() } else { file_paths.join(", ") },
            prompt_text,
            resp_text,
        ));
    }

    let llm_config = crate::llm::LlmConfig {
        provider: config.provider.clone(),
        base_url: config.base_url.clone(),
        api_key: config.api_key.clone(),
        model: config.model.clone(),
    };

    match crate::llm::call_llm(&llm_config, None, &context, 1024).await {
        Ok(summary) => json_response(serde_json::json!({"summary": summary})),
        Err(e) => json_error(&e),
    }
}

async fn serve_summarizer_models(state: &AppState, body: &[u8]) -> Response<Full<Bytes>> {
    let val: serde_json::Value = serde_json::from_slice(body).unwrap_or_default();
    let provider = val.get("provider").and_then(|v| v.as_str()).unwrap_or("anthropic");
    let base_url = val.get("base_url").and_then(|v| v.as_str()).unwrap_or("");
    let api_key = val.get("api_key").and_then(|v| v.as_str()).unwrap_or("");

    let actual_key = if api_key.is_empty() {
        let db = state.db.lock().await;
        db::get_summarizer_config(&db).map(|c| c.api_key).unwrap_or_default()
    } else {
        api_key.to_string()
    };

    if actual_key.is_empty() {
        return json_error("API key required to fetch models");
    }

    let cache_id = format!("{}:{}", provider, base_url);

    // Check 1-day cache
    {
        let db = state.db.lock().await;
        let cached: Option<(String, String)> = db.query_row(
            "SELECT models, cached_at FROM model_cache WHERE id = ?1",
            rusqlite::params![cache_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        ).ok();
        if let Some((models_json, cached_at)) = cached {
            if let Ok(cached_time) = chrono::DateTime::parse_from_rfc3339(&cached_at) {
                let age = chrono::Utc::now().signed_duration_since(cached_time);
                if age.num_hours() < 24 {
                    if let Ok(models) = serde_json::from_str::<Vec<String>>(&models_json) {
                        return json_response(serde_json::json!({ "models": models, "cached": true }));
                    }
                }
            }
        }
    }

    // Fetch from provider
    let client = reqwest::Client::new();
    let url = format!("{}/v1/models", base_url);

    let mut req = client.get(&url).header("content-type", "application/json");
    if provider == "anthropic" {
        req = req.header("x-api-key", &actual_key).header("anthropic-version", "2023-06-01");
    } else {
        req = req.header("authorization", format!("Bearer {}", actual_key));
    }

    let resp: reqwest::Response = match req.send().await {
        Ok(r) => r,
        Err(e) => return json_error(&format!("Failed to fetch models: {e}")),
    };

    let text = match resp.text().await {
        Ok(t) => t,
        Err(e) => return json_error(&format!("Failed to read response: {e}")),
    };

    let json: serde_json::Value = match serde_json::from_str(&text) {
        Ok(j) => j,
        Err(_) => return json_error("Invalid response from provider"),
    };

    let mut models: Vec<String> = json.get("data")
        .and_then(|d| d.as_array())
        .map(|arr| arr.iter()
            .filter_map(|m| m.get("id").and_then(|v| v.as_str()).map(|s| s.to_string()))
            .collect())
        .unwrap_or_default();
    models.sort();

    // Cache for 1 day
    if !models.is_empty() {
        let db = state.db.lock().await;
        let now = chrono::Utc::now().to_rfc3339();
        let models_json = serde_json::to_string(&models).unwrap_or_default();
        let _ = db.execute(
            "INSERT OR REPLACE INTO model_cache (id, models, cached_at) VALUES (?1, ?2, ?3)",
            rusqlite::params![cache_id, models_json, now],
        );
    }

    json_response(serde_json::json!({ "models": models, "cached": false }))
}

fn extract_tool_file_paths(response_body: &str) -> Vec<String> {
    // Use response-based extractors to avoid cumulative messages pollution
    let accesses = if let Ok(body) = serde_json::from_str::<serde_json::Value>(response_body) {
        if let Some(raw_sse) = body.get("raw_sse").and_then(|v| v.as_str()) {
            supervisor::extract_file_accesses_from_sse(raw_sse.as_bytes())
        } else {
            supervisor::extract_file_accesses_from_response(response_body)
        }
    } else {
        Vec::new()
    };
    let mut paths: Vec<String> = Vec::new();
    for (path, _, _) in accesses {
        if !paths.contains(&path) { paths.push(path); }
    }
    paths
}

fn extract_last_user_text(request_body: &str) -> String {
    if let Ok(body) = serde_json::from_str::<serde_json::Value>(request_body) {
        if let Some(msgs) = body.get("messages").and_then(|m| m.as_array()) {
            for m in msgs.iter().rev() {
                if m.get("role").and_then(|r| r.as_str()) != Some("user") { continue; }
                if let Some(s) = m.get("content").and_then(|c| c.as_str()) {
                    if !s.starts_with("<system-reminder>") { return s.to_string(); }
                }
                if let Some(arr) = m.get("content").and_then(|c| c.as_array()) {
                    // Skip messages that only contain tool_result blocks (no human text)
                    let all_tool_results = arr.iter().all(|b| {
                        b.get("type").and_then(|t| t.as_str()) == Some("tool_result")
                    });
                    if all_tool_results { continue; }
                    for b in arr.iter().rev() {
                        if b.get("type").and_then(|t| t.as_str()) == Some("tool_result") { continue; }
                        if let Some(t) = b.get("text").and_then(|t| t.as_str()) {
                            if !t.starts_with("<system-reminder>") { return t.to_string(); }
                        }
                    }
                }
            }
        }
    }
    String::new()
}

fn extract_response_text(response_body: &str) -> String {
    if let Ok(body) = serde_json::from_str::<serde_json::Value>(response_body) {
        if let Some(acc) = body.get("accumulated_content").and_then(|v| v.as_str()) {
            return acc.to_string();
        }
        if let Some(content) = body.get("content").and_then(|c| c.as_array()) {
            return content.iter()
                .filter_map(|b| b.get("text").and_then(|t| t.as_str()))
                .collect::<Vec<_>>()
                .join("\n");
        }
    }
    String::new()
}

#[allow(dead_code)]
fn truncate(s: &str, max: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= max { s.to_string() } else { format!("{}…", chars[..max].iter().collect::<String>()) }
}

// ── Files API handlers (Code Viewer) ─────────────────────────────────────────

/// Get the CWD for a session, returning an error response if not found.
async fn get_session_cwd(state: &AppState, session_id: &str) -> Result<String, Response<Full<Bytes>>> {
    let db = state.db.lock().await;
    let mut stmt = db.prepare("SELECT cwd FROM sessions WHERE id = ?1")
        .map_err(|e| json_error(&e.to_string()))?;
    let cwd: Option<String> = stmt.query_row(rusqlite::params![session_id], |row| row.get(0)).ok();
    match cwd {
        Some(c) if !c.is_empty() => Ok(c),
        _ => Err(json_error("Session not found or has no CWD")),
    }
}

/// Validate that a path is within the CWD (prevent path traversal).
fn validate_path(cwd: &str, requested_path: &str) -> Result<std::path::PathBuf, Response<Full<Bytes>>> {
    let cwd_canon = std::fs::canonicalize(cwd).map_err(|_| json_error("CWD not accessible"))?;
    let full_path = if requested_path.starts_with('/') {
        std::path::PathBuf::from(requested_path)
    } else {
        cwd_canon.join(requested_path)
    };
    let canon = std::fs::canonicalize(&full_path).map_err(|_| json_error("File not found"))?;
    if !canon.starts_with(&cwd_canon) {
        return Err(json_error("Path outside session CWD"));
    }
    Ok(canon)
}

async fn serve_file_tree(state: &AppState, session_id: &str) -> Response<Full<Bytes>> {
    let cwd = match get_session_cwd(state, session_id).await {
        Ok(c) => c,
        Err(e) => return e,
    };

    fn walk_dir(dir: &std::path::Path, depth: usize) -> serde_json::Value {
        if depth > 8 { return serde_json::json!([]); }
        let mut entries: Vec<serde_json::Value> = Vec::new();
        let Ok(read_dir) = std::fs::read_dir(dir) else { return serde_json::json!([]); };

        let mut items: Vec<_> = read_dir.filter_map(|e| e.ok()).collect();
        items.sort_by(|a, b| {
            let a_dir = a.file_type().map(|t| t.is_dir()).unwrap_or(false);
            let b_dir = b.file_type().map(|t| t.is_dir()).unwrap_or(false);
            b_dir.cmp(&a_dir).then(a.file_name().cmp(&b.file_name()))
        });

        for entry in items {
            let name = entry.file_name().to_string_lossy().to_string();
            // Skip hidden dirs and common large dirs
            if name.starts_with('.') || name == "node_modules" || name == "target"
                || name == "__pycache__" || name == "dist" || name == "build" {
                continue;
            }
            let path = entry.path();
            let is_dir = path.is_dir();
            if is_dir {
                entries.push(serde_json::json!({
                    "name": name,
                    "path": path.to_string_lossy(),
                    "type": "dir",
                    "children": walk_dir(&path, depth + 1),
                }));
            } else {
                // Count lines for text files (skip binary/large files)
                let line_count = std::fs::read_to_string(&path)
                    .map(|c| c.lines().count() as i64)
                    .unwrap_or(-1);
                entries.push(serde_json::json!({
                    "name": name,
                    "path": path.to_string_lossy(),
                    "type": "file",
                    "total_lines": line_count,
                }));
            }
        }
        serde_json::json!(entries)
    }

    let tree = walk_dir(std::path::Path::new(&cwd), 0);
    json_response(tree)
}

async fn serve_file_content(state: &AppState, session_id: &str, query: &str) -> Response<Full<Bytes>> {
    let cwd = match get_session_cwd(state, session_id).await {
        Ok(c) => c,
        Err(e) => return e,
    };

    let params = parse_query(query);
    let path = match params.get("path") {
        Some(p) => url_decode(p),
        None => return json_error("Missing 'path' parameter"),
    };

    let canon = match validate_path(&cwd, &path) {
        Ok(p) => p,
        Err(e) => return e,
    };

    // Read file content
    match std::fs::read_to_string(&canon) {
        Ok(content) => {
            let lines: Vec<&str> = content.lines().collect();
            let path_str = canon.to_string_lossy().to_string();
            let funcs = functions::extract_functions(&path_str, &content);
            let lang = functions::detect_language(&path_str);
            json_response(serde_json::json!({
                "path": path_str,
                "lines": lines,
                "total_lines": lines.len(),
                "language": lang,
                "functions": funcs,
            }))
        }
        Err(_) => json_error("Cannot read file (binary or inaccessible)"),
    }
}

async fn serve_file_requests(state: &AppState, session_id: &str, query: &str) -> Response<Full<Bytes>> {
    let cwd = match get_session_cwd(state, session_id).await {
        Ok(c) => c,
        Err(e) => return e,
    };

    let params = parse_query(query);
    let path = match params.get("path") {
        Some(p) => url_decode(p),
        None => return json_error("Missing 'path' parameter"),
    };

    let _ = match validate_path(&cwd, &path) {
        Ok(p) => p,
        Err(e) => return e,
    };

    let db = state.db.lock().await;

    // Get all file_access records for this file in this session
    let mut stmt = match db.prepare(
        "SELECT fa.request_id, fa.access_type, fa.read_range, fa.timestamp,
                r.agent_type, r.agent_task, r.request_body, r.response_body,
                r.input_tokens, r.output_tokens, r.duration_ms, r.status
         FROM file_access fa
         JOIN requests r ON fa.request_id = r.id
         WHERE fa.session_id = ?1 AND fa.file_path = ?2
         ORDER BY fa.timestamp ASC"
    ) {
        Ok(s) => s,
        Err(e) => return json_error(&e.to_string()),
    };

    let rows: Vec<serde_json::Value> = stmt.query_map(
        rusqlite::params![session_id, path],
        |row| {
            Ok(serde_json::json!({
                "request_id": row.get::<_, String>(0)?,
                "access_type": row.get::<_, String>(1)?,
                "read_range": row.get::<_, String>(2).unwrap_or_default(),
                "timestamp": row.get::<_, String>(3)?,
                "agent_type": row.get::<_, String>(4).unwrap_or_default(),
                "agent_task": row.get::<_, String>(5).unwrap_or_default(),
                "request_body": row.get::<_, String>(6).unwrap_or_default(),
                "response_body": row.get::<_, Option<String>>(7).unwrap_or(None),
                "input_tokens": row.get::<_, Option<i64>>(8).unwrap_or(None),
                "output_tokens": row.get::<_, Option<i64>>(9).unwrap_or(None),
                "duration_ms": row.get::<_, Option<i64>>(10).unwrap_or(None),
                "status": row.get::<_, String>(11).unwrap_or_default(),
            }))
        }
    ).ok().map(|r| r.filter_map(|v| v.ok()).collect()).unwrap_or_default();

    json_response(serde_json::json!({
        "file_path": path,
        "request_count": rows.len(),
        "requests": rows,
    }))
}

fn url_decode(s: &str) -> String {
    let s = s.replace('+', " ");
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '%' {
            let hex: String = chars.by_ref().take(2).collect();
            if let Ok(byte) = u8::from_str_radix(&hex, 16) {
                result.push(byte as char);
            } else {
                result.push('%');
                result.push_str(&hex);
            }
        } else {
            result.push(c);
        }
    }
    result
}

fn parse_query(q: &str) -> std::collections::HashMap<String, String> {
    q.split('&').filter_map(|part| {
        let mut it = part.splitn(2, '=');
        let k = url_decode(it.next()?);
        let v = url_decode(it.next().unwrap_or(""));
        if k.is_empty() { None } else { Some((k, v)) }
    }).collect()
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::types::{DashboardEvent, RequestRecord, RoutingConfig, RoutingRule, SessionRecord};
    use bytes::Bytes;
    use http_body_util::Full;
    use hyper::service::service_fn;
    use hyper::server::conn::http1;
    use hyper_util::rt::TokioIo;
    use rusqlite::Connection;
    use tokio::net::TcpListener;
    use tokio::sync::broadcast;

    fn make_state() -> Arc<AppState> {
        let conn = Connection::open_in_memory().unwrap();
        db::init_db(&conn).unwrap();
        let (tx, _) = broadcast::channel(4);
        AppState::with_upstream(conn, tx, "http://mock".to_string())
    }

    fn seed_session(state: &Arc<AppState>) -> String {
        let id = "s1".to_string();
        let db = state.db.try_lock().unwrap();
        db::upsert_session(&db, &SessionRecord {
            id: id.clone(),
            pid: Some(1),
            cwd: Some("/proj".to_string()),
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
            source: "claude_code".to_string(),
            target_host: "api.anthropic.com".to_string(),
        }).unwrap();
        // Populate the response fields (insert_request only stores base fields)
        db::update_request_complete(&db, req_id, 200, "{}", "{}", Some(5), Some(3), 100, "complete").unwrap();
    }

    #[tokio::test]
    async fn serve_sessions_empty() {
        let state = make_state();
        let resp = serve_sessions(&state).await;
        assert_eq!(resp.status(), 200);
        let bytes = resp_body_to_bytes(serve_sessions(&state).await).await;
        let arr: Vec<serde_json::Value> = serde_json::from_slice(&bytes).unwrap();
        assert!(arr.is_empty());
    }

    #[tokio::test]
    async fn serve_sessions_returns_stats() {
        let state = make_state();
        seed_session(&state);
        let bytes = resp_body_to_bytes(serve_sessions(&state).await).await;
        let arr: Vec<serde_json::Value> = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["project_name"], "proj");
    }

    #[tokio::test]
    async fn serve_requests_no_filter() {
        let state = make_state();
        let sid = seed_session(&state);
        seed_request(&state, "r1", &sid);
        seed_request(&state, "r2", &sid);

        let bytes = resp_body_to_bytes(serve_requests(&state, "").await).await;
        let arr: Vec<serde_json::Value> = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(arr.len(), 2);
    }

    #[tokio::test]
    async fn serve_requests_with_session_filter() {
        let state = make_state();
        let sid = seed_session(&state);
        seed_request(&state, "r1", &sid);
        seed_request(&state, "r2", "other");

        let bytes = resp_body_to_bytes(serve_requests(&state, "session_id=s1").await).await;
        let arr: Vec<serde_json::Value> = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["id"], "r1");
    }

    #[tokio::test]
    async fn serve_requests_with_limit_and_offset() {
        let state = make_state();
        let sid = seed_session(&state);
        for i in 0..5 {
            seed_request(&state, &format!("r{i}"), &sid);
        }
        let bytes = resp_body_to_bytes(serve_requests(&state, "limit=2&offset=2").await).await;
        let arr: Vec<serde_json::Value> = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(arr.len(), 2);
    }

    #[tokio::test]
    async fn serve_request_detail_found() {
        let state = make_state();
        let sid = seed_session(&state);
        seed_request(&state, "r1", &sid);

        let resp = serve_request_detail(&state, "r1").await;
        assert_eq!(resp.status(), 200);
        let bytes = resp_body_to_bytes(resp).await;
        let obj: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(obj["id"], "r1");
        assert_eq!(obj["status"], "complete");
        assert_eq!(obj["input_tokens"], 5);
    }

    #[tokio::test]
    async fn serve_request_detail_not_found() {
        let state = make_state();
        let resp = serve_request_detail(&state, "does-not-exist").await;
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[test]
    fn serve_sse_stream_returns_event_stream_headers() {
        let state = make_state();
        let resp = serve_sse_stream(&state);
        assert_eq!(resp.status(), 200);
        let ct = resp.headers().get("content-type").unwrap().to_str().unwrap();
        assert!(ct.contains("text/event-stream"));
        assert_eq!(resp.headers().get("cache-control").unwrap(), "no-cache");
        assert_eq!(resp.headers().get("access-control-allow-origin").unwrap(), "*");
    }

    #[tokio::test]
    async fn serve_sse_stream_delivers_events() {
        use http_body_util::BodyExt;
        let state = make_state();
        let tx = state.event_tx.clone();
        let resp = serve_sse_stream(&state);
        let mut body = resp.into_body();

        // Send an event after subscribing
        tx.send(DashboardEvent {
            event_type: "request_update".to_string(),
            data: serde_json::json!({"id": "x"}),
        }).unwrap();

        // Read the first frame with a timeout
        let frame = tokio::time::timeout(
            tokio::time::Duration::from_secs(2),
            body.frame(),
        ).await.unwrap().unwrap().unwrap();

        let data = frame.into_data().unwrap();
        let text = String::from_utf8(data.to_vec()).unwrap();
        assert!(text.contains("request_update"));
        assert!(text.contains("event:"));
        assert!(text.contains("data:"));
    }

    #[test]
    fn guess_mime_known_extensions() {
        assert_eq!(guess_mime("file.html"), "text/html; charset=utf-8");
        assert_eq!(guess_mime("bundle.js"),  "application/javascript");
        assert_eq!(guess_mime("style.css"),  "text/css");
        assert_eq!(guess_mime("icon.svg"),   "image/svg+xml");
        assert_eq!(guess_mime("data.json"),  "application/json");
        assert_eq!(guess_mime("font.woff2"), "font/woff2");
        assert_eq!(guess_mime("image.ico"),  "image/x-icon");
    }

    #[test]
    fn guess_mime_unknown_extension_returns_octet_stream() {
        assert_eq!(guess_mime("file.xyz"), "application/octet-stream");
        assert_eq!(guess_mime("noext"),    "application/octet-stream");
    }

    #[test]
    fn serve_static_fallback_returns_html() {
        // When DIST doesn't have the file, falls back to FALLBACK_HTML
        let resp = serve_static("/nonexistent-page");
        assert_eq!(resp.status(), 200);
        let ct = resp.headers().get("content-type").unwrap().to_str().unwrap();
        assert!(ct.contains("text/html"));
    }

    #[test]
    fn parse_query_basic() {
        let map = parse_query("session_id=abc&limit=10&offset=5");
        assert_eq!(map["session_id"], "abc");
        assert_eq!(map["limit"],      "10");
        assert_eq!(map["offset"],     "5");
    }

    #[test]
    fn parse_query_empty_string() {
        let map = parse_query("");
        assert!(map.is_empty());
    }

    #[test]
    fn parse_query_ignores_empty_keys() {
        let map = parse_query("&valid=yes&");
        assert_eq!(map.len(), 1);
        assert_eq!(map["valid"], "yes");
    }

    #[test]
    fn parse_query_decodes_plus_and_percent() {
        let map = parse_query("search=hello+world&foo=bar%20baz");
        assert_eq!(map["search"], "hello world");
        assert_eq!(map["foo"], "bar baz");
    }

    #[test]
    fn url_decode_handles_special_chars() {
        assert_eq!(url_decode("A+specific+feature"), "A specific feature");
        assert_eq!(url_decode("100%25"), "100%");
        assert_eq!(url_decode("no+encoding"), "no encoding");
    }

    #[test]
    fn json_response_has_cors_header() {
        let resp = json_response(serde_json::json!([]));
        assert_eq!(
            resp.headers().get("access-control-allow-origin").unwrap(),
            "*"
        );
        assert_eq!(resp.status(), 200);
    }

    #[test]
    fn json_error_returns_400() {
        let resp = json_error("something went wrong");
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let ct = resp.headers().get("content-type").unwrap().to_str().unwrap();
        assert!(ct.contains("application/json"));
    }

    #[tokio::test]
    async fn serve_delete_session_removes_data() {
        let state = make_state();
        let sid = seed_session(&state);
        seed_request(&state, "r1", &sid);
        seed_request(&state, "r2", &sid);

        let resp = serve_delete_session(&state, &sid).await;
        assert_eq!(resp.status(), 200);
        let bytes = resp_body_to_bytes(resp).await;
        let obj: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(obj["ok"], true);

        // Session and requests should be gone
        let bytes = resp_body_to_bytes(serve_sessions(&state).await).await;
        let arr: Vec<serde_json::Value> = serde_json::from_slice(&bytes).unwrap();
        assert!(arr.is_empty());

        let bytes = resp_body_to_bytes(serve_requests(&state, "").await).await;
        let arr: Vec<serde_json::Value> = serde_json::from_slice(&bytes).unwrap();
        assert!(arr.is_empty());
    }

    #[tokio::test]
    async fn serve_toggle_star_stars_and_unstars() {
        let state = make_state();
        let sid = seed_session(&state);
        seed_request(&state, "r1", &sid);

        // Star it
        let resp = serve_toggle_star(&state, "r1").await;
        let bytes = resp_body_to_bytes(resp).await;
        let obj: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(obj["starred"], true);

        // Unstar it
        let resp = serve_toggle_star(&state, "r1").await;
        let bytes = resp_body_to_bytes(resp).await;
        let obj: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(obj["starred"], false);
    }

    #[tokio::test]
    async fn serve_requests_starred_filter() {
        let state = make_state();
        let sid = seed_session(&state);
        seed_request(&state, "r1", &sid);
        seed_request(&state, "r2", &sid);

        // Star r1
        serve_toggle_star(&state, "r1").await;

        let bytes = resp_body_to_bytes(serve_requests(&state, "starred=1").await).await;
        let arr: Vec<serde_json::Value> = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["id"], "r1");
        assert_eq!(arr[0]["starred"], true);
    }

    #[tokio::test]
    async fn serve_requests_with_search() {
        let state = make_state();
        let sid = seed_session(&state);
        // seed_request inserts with body "{}" — let's insert one with custom body
        {
            let db = state.db.try_lock().unwrap();
            db::insert_request(&db, &RequestRecord {
                id: "r-search".to_string(),
                session_id: Some(sid.clone()),
                timestamp: "2024-01-01T00:00:00Z".to_string(),
                method: "POST".to_string(),
                path: "/v1/messages".to_string(),
                request_headers: "{}".to_string(),
                request_body: r#"{"model":"claude-haiku","messages":[]}"#.to_string(),
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
                source: "claude_code".to_string(),
                target_host: "api.anthropic.com".to_string(),
            }).unwrap();
        }
        seed_request(&state, "r-other", &sid);

        let bytes = resp_body_to_bytes(serve_requests(&state, "search=haiku").await).await;
        let arr: Vec<serde_json::Value> = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["id"], "r-search");
    }

    #[tokio::test]
    async fn serve_requests_search_no_match() {
        let state = make_state();
        let sid = seed_session(&state);
        seed_request(&state, "r1", &sid);

        let bytes = resp_body_to_bytes(serve_requests(&state, "search=nonexistent").await).await;
        let arr: Vec<serde_json::Value> = serde_json::from_slice(&bytes).unwrap();
        assert!(arr.is_empty());
    }

    // ── Intercept tests ────────────────────────────────────────────────────────

    #[tokio::test]
    async fn serve_intercept_status_default_disabled() {
        let state = make_state();
        let bytes = resp_body_to_bytes(serve_intercept_status(&state)).await;
        let obj: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(obj["enabled"], false);
    }

    #[tokio::test]
    async fn serve_intercept_toggle_enables() {
        let state = make_state();
        let bytes = resp_body_to_bytes(serve_intercept_toggle(&state)).await;
        let obj: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(obj["enabled"], true);

        let bytes = resp_body_to_bytes(serve_intercept_toggle(&state)).await;
        let obj: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(obj["enabled"], false);
    }

    #[tokio::test]
    async fn serve_intercept_pending_empty() {
        let state = make_state();
        let bytes = resp_body_to_bytes(serve_intercept_pending(&state)).await;
        let arr: Vec<String> = serde_json::from_slice(&bytes).unwrap();
        assert!(arr.is_empty());
    }

    #[test]
    fn serve_intercept_forward_not_found() {
        let state = make_state();
        let resp = serve_intercept_forward(&state, "nonexistent");
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn serve_intercept_forward_resolves() {
        let state = make_state();
        let rx = crate::intercept::register(&state, "r1");
        let resp = serve_intercept_forward(&state, "r1");
        assert_eq!(resp.status(), 200);
        let action = rx.await.unwrap();
        assert!(matches!(action, crate::types::InterceptAction::ForwardOriginal));
    }

    #[tokio::test]
    async fn serve_intercept_forward_modified_resolves() {
        let state = make_state();
        let rx = crate::intercept::register(&state, "r2");
        let body = br#"{"model":"new"}"#;
        let resp = serve_intercept_forward_modified(&state, "r2", body);
        assert_eq!(resp.status(), 200);
        let action = rx.await.unwrap();
        match action {
            crate::types::InterceptAction::ForwardModified { body: b } => {
                assert_eq!(b, r#"{"model":"new"}"#);
            }
            _ => panic!("expected ForwardModified"),
        }
    }

    #[test]
    fn serve_intercept_forward_modified_not_found() {
        let state = make_state();
        let resp = serve_intercept_forward_modified(&state, "nonexistent", b"{}");
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn serve_intercept_reject_resolves() {
        let state = make_state();
        let rx = crate::intercept::register(&state, "r3");
        let resp = serve_intercept_reject(&state, "r3");
        assert_eq!(resp.status(), 200);
        let action = rx.await.unwrap();
        assert!(matches!(action, crate::types::InterceptAction::Reject));
    }

    #[test]
    fn serve_intercept_reject_not_found() {
        let state = make_state();
        let resp = serve_intercept_reject(&state, "nonexistent");
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn serve_set_memo_saves() {
        let state = make_state();
        let sid = seed_session(&state);
        seed_request(&state, "r-memo", &sid);
        let body = br#"{"memo":"hello world"}"#;
        let resp = serve_set_memo(&state, "r-memo", body).await;
        assert_eq!(resp.status(), 200);
        let bytes = resp_body_to_bytes(resp).await;
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(v["memo"], "hello world");
        // Verify persisted
        let db = state.db.lock().await;
        let req = crate::db::get_request_by_id(&db, "r-memo").unwrap().unwrap();
        assert_eq!(req.memo, "hello world");
    }

    #[tokio::test]
    async fn serve_set_memo_empty_body() {
        let state = make_state();
        let sid = seed_session(&state);
        seed_request(&state, "r-memo2", &sid);
        let resp = serve_set_memo(&state, "r-memo2", b"{}").await;
        assert_eq!(resp.status(), 200);
        let bytes = resp_body_to_bytes(resp).await;
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(v["memo"], "");
    }

    // ── Routing API tests ──────────────────────────────────────────────────────

    #[tokio::test]
    async fn serve_routing_config_get_and_save() {
        let state = make_state();

        // GET default config
        let bytes = resp_body_to_bytes(serve_routing_config_get(&state).await).await;
        let obj: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(obj["enabled"], false);
        assert_eq!(obj["classifier_model"], "claude-haiku-4-5-20251001");

        // POST to save
        let new_config = RoutingConfig {
            enabled: true,
            classifier_base_url: "https://openai.com".to_string(),
            classifier_api_key: "sk-test".to_string(),
            classifier_model: "gpt-4".to_string(),
            classifier_prompt: "custom prompt".to_string(),
        };
        let body = serde_json::to_vec(&new_config).unwrap();
        let resp = serve_routing_config_save(&state, &body).await;
        assert_eq!(resp.status(), 200);

        let bytes = resp_body_to_bytes(resp).await;
        let obj: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(obj["enabled"], true);
        assert_eq!(obj["classifier_model"], "gpt-4");

        // State should be updated
        let config = state.routing_config.read().await;
        assert!(config.enabled);
        assert_eq!(config.classifier_model, "gpt-4");
    }

    #[tokio::test]
    async fn serve_routing_config_save_invalid_json() {
        let state = make_state();
        let resp = serve_routing_config_save(&state, b"not json").await;
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn serve_routing_rules_full_crud() {
        let state = make_state();

        // List empty
        let bytes = resp_body_to_bytes(serve_routing_rules_list(&state).await).await;
        let arr: Vec<serde_json::Value> = serde_json::from_slice(&bytes).unwrap();
        assert!(arr.is_empty());

        // Create
        let rule_body = serde_json::json!({
            "id": "ignored",  // will be replaced by UUID
            "priority": 999,  // will be overridden
            "enabled": true,
            "category": "code_gen",
            "target_url": "https://openai.com",
            "model_override": "gpt-4",
            "label": "test rule",
        });
        let body = serde_json::to_vec(&rule_body).unwrap();
        let resp = serve_routing_rules_create(&state, &body).await;
        assert_eq!(resp.status(), 200);
        let bytes = resp_body_to_bytes(resp).await;
        let created: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let rule_id = created["id"].as_str().unwrap().to_string();
        assert!(!rule_id.is_empty());
        assert_eq!(created["category"], "code_gen");
        // Priority auto-assigned
        assert_eq!(created["priority"], 10);  // 1st rule => (1+1)*10... wait: (0+1)*10=10

        // List should have 1
        let bytes = resp_body_to_bytes(serve_routing_rules_list(&state).await).await;
        let arr: Vec<serde_json::Value> = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(arr.len(), 1);

        // Update
        let updated_rule = RoutingRule {
            id: rule_id.clone(),
            priority: 5,
            enabled: false,
            category: "docs".to_string(),
            description: String::new(),
            target_url: "https://updated.com".to_string(),
            api_key: String::new(),
            prompt_override: String::new(),
            model_override: String::new(),
            label: "updated".to_string(),
        };
        let body = serde_json::to_vec(&updated_rule).unwrap();
        let resp = serve_routing_rules_update(&state, &rule_id, &body).await;
        assert_eq!(resp.status(), 200);
        let bytes = resp_body_to_bytes(resp).await;
        let obj: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(obj["category"], "docs");
        assert_eq!(obj["enabled"], false);

        // Delete
        let resp = serve_routing_rules_delete(&state, &rule_id).await;
        assert_eq!(resp.status(), 200);
        let bytes = resp_body_to_bytes(resp).await;
        let obj: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(obj["ok"], true);

        // List should be empty again
        let rules = state.routing_rules.read().await;
        assert!(rules.is_empty());
    }

    #[tokio::test]
    async fn serve_routing_reorder_test() {
        let state = make_state();

        // Create 3 rules
        for cat in &["code_gen", "docs", "qa"] {
            let rule = RoutingRule {
                id: String::new(),
                priority: 0,
                enabled: true,
                category: cat.to_string(),
                description: String::new(),
                target_url: "https://example.com".to_string(),
                api_key: String::new(),
                prompt_override: String::new(),
                model_override: String::new(),
                label: String::new(),
            };
            let body = serde_json::to_vec(&rule).unwrap();
            serve_routing_rules_create(&state, &body).await;
        }

        // Get IDs in current order
        let rules = state.routing_rules.read().await.clone();
        assert_eq!(rules.len(), 3);
        let ids: Vec<String> = rules.iter().map(|r| r.id.clone()).collect();

        // Reorder: reverse
        let reversed: Vec<String> = ids.iter().rev().cloned().collect();
        let body = serde_json::json!({"ids": reversed});
        let resp = serve_routing_reorder(&state, &serde_json::to_vec(&body).unwrap()).await;
        assert_eq!(resp.status(), 200);

        // Check order in state
        let rules = state.routing_rules.read().await;
        assert_eq!(rules[0].id, ids[2]);
        assert_eq!(rules[0].priority, 1);
        assert_eq!(rules[2].id, ids[0]);
        assert_eq!(rules[2].priority, 3);
    }

    #[tokio::test]
    async fn serve_routing_test_success() {
        // Spawn a mock classifier
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            if let Ok((stream, _)) = listener.accept().await {
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

        let state = make_state();
        // Configure routing with classifier pointing to mock
        {
            let mut config = state.routing_config.write().await;
            config.enabled = true;
            config.classifier_base_url = format!("http://{addr}");
            config.classifier_api_key = "test".to_string();
        }
        // Seed a routing rule so categories_from_rules returns ["code_gen"]
        {
            let mut rules = state.routing_rules.write().await;
            rules.push(crate::types::RoutingRule {
                id: "test-rule-1".to_string(),
                priority: 1,
                enabled: true,
                category: "code_gen".to_string(),
                description: "Writing new code".to_string(),
                target_url: "https://api.anthropic.com".to_string(),
                api_key: String::new(),
                prompt_override: String::new(),
                model_override: String::new(),
                label: "Code Gen".to_string(),
            });
        }

        let body = serde_json::json!({"prompt": "write me some code"});
        let resp = serve_routing_test(&state, &serde_json::to_vec(&body).unwrap()).await;
        assert_eq!(resp.status(), 200);
        let bytes = resp_body_to_bytes(resp).await;
        let obj: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(obj["category"], "code_gen");
    }

    #[tokio::test]
    async fn serve_routing_test_not_enabled_returns_error() {
        let state = make_state();
        // Routing disabled, no key
        let body = serde_json::json!({"prompt": "test"});
        let resp = serve_routing_test(&state, &serde_json::to_vec(&body).unwrap()).await;
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    // ── Helper ────────────────────────────────────────────────────────────────

    async fn resp_body_to_bytes(resp: Response<Full<Bytes>>) -> Vec<u8> {
        use http_body_util::BodyExt;
        resp.into_body().collect().await.unwrap().to_bytes().to_vec()
    }
}

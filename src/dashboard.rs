use std::sync::Arc;
use std::convert::Infallible;

use bytes::Bytes;
use futures::StreamExt;
use http_body_util::{Full, BodyExt, StreamBody};
use hyper::body::Frame;
use hyper::{Request, Response, StatusCode};
use tracing::warn;

use crate::db;
use crate::intercept;
use crate::types::{AppState, InterceptAction};

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

    // Read body for POST requests that need it
    let post_body = if method == "POST" {
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
            let body = post_body.as_deref().unwrap_or(&[]);
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
            let body = post_body.as_deref().unwrap_or(&[]);
            full_to_box(serve_intercept_forward_modified(&state, id, body))
        }
        ("POST", p) if p.starts_with("/api/intercept/") && p.ends_with("/reject") => {
            let id = p.trim_start_matches("/api/intercept/").trim_end_matches("/reject");
            full_to_box(serve_intercept_reject(&state, id))
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
        match db::get_requests(&db, session_id, limit, offset) {
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
    use crate::types::{DashboardEvent, RequestRecord, SessionRecord};
    use rusqlite::Connection;
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
        }).unwrap();
        // Populate the response fields (insert_request only stores base fields)
        db::update_request_complete(&db, req_id, 200, "{}", "{}", Some(5), Some(3), 100, "complete").unwrap();
    }

    // Helper: build a fake hyper request (body unused for API routes)
    fn fake_request(path: &str) -> Request<hyper::body::Incoming> {
        // We can't construct Incoming without a live connection, so we test
        // the inner async functions directly (serve_sessions, etc.) instead.
        // This function exists only as documentation of the approach.
        let _ = path;
        unreachable!("use serve_* helpers directly")
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

    // ── Helper ────────────────────────────────────────────────────────────────

    async fn resp_body_to_bytes(resp: Response<Full<Bytes>>) -> Vec<u8> {
        use http_body_util::BodyExt;
        resp.into_body().collect().await.unwrap().to_bytes().to_vec()
    }
}

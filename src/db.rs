use anyhow::Result;
use rusqlite::{Connection, params};
use crate::types::{RequestRecord, SessionRecord};

pub fn init_db(conn: &Connection) -> Result<()> {
    conn.execute_batch("
        CREATE TABLE IF NOT EXISTS sessions (
            id           TEXT PRIMARY KEY,
            pid          INTEGER,
            cwd          TEXT,
            project_name TEXT,
            started_at   TEXT NOT NULL,
            last_seen_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS requests (
            id               TEXT PRIMARY KEY,
            session_id       TEXT,
            timestamp        TEXT NOT NULL,
            method           TEXT NOT NULL,
            path             TEXT NOT NULL,
            request_headers  TEXT NOT NULL,
            request_body     TEXT NOT NULL,
            response_status  INTEGER,
            response_headers TEXT,
            response_body    TEXT,
            is_streaming     INTEGER NOT NULL DEFAULT 0,
            input_tokens     INTEGER,
            output_tokens    INTEGER,
            duration_ms      INTEGER,
            status           TEXT NOT NULL DEFAULT 'pending'
        );

        CREATE INDEX IF NOT EXISTS idx_requests_timestamp ON requests(timestamp DESC);
        CREATE INDEX IF NOT EXISTS idx_requests_session ON requests(session_id);
    ")?;
    Ok(())
}

pub fn upsert_session(conn: &Connection, session: &SessionRecord) -> Result<()> {
    conn.execute(
        "INSERT INTO sessions (id, pid, cwd, project_name, started_at, last_seen_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(id) DO UPDATE SET last_seen_at = excluded.last_seen_at",
        params![
            session.id,
            session.pid,
            session.cwd,
            session.project_name,
            session.started_at,
            session.last_seen_at,
        ],
    )?;
    Ok(())
}

pub fn insert_request(conn: &Connection, req: &RequestRecord) -> Result<()> {
    conn.execute(
        "INSERT INTO requests (id, session_id, timestamp, method, path, request_headers, request_body, is_streaming, status)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            req.id,
            req.session_id,
            req.timestamp,
            req.method,
            req.path,
            req.request_headers,
            req.request_body,
            req.is_streaming as i32,
            req.status,
        ],
    )?;
    Ok(())
}

pub fn update_request_complete(
    conn: &Connection,
    id: &str,
    response_status: i64,
    response_headers: &str,
    response_body: &str,
    input_tokens: Option<i64>,
    output_tokens: Option<i64>,
    duration_ms: i64,
    status: &str,
) -> Result<()> {
    conn.execute(
        "UPDATE requests SET
            response_status = ?1,
            response_headers = ?2,
            response_body = ?3,
            input_tokens = ?4,
            output_tokens = ?5,
            duration_ms = ?6,
            status = ?7
         WHERE id = ?8",
        params![
            response_status,
            response_headers,
            response_body,
            input_tokens,
            output_tokens,
            duration_ms,
            status,
            id,
        ],
    )?;
    Ok(())
}

/// Returns the most recently seen session ID for a given CWD, if any exists.
/// Used to group subagents running from the same directory under one session.
pub fn find_session_id_by_cwd(conn: &Connection, cwd: &str) -> Result<Option<String>> {
    let mut stmt = conn.prepare(
        "SELECT id FROM sessions WHERE cwd = ?1 ORDER BY last_seen_at DESC LIMIT 1"
    )?;
    let mut rows = stmt.query_map(params![cwd], |row| row.get::<_, String>(0))?;
    Ok(rows.next().transpose()?)
}

pub fn get_sessions(conn: &Connection) -> Result<Vec<SessionRecord>> {
    let mut stmt = conn.prepare(
        "SELECT id, pid, cwd, project_name, started_at, last_seen_at FROM sessions ORDER BY last_seen_at DESC"
    )?;
    let sessions = stmt.query_map([], |row| {
        Ok(SessionRecord {
            id: row.get(0)?,
            pid: row.get(1)?,
            cwd: row.get(2)?,
            project_name: row.get(3)?,
            started_at: row.get(4)?,
            last_seen_at: row.get(5)?,
        })
    })?
    .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(sessions)
}

pub fn get_session_stats(conn: &Connection) -> Result<Vec<serde_json::Value>> {
    let mut stmt = conn.prepare(
        "SELECT s.id, s.pid, s.cwd, s.project_name, s.started_at, s.last_seen_at,
                COUNT(r.id) as request_count,
                SUM(COALESCE(r.input_tokens, 0)) as total_input,
                SUM(COALESCE(r.output_tokens, 0)) as total_output,
                SUM(CASE WHEN r.status = 'pending' THEN 1 ELSE 0 END) as pending_count
         FROM sessions s
         LEFT JOIN requests r ON r.session_id = s.id
         GROUP BY s.id
         ORDER BY s.last_seen_at DESC"
    )?;
    let stats = stmt.query_map([], |row| {
        Ok(serde_json::json!({
            "id": row.get::<_, String>(0)?,
            "pid": row.get::<_, Option<i64>>(1)?,
            "cwd": row.get::<_, Option<String>>(2)?,
            "project_name": row.get::<_, Option<String>>(3)?,
            "started_at": row.get::<_, String>(4)?,
            "last_seen_at": row.get::<_, String>(5)?,
            "request_count": row.get::<_, i64>(6)?,
            "total_input_tokens": row.get::<_, i64>(7)?,
            "total_output_tokens": row.get::<_, i64>(8)?,
            "pending_count": row.get::<_, i64>(9)?,
        }))
    })?
    .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(stats)
}

pub fn get_requests(
    conn: &Connection,
    session_id: Option<&str>,
    limit: i64,
    offset: i64,
) -> Result<Vec<RequestRecord>> {
    let rows = if let Some(sid) = session_id {
        let mut stmt = conn.prepare(
            "SELECT id, session_id, timestamp, method, path, request_headers, request_body,
                    response_status, response_headers, response_body, is_streaming,
                    input_tokens, output_tokens, duration_ms, status
             FROM requests WHERE session_id = ?1
             ORDER BY timestamp DESC LIMIT ?2 OFFSET ?3"
        )?;
        let x = stmt.query_map(params![sid, limit, offset], map_request_row)?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        x
    } else {
        let mut stmt = conn.prepare(
            "SELECT id, session_id, timestamp, method, path, request_headers, request_body,
                    response_status, response_headers, response_body, is_streaming,
                    input_tokens, output_tokens, duration_ms, status
             FROM requests
             ORDER BY timestamp DESC LIMIT ?1 OFFSET ?2"
        )?;
        let x = stmt.query_map(params![limit, offset], map_request_row)?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        x
    };
    Ok(rows)
}

pub fn get_request_by_id(conn: &Connection, id: &str) -> Result<Option<RequestRecord>> {
    let mut stmt = conn.prepare(
        "SELECT id, session_id, timestamp, method, path, request_headers, request_body,
                response_status, response_headers, response_body, is_streaming,
                input_tokens, output_tokens, duration_ms, status
         FROM requests WHERE id = ?1"
    )?;
    let mut rows = stmt.query_map(params![id], map_request_row)?;
    Ok(rows.next().transpose()?)
}

fn map_request_row(row: &rusqlite::Row) -> rusqlite::Result<RequestRecord> {
    Ok(RequestRecord {
        id: row.get(0)?,
        session_id: row.get(1)?,
        timestamp: row.get(2)?,
        method: row.get(3)?,
        path: row.get(4)?,
        request_headers: row.get(5)?,
        request_body: row.get(6)?,
        response_status: row.get(7)?,
        response_headers: row.get(8)?,
        response_body: row.get(9)?,
        is_streaming: row.get::<_, i32>(10)? != 0,
        input_tokens: row.get(11)?,
        output_tokens: row.get(12)?,
        duration_ms: row.get(13)?,
        status: row.get(14)?,
    })
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn setup() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        init_db(&conn).unwrap();
        conn
    }

    fn sample_session(id: &str) -> SessionRecord {
        SessionRecord {
            id: id.to_string(),
            pid: Some(1234),
            cwd: Some("/home/user/project".to_string()),
            project_name: Some("project".to_string()),
            started_at: "2024-01-01T00:00:00Z".to_string(),
            last_seen_at: "2024-01-01T00:00:00Z".to_string(),
        }
    }

    fn sample_request(id: &str, session_id: &str) -> RequestRecord {
        RequestRecord {
            id: id.to_string(),
            session_id: Some(session_id.to_string()),
            timestamp: "2024-01-01T00:00:00Z".to_string(),
            method: "POST".to_string(),
            path: "/v1/messages".to_string(),
            request_headers: r#"{"content-type":"application/json"}"#.to_string(),
            request_body: r#"{"model":"claude","messages":[],"max_tokens":10}"#.to_string(),
            response_status: None,
            response_headers: None,
            response_body: None,
            is_streaming: false,
            input_tokens: None,
            output_tokens: None,
            duration_ms: None,
            status: "pending".to_string(),
        }
    }

    #[test]
    fn init_db_creates_tables() {
        let conn = Connection::open_in_memory().unwrap();
        init_db(&conn).unwrap();
        // Verify sessions table exists
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM sessions", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0);
        // Verify requests table exists
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM requests", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn init_db_is_idempotent() {
        let conn = Connection::open_in_memory().unwrap();
        init_db(&conn).unwrap();
        init_db(&conn).unwrap(); // second call must not fail
    }

    #[test]
    fn upsert_session_insert() {
        let conn = setup();
        let s = sample_session("s1");
        upsert_session(&conn, &s).unwrap();

        let sessions = get_sessions(&conn).unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].id, "s1");
        assert_eq!(sessions[0].pid, Some(1234));
        assert_eq!(sessions[0].project_name.as_deref(), Some("project"));
    }

    #[test]
    fn upsert_session_updates_last_seen() {
        let conn = setup();
        let s = sample_session("s1");
        upsert_session(&conn, &s).unwrap();

        let mut updated = s.clone();
        updated.last_seen_at = "2024-06-01T12:00:00Z".to_string();
        upsert_session(&conn, &updated).unwrap();

        let sessions = get_sessions(&conn).unwrap();
        assert_eq!(sessions.len(), 1); // still one row
        assert_eq!(sessions[0].last_seen_at, "2024-06-01T12:00:00Z");
    }

    #[test]
    fn insert_and_get_request() {
        let conn = setup();
        upsert_session(&conn, &sample_session("s1")).unwrap();

        let req = sample_request("r1", "s1");
        insert_request(&conn, &req).unwrap();

        let found = get_request_by_id(&conn, "r1").unwrap().unwrap();
        assert_eq!(found.id, "r1");
        assert_eq!(found.status, "pending");
        assert!(!found.is_streaming);
        assert_eq!(found.method, "POST");
    }

    #[test]
    fn get_request_by_id_not_found() {
        let conn = setup();
        let result = get_request_by_id(&conn, "does-not-exist").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn update_request_complete() {
        let conn = setup();
        upsert_session(&conn, &sample_session("s1")).unwrap();
        insert_request(&conn, &sample_request("r1", "s1")).unwrap();

        super::update_request_complete(
            &conn, "r1", 200, "{}", r#"{"content":"hello"}"#,
            Some(10), Some(5), 1234, "complete",
        ).unwrap();

        let found = get_request_by_id(&conn, "r1").unwrap().unwrap();
        assert_eq!(found.status, "complete");
        assert_eq!(found.response_status, Some(200));
        assert_eq!(found.input_tokens, Some(10));
        assert_eq!(found.output_tokens, Some(5));
        assert_eq!(found.duration_ms, Some(1234));
    }

    #[test]
    fn get_requests_no_filter() {
        let conn = setup();
        upsert_session(&conn, &sample_session("s1")).unwrap();
        insert_request(&conn, &sample_request("r1", "s1")).unwrap();
        insert_request(&conn, &sample_request("r2", "s1")).unwrap();

        let reqs = get_requests(&conn, None, 10, 0).unwrap();
        assert_eq!(reqs.len(), 2);
    }

    #[test]
    fn get_requests_with_session_filter() {
        let conn = setup();
        upsert_session(&conn, &sample_session("s1")).unwrap();
        upsert_session(&conn, &sample_session("s2")).unwrap();
        insert_request(&conn, &sample_request("r1", "s1")).unwrap();
        insert_request(&conn, &sample_request("r2", "s2")).unwrap();

        let reqs = get_requests(&conn, Some("s1"), 10, 0).unwrap();
        assert_eq!(reqs.len(), 1);
        assert_eq!(reqs[0].id, "r1");
    }

    #[test]
    fn get_requests_pagination() {
        let conn = setup();
        upsert_session(&conn, &sample_session("s1")).unwrap();
        for i in 0..5 {
            insert_request(&conn, &sample_request(&format!("r{i}"), "s1")).unwrap();
        }

        let page1 = get_requests(&conn, None, 2, 0).unwrap();
        let page2 = get_requests(&conn, None, 2, 2).unwrap();
        let page3 = get_requests(&conn, None, 2, 4).unwrap();

        assert_eq!(page1.len(), 2);
        assert_eq!(page2.len(), 2);
        assert_eq!(page3.len(), 1);
    }

    #[test]
    fn get_session_stats_aggregates_tokens() {
        let conn = setup();
        upsert_session(&conn, &sample_session("s1")).unwrap();
        insert_request(&conn, &sample_request("r1", "s1")).unwrap();
        insert_request(&conn, &sample_request("r2", "s1")).unwrap();

        super::update_request_complete(&conn, "r1", 200, "{}", "{}", Some(10), Some(5), 100, "complete").unwrap();
        super::update_request_complete(&conn, "r2", 200, "{}", "{}", Some(20), Some(8), 200, "complete").unwrap();

        let stats = get_session_stats(&conn).unwrap();
        assert_eq!(stats.len(), 1);
        assert_eq!(stats[0]["request_count"], 2);
        assert_eq!(stats[0]["total_input_tokens"], 30);
        assert_eq!(stats[0]["total_output_tokens"], 13);
        assert_eq!(stats[0]["pending_count"], 0);
    }

    #[test]
    fn get_session_stats_counts_pending() {
        let conn = setup();
        upsert_session(&conn, &sample_session("s1")).unwrap();
        insert_request(&conn, &sample_request("r1", "s1")).unwrap(); // pending
        insert_request(&conn, &sample_request("r2", "s1")).unwrap(); // pending

        super::update_request_complete(&conn, "r1", 200, "{}", "{}", None, None, 100, "complete").unwrap();

        let stats = get_session_stats(&conn).unwrap();
        assert_eq!(stats[0]["pending_count"], 1);
    }

    #[test]
    fn find_session_id_by_cwd_returns_most_recent() {
        let conn = setup();
        let mut s1 = sample_session("s1");
        s1.cwd = Some("/proj".to_string());
        s1.last_seen_at = "2024-01-01T00:00:00Z".to_string();
        let mut s2 = sample_session("s2");
        s2.cwd = Some("/proj".to_string());
        s2.last_seen_at = "2024-06-01T00:00:00Z".to_string();
        upsert_session(&conn, &s1).unwrap();
        upsert_session(&conn, &s2).unwrap();

        let id = find_session_id_by_cwd(&conn, "/proj").unwrap();
        assert_eq!(id, Some("s2".to_string())); // most recent
    }

    #[test]
    fn find_session_id_by_cwd_returns_none_when_missing() {
        let conn = setup();
        let id = find_session_id_by_cwd(&conn, "/nonexistent").unwrap();
        assert!(id.is_none());
    }

    #[test]
    fn get_sessions_ordered_by_last_seen_desc() {
        let conn = setup();
        let mut s1 = sample_session("s1");
        s1.last_seen_at = "2024-01-01T00:00:00Z".to_string();
        let mut s2 = sample_session("s2");
        s2.last_seen_at = "2024-06-01T00:00:00Z".to_string();
        upsert_session(&conn, &s1).unwrap();
        upsert_session(&conn, &s2).unwrap();

        let sessions = get_sessions(&conn).unwrap();
        assert_eq!(sessions[0].id, "s2"); // most recent first
        assert_eq!(sessions[1].id, "s1");
    }
}

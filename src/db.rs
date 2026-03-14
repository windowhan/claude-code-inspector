use anyhow::Result;
use rusqlite::{Connection, params};
use crate::types::{RequestRecord, RoutingConfig, RoutingRule, SessionRecord};

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

        CREATE TABLE IF NOT EXISTS routing_config (
            id                    INTEGER PRIMARY KEY DEFAULT 1,
            enabled               INTEGER NOT NULL DEFAULT 0,
            classifier_base_url   TEXT NOT NULL DEFAULT 'https://api.anthropic.com',
            classifier_api_key    TEXT NOT NULL DEFAULT '',
            classifier_model      TEXT NOT NULL DEFAULT 'claude-haiku-4-5-20251001',
            classifier_prompt     TEXT NOT NULL DEFAULT ''
        );

        CREATE TABLE IF NOT EXISTS routing_rules (
            id             TEXT PRIMARY KEY,
            priority       INTEGER NOT NULL DEFAULT 100,
            enabled        INTEGER NOT NULL DEFAULT 1,
            category       TEXT NOT NULL,
            description    TEXT NOT NULL DEFAULT '',
            target_url      TEXT NOT NULL,
            api_key         TEXT NOT NULL DEFAULT '',
            prompt_override TEXT NOT NULL DEFAULT '',
            model_override  TEXT NOT NULL DEFAULT '',
            label          TEXT NOT NULL DEFAULT ''
        );

        CREATE INDEX IF NOT EXISTS idx_requests_timestamp ON requests(timestamp DESC);
        CREATE INDEX IF NOT EXISTS idx_requests_session ON requests(session_id);

        CREATE TABLE IF NOT EXISTS file_access (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            session_id TEXT NOT NULL,
            request_id TEXT NOT NULL,
            file_path TEXT NOT NULL,
            access_type TEXT NOT NULL,
            timestamp TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_file_access_session ON file_access(session_id);
        CREATE INDEX IF NOT EXISTS idx_file_access_request ON file_access(request_id);
        CREATE INDEX IF NOT EXISTS idx_file_access_path ON file_access(file_path);

        CREATE TABLE IF NOT EXISTS supervisor_cache (
            id TEXT PRIMARY KEY,
            session_id TEXT NOT NULL,
            tool_name TEXT NOT NULL,
            request_count INTEGER NOT NULL,
            pending_count INTEGER NOT NULL,
            result TEXT NOT NULL,
            cached_at TEXT NOT NULL
        );
    ")?;
    // Migration: add starred column if not yet present (safe to run on existing DBs)
    let _ = conn.execute(
        "ALTER TABLE requests ADD COLUMN starred INTEGER NOT NULL DEFAULT 0", [],
    );
    // Migration: add memo column
    let _ = conn.execute(
        "ALTER TABLE requests ADD COLUMN memo TEXT NOT NULL DEFAULT ''", [],
    );
    // Migration: add agent_type column
    let _ = conn.execute(
        "ALTER TABLE requests ADD COLUMN agent_type TEXT NOT NULL DEFAULT 'main'", [],
    );
    // Migration: add agent_task column
    let _ = conn.execute(
        "ALTER TABLE requests ADD COLUMN agent_task TEXT NOT NULL DEFAULT ''", [],
    );
    // Migration: add routing columns
    let _ = conn.execute(
        "ALTER TABLE requests ADD COLUMN routing_category TEXT NOT NULL DEFAULT ''", [],
    );
    let _ = conn.execute(
        "ALTER TABLE requests ADD COLUMN routed_to_url TEXT NOT NULL DEFAULT ''", [],
    );
    // Migration: add description column to routing_rules
    let _ = conn.execute(
        "ALTER TABLE routing_rules ADD COLUMN description TEXT NOT NULL DEFAULT ''", [],
    );
    // Migration: add api_key column to routing_rules
    let _ = conn.execute(
        "ALTER TABLE routing_rules ADD COLUMN api_key TEXT NOT NULL DEFAULT ''", [],
    );
    // Migration: add prompt_override column to routing_rules
    let _ = conn.execute(
        "ALTER TABLE routing_rules ADD COLUMN prompt_override TEXT NOT NULL DEFAULT ''", [],
    );
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
        "INSERT INTO requests (id, session_id, timestamp, method, path, request_headers, request_body, is_streaming, status, agent_type, agent_task, routing_category, routed_to_url)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
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
            req.agent_type,
            req.agent_task,
            req.routing_category,
            req.routed_to_url,
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

pub fn set_request_memo(conn: &Connection, id: &str, memo: &str) -> Result<()> {
    conn.execute(
        "UPDATE requests SET memo = ?1 WHERE id = ?2",
        params![memo, id],
    )?;
    Ok(())
}

pub fn update_request_status(conn: &Connection, id: &str, status: &str) -> Result<()> {
    conn.execute(
        "UPDATE requests SET status = ?1 WHERE id = ?2",
        params![status, id],
    )?;
    Ok(())
}

pub fn update_request_body(conn: &Connection, id: &str, body: &str) -> Result<()> {
    conn.execute(
        "UPDATE requests SET request_body = ?1 WHERE id = ?2",
        params![body, id],
    )?;
    Ok(())
}

/// Delete a session and all its requests, file access records, and cached analyses.
pub fn delete_session(conn: &Connection, id: &str) -> Result<()> {
    conn.execute("DELETE FROM file_access WHERE session_id = ?1", params![id])?;
    conn.execute("DELETE FROM supervisor_cache WHERE session_id = ?1", params![id])?;
    conn.execute("DELETE FROM requests WHERE session_id = ?1", params![id])?;
    conn.execute("DELETE FROM sessions WHERE id = ?1", params![id])?;
    Ok(())
}

/// Set the starred flag on a request.
pub fn set_request_starred(conn: &Connection, id: &str, starred: bool) -> Result<()> {
    conn.execute(
        "UPDATE requests SET starred = ?1 WHERE id = ?2",
        params![starred as i32, id],
    )?;
    Ok(())
}

/// Returns all starred requests ordered by timestamp descending.
pub fn get_starred_requests(conn: &Connection, limit: i64, offset: i64) -> Result<Vec<RequestRecord>> {
    let mut stmt = conn.prepare(
        "SELECT id, session_id, timestamp, method, path, request_headers, request_body,
                response_status, response_headers, response_body, is_streaming,
                input_tokens, output_tokens, duration_ms, status, starred, memo, agent_type, agent_task,
                routing_category, routed_to_url
         FROM requests WHERE starred = 1
         ORDER BY timestamp DESC LIMIT ?1 OFFSET ?2"
    )?;
    let rows = stmt.query_map(params![limit, offset], map_request_row)?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows)
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

#[allow(dead_code)]
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
                    input_tokens, output_tokens, duration_ms, status, starred, memo, agent_type, agent_task,
                    routing_category, routed_to_url
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
                    input_tokens, output_tokens, duration_ms, status, starred, memo, agent_type, agent_task,
                    routing_category, routed_to_url
             FROM requests
             ORDER BY timestamp DESC LIMIT ?1 OFFSET ?2"
        )?;
        let x = stmt.query_map(params![limit, offset], map_request_row)?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        x
    };
    Ok(rows)
}

pub fn search_requests(
    conn: &Connection,
    query: &str,
    session_id: Option<&str>,
    limit: i64,
    offset: i64,
) -> Result<Vec<RequestRecord>> {
    let pattern = format!("%{}%", query);
    let rows = if let Some(sid) = session_id {
        let mut stmt = conn.prepare(
            "SELECT id, session_id, timestamp, method, path, request_headers, request_body,
                    response_status, response_headers, response_body, is_streaming,
                    input_tokens, output_tokens, duration_ms, status, starred, memo, agent_type, agent_task,
                    routing_category, routed_to_url
             FROM requests
             WHERE session_id = ?1
               AND (request_body LIKE ?2 OR response_body LIKE ?2 OR path LIKE ?2)
             ORDER BY timestamp DESC LIMIT ?3 OFFSET ?4"
        )?;
        let x = stmt.query_map(params![sid, pattern, limit, offset], map_request_row)?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        x
    } else {
        let mut stmt = conn.prepare(
            "SELECT id, session_id, timestamp, method, path, request_headers, request_body,
                    response_status, response_headers, response_body, is_streaming,
                    input_tokens, output_tokens, duration_ms, status, starred, memo, agent_type, agent_task,
                    routing_category, routed_to_url
             FROM requests
             WHERE request_body LIKE ?1 OR response_body LIKE ?1 OR path LIKE ?1
             ORDER BY timestamp DESC LIMIT ?2 OFFSET ?3"
        )?;
        let x = stmt.query_map(params![pattern, limit, offset], map_request_row)?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        x
    };
    Ok(rows)
}

pub fn get_request_by_id(conn: &Connection, id: &str) -> Result<Option<RequestRecord>> {
    let mut stmt = conn.prepare(
        "SELECT id, session_id, timestamp, method, path, request_headers, request_body,
                response_status, response_headers, response_body, is_streaming,
                input_tokens, output_tokens, duration_ms, status, starred, memo, agent_type, agent_task,
                routing_category, routed_to_url
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
        starred: row.get::<_, i32>(15)? != 0,
        memo: row.get::<_, String>(16).unwrap_or_default(),
        agent_type: row.get::<_, String>(17).unwrap_or_else(|_| "main".to_string()),
        agent_task: row.get::<_, String>(18).unwrap_or_default(),
        routing_category: row.get::<_, String>(19).unwrap_or_default(),
        routed_to_url: row.get::<_, String>(20).unwrap_or_default(),
    })
}

// ── Routing config CRUD ───────────────────────────────────────────────────────

pub fn get_routing_config(conn: &Connection) -> Result<RoutingConfig> {
    // Ensure a default row exists
    conn.execute(
        "INSERT OR IGNORE INTO routing_config (id) VALUES (1)",
        [],
    )?;
    let mut stmt = conn.prepare(
        "SELECT enabled, classifier_base_url, classifier_api_key, classifier_model,
                classifier_prompt
         FROM routing_config WHERE id = 1"
    )?;
    let config = stmt.query_row([], |row| {
        let enabled: i32 = row.get(0)?;
        Ok(RoutingConfig {
            enabled: enabled != 0,
            classifier_base_url: row.get(1)?,
            classifier_api_key: row.get(2)?,
            classifier_model: row.get(3)?,
            classifier_prompt: row.get(4)?,
        })
    })?;
    Ok(config)
}

pub fn save_routing_config(conn: &Connection, config: &RoutingConfig) -> Result<()> {
    conn.execute(
        "INSERT OR REPLACE INTO routing_config
         (id, enabled, classifier_base_url, classifier_api_key, classifier_model, classifier_prompt)
         VALUES (1, ?1, ?2, ?3, ?4, ?5)",
        params![
            config.enabled as i32,
            config.classifier_base_url,
            config.classifier_api_key,
            config.classifier_model,
            config.classifier_prompt,
        ],
    )?;
    Ok(())
}

pub fn get_routing_rules(conn: &Connection) -> Result<Vec<RoutingRule>> {
    let mut stmt = conn.prepare(
        "SELECT id, priority, enabled, category, description, target_url, api_key, prompt_override, model_override, label
         FROM routing_rules ORDER BY priority ASC"
    )?;
    let rules = stmt.query_map([], |row| {
        let enabled: i32 = row.get(2)?;
        Ok(RoutingRule {
            id: row.get(0)?,
            priority: row.get(1)?,
            enabled: enabled != 0,
            category: row.get(3)?,
            description: row.get::<_, String>(4).unwrap_or_default(),
            target_url: row.get(5)?,
            api_key: row.get::<_, String>(6).unwrap_or_default(),
            prompt_override: row.get::<_, String>(7).unwrap_or_default(),
            model_override: row.get(8)?,
            label: row.get(9)?,
        })
    })?
    .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rules)
}

pub fn insert_routing_rule(conn: &Connection, rule: &RoutingRule) -> Result<()> {
    conn.execute(
        "INSERT INTO routing_rules (id, priority, enabled, category, description, target_url, api_key, prompt_override, model_override, label)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![
            rule.id,
            rule.priority,
            rule.enabled as i32,
            rule.category,
            rule.description,
            rule.target_url,
            rule.api_key,
            rule.prompt_override,
            rule.model_override,
            rule.label,
        ],
    )?;
    Ok(())
}

pub fn update_routing_rule(conn: &Connection, rule: &RoutingRule) -> Result<()> {
    conn.execute(
        "UPDATE routing_rules SET
         priority = ?1, enabled = ?2, category = ?3, description = ?4, target_url = ?5,
         api_key = ?6, prompt_override = ?7, model_override = ?8, label = ?9
         WHERE id = ?10",
        params![
            rule.priority,
            rule.enabled as i32,
            rule.category,
            rule.description,
            rule.target_url,
            rule.api_key,
            rule.prompt_override,
            rule.model_override,
            rule.label,
            rule.id,
        ],
    )?;
    Ok(())
}

pub fn delete_routing_rule(conn: &Connection, id: &str) -> Result<()> {
    conn.execute("DELETE FROM routing_rules WHERE id = ?1", params![id])?;
    Ok(())
}

/// Reassigns priority 1..N in the order given by `ids`.
pub fn reorder_routing_rules(conn: &Connection, ids: &[String]) -> Result<()> {
    for (i, id) in ids.iter().enumerate() {
        conn.execute(
            "UPDATE routing_rules SET priority = ?1 WHERE id = ?2",
            params![(i + 1) as i64, id],
        )?;
    }
    Ok(())
}

// ── Supervisor (file access + cache) ─────────────────────────────────────────

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct FileAccessRecord {
    pub session_id: String,
    pub request_id: String,
    pub file_path: String,
    pub access_type: String,
    pub timestamp: String,
}

pub fn insert_file_access(
    conn: &Connection,
    session_id: &str,
    request_id: &str,
    file_path: &str,
    access_type: &str,
    timestamp: &str,
) -> Result<()> {
    conn.execute(
        "INSERT INTO file_access (session_id, request_id, file_path, access_type, timestamp)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![session_id, request_id, file_path, access_type, timestamp],
    )?;
    Ok(())
}

pub fn get_file_access_by_session(conn: &Connection, session_id: &str) -> Result<Vec<FileAccessRecord>> {
    let mut stmt = conn.prepare(
        "SELECT session_id, request_id, file_path, access_type, timestamp
         FROM file_access WHERE session_id = ?1
         ORDER BY timestamp ASC"
    )?;
    let rows = stmt.query_map(params![session_id], |row| {
        Ok(FileAccessRecord {
            session_id: row.get(0)?,
            request_id: row.get(1)?,
            file_path: row.get(2)?,
            access_type: row.get(3)?,
            timestamp: row.get(4)?,
        })
    })?
    .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Returns cached supervisor result if request_count and pending_count both match.
pub fn get_supervisor_cache(
    conn: &Connection,
    session_id: &str,
    tool_name: &str,
    current_request_count: i64,
    current_pending_count: i64,
) -> Result<Option<String>> {
    let id = format!("{session_id}:{tool_name}");
    let mut stmt = conn.prepare(
        "SELECT result FROM supervisor_cache
         WHERE id = ?1 AND request_count = ?2 AND pending_count = ?3"
    )?;
    let mut rows = stmt.query_map(params![id, current_request_count, current_pending_count], |row| {
        row.get::<_, String>(0)
    })?;
    Ok(rows.next().transpose()?)
}

pub fn set_supervisor_cache(
    conn: &Connection,
    session_id: &str,
    tool_name: &str,
    request_count: i64,
    pending_count: i64,
    result: &str,
) -> Result<()> {
    let id = format!("{session_id}:{tool_name}");
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "INSERT OR REPLACE INTO supervisor_cache (id, session_id, tool_name, request_count, pending_count, result, cached_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![id, session_id, tool_name, request_count, pending_count, result, now],
    )?;
    Ok(())
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
            starred: false,
            memo: String::new(),
            agent_type: "main".to_string(),
            agent_task: String::new(),
            routing_category: String::new(),
            routed_to_url: String::new(),
        }
    }

    fn sample_rule(id: &str, priority: i64, category: &str) -> RoutingRule {
        RoutingRule {
            id: id.to_string(),
            priority,
            enabled: true,
            category: category.to_string(),
            description: format!("description for {category}"),
            target_url: "https://openai.com".to_string(),
            api_key: String::new(),
            prompt_override: String::new(),
            model_override: "gpt-4".to_string(),
            label: format!("rule-{id}"),
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
        // Verify routing_config table
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM routing_config", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0);
        // Verify routing_rules table
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM routing_rules", [], |r| r.get(0))
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
    fn delete_session_removes_session_and_requests() {
        let conn = setup();
        upsert_session(&conn, &sample_session("s1")).unwrap();
        upsert_session(&conn, &sample_session("s2")).unwrap();
        insert_request(&conn, &sample_request("r1", "s1")).unwrap();
        insert_request(&conn, &sample_request("r2", "s1")).unwrap();
        insert_request(&conn, &sample_request("r3", "s2")).unwrap();

        delete_session(&conn, "s1").unwrap();

        let sessions = get_sessions(&conn).unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].id, "s2");

        let reqs = get_requests(&conn, None, 10, 0).unwrap();
        assert_eq!(reqs.len(), 1);
        assert_eq!(reqs[0].id, "r3");
    }

    #[test]
    fn set_request_starred_and_get_starred() {
        let conn = setup();
        upsert_session(&conn, &sample_session("s1")).unwrap();
        insert_request(&conn, &sample_request("r1", "s1")).unwrap();
        insert_request(&conn, &sample_request("r2", "s1")).unwrap();

        set_request_starred(&conn, "r1", true).unwrap();

        let starred = get_starred_requests(&conn, 10, 0).unwrap();
        assert_eq!(starred.len(), 1);
        assert_eq!(starred[0].id, "r1");
        assert!(starred[0].starred);
    }

    #[test]
    fn set_request_starred_toggle_off() {
        let conn = setup();
        upsert_session(&conn, &sample_session("s1")).unwrap();
        insert_request(&conn, &sample_request("r1", "s1")).unwrap();

        set_request_starred(&conn, "r1", true).unwrap();
        set_request_starred(&conn, "r1", false).unwrap();

        let starred = get_starred_requests(&conn, 10, 0).unwrap();
        assert!(starred.is_empty());
    }

    #[test]
    fn search_requests_matches_request_body() {
        let conn = setup();
        upsert_session(&conn, &sample_session("s1")).unwrap();
        let mut r1 = sample_request("r1", "s1");
        r1.request_body = r#"{"model":"claude-haiku","messages":[]}"#.to_string();
        let mut r2 = sample_request("r2", "s1");
        r2.request_body = r#"{"model":"claude-sonnet","messages":[]}"#.to_string();
        insert_request(&conn, &r1).unwrap();
        insert_request(&conn, &r2).unwrap();

        let results = search_requests(&conn, "haiku", None, 10, 0).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "r1");
    }

    #[test]
    fn search_requests_matches_response_body() {
        let conn = setup();
        upsert_session(&conn, &sample_session("s1")).unwrap();
        insert_request(&conn, &sample_request("r1", "s1")).unwrap();
        super::update_request_complete(&conn, "r1", 200, "{}", r#"{"content":"hello world"}"#, None, None, 100, "complete").unwrap();

        let results = search_requests(&conn, "hello world", None, 10, 0).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "r1");
    }

    #[test]
    fn search_requests_matches_path() {
        let conn = setup();
        upsert_session(&conn, &sample_session("s1")).unwrap();
        insert_request(&conn, &sample_request("r1", "s1")).unwrap();

        let results = search_requests(&conn, "/v1/messages", None, 10, 0).unwrap();
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn search_requests_no_match_returns_empty() {
        let conn = setup();
        upsert_session(&conn, &sample_session("s1")).unwrap();
        insert_request(&conn, &sample_request("r1", "s1")).unwrap();

        let results = search_requests(&conn, "nonexistent-string-xyz", None, 10, 0).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn search_requests_with_session_filter() {
        let conn = setup();
        upsert_session(&conn, &sample_session("s1")).unwrap();
        upsert_session(&conn, &sample_session("s2")).unwrap();
        let mut r1 = sample_request("r1", "s1");
        r1.request_body = r#"{"model":"claude-haiku"}"#.to_string();
        let mut r2 = sample_request("r2", "s2");
        r2.request_body = r#"{"model":"claude-haiku"}"#.to_string();
        insert_request(&conn, &r1).unwrap();
        insert_request(&conn, &r2).unwrap();

        let results = search_requests(&conn, "haiku", Some("s1"), 10, 0).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "r1");
    }

    #[test]
    fn get_starred_requests_empty_when_none_starred() {
        let conn = setup();
        upsert_session(&conn, &sample_session("s1")).unwrap();
        insert_request(&conn, &sample_request("r1", "s1")).unwrap();

        let starred = get_starred_requests(&conn, 10, 0).unwrap();
        assert!(starred.is_empty());
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
    fn update_request_status_changes_status() {
        let conn = setup();
        upsert_session(&conn, &sample_session("s1")).unwrap();
        insert_request(&conn, &sample_request("r1", "s1")).unwrap();

        update_request_status(&conn, "r1", "intercepted").unwrap();
        let found = get_request_by_id(&conn, "r1").unwrap().unwrap();
        assert_eq!(found.status, "intercepted");

        update_request_status(&conn, "r1", "rejected").unwrap();
        let found = get_request_by_id(&conn, "r1").unwrap().unwrap();
        assert_eq!(found.status, "rejected");
    }

    #[test]
    fn update_request_body_changes_body() {
        let conn = setup();
        upsert_session(&conn, &sample_session("s1")).unwrap();
        insert_request(&conn, &sample_request("r1", "s1")).unwrap();

        let new_body = r#"{"model":"new-model","messages":[]}"#;
        update_request_body(&conn, "r1", new_body).unwrap();
        let found = get_request_by_id(&conn, "r1").unwrap().unwrap();
        assert_eq!(found.request_body, new_body);
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

    // ── Routing config tests ──────────────────────────────────────────────────

    #[test]
    fn routing_config_default_and_save() {
        let conn = setup();
        // get_routing_config returns default when no row inserted
        let config = get_routing_config(&conn).unwrap();
        assert!(!config.enabled);
        assert_eq!(config.classifier_model, "claude-haiku-4-5-20251001");

        // Save a custom config
        let mut custom = config.clone();
        custom.enabled = true;
        custom.classifier_model = "gpt-4".to_string();
        save_routing_config(&conn, &custom).unwrap();

        let loaded = get_routing_config(&conn).unwrap();
        assert!(loaded.enabled);
        assert_eq!(loaded.classifier_model, "gpt-4");
    }

    #[test]
    fn routing_config_prompt_round_trip() {
        let conn = setup();
        let mut cfg = RoutingConfig::default();
        cfg.classifier_prompt = "Custom prompt".to_string();
        save_routing_config(&conn, &cfg).unwrap();
        let loaded = get_routing_config(&conn).unwrap();
        assert_eq!(loaded.classifier_prompt, "Custom prompt");
    }

    #[test]
    fn routing_rules_insert_update_delete() {
        let conn = setup();
        let rule = sample_rule("rule-1", 10, "code_gen");
        insert_routing_rule(&conn, &rule).unwrap();

        let rules = get_routing_rules(&conn).unwrap();
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].id, "rule-1");
        assert_eq!(rules[0].category, "code_gen");

        // Update
        let mut updated = rule.clone();
        updated.category = "docs".to_string();
        updated.label = "updated".to_string();
        update_routing_rule(&conn, &updated).unwrap();

        let rules = get_routing_rules(&conn).unwrap();
        assert_eq!(rules[0].category, "docs");
        assert_eq!(rules[0].label, "updated");

        // Delete
        delete_routing_rule(&conn, "rule-1").unwrap();
        let rules = get_routing_rules(&conn).unwrap();
        assert!(rules.is_empty());
    }

    #[test]
    fn routing_rules_reorder_by_ids() {
        let conn = setup();
        insert_routing_rule(&conn, &sample_rule("r1", 1, "code_gen")).unwrap();
        insert_routing_rule(&conn, &sample_rule("r2", 2, "docs")).unwrap();
        insert_routing_rule(&conn, &sample_rule("r3", 3, "qa")).unwrap();

        // Reorder: r3, r1, r2
        reorder_routing_rules(&conn, &[
            "r3".to_string(), "r1".to_string(), "r2".to_string(),
        ]).unwrap();

        let rules = get_routing_rules(&conn).unwrap();
        // Ordered by priority asc: r3=1, r1=2, r2=3
        assert_eq!(rules[0].id, "r3");
        assert_eq!(rules[0].priority, 1);
        assert_eq!(rules[1].id, "r1");
        assert_eq!(rules[1].priority, 2);
        assert_eq!(rules[2].id, "r2");
        assert_eq!(rules[2].priority, 3);
    }

    #[test]
    fn routing_rules_ordered_by_priority() {
        let conn = setup();
        insert_routing_rule(&conn, &sample_rule("r-high", 100, "code_gen")).unwrap();
        insert_routing_rule(&conn, &sample_rule("r-low", 10, "docs")).unwrap();

        let rules = get_routing_rules(&conn).unwrap();
        assert_eq!(rules[0].id, "r-low");  // priority 10 first
        assert_eq!(rules[1].id, "r-high"); // priority 100 second
    }

    #[test]
    fn requests_routing_columns_persist() {
        let conn = setup();
        upsert_session(&conn, &sample_session("s1")).unwrap();
        let mut req = sample_request("r1", "s1");
        req.routing_category = "code_gen".to_string();
        req.routed_to_url = "https://openai.com".to_string();
        insert_request(&conn, &req).unwrap();

        let found = get_request_by_id(&conn, "r1").unwrap().unwrap();
        assert_eq!(found.routing_category, "code_gen");
        assert_eq!(found.routed_to_url, "https://openai.com");
    }

    // ── Supervisor DB tests ──────────────────────────────────────────────────

    #[test]
    fn file_access_insert_and_query() {
        let conn = setup();
        insert_file_access(&conn, "s1", "r1", "/src/main.rs", "read", "2024-01-01T00:00:00Z").unwrap();
        insert_file_access(&conn, "s1", "r1", "/src/db.rs", "edit", "2024-01-01T00:01:00Z").unwrap();
        insert_file_access(&conn, "s2", "r2", "/other.rs", "read", "2024-01-01T00:02:00Z").unwrap();

        let accesses = get_file_access_by_session(&conn, "s1").unwrap();
        assert_eq!(accesses.len(), 2);
        assert_eq!(accesses[0].file_path, "/src/main.rs");
        assert_eq!(accesses[0].access_type, "read");
        assert_eq!(accesses[1].file_path, "/src/db.rs");
        assert_eq!(accesses[1].access_type, "edit");
    }

    #[test]
    fn supervisor_cache_hit_and_miss() {
        let conn = setup();
        set_supervisor_cache(&conn, "s1", "get_session_summary", 5, 1, r#"{"cached":true}"#).unwrap();

        // Hit: same counts
        let hit = get_supervisor_cache(&conn, "s1", "get_session_summary", 5, 1).unwrap();
        assert_eq!(hit, Some(r#"{"cached":true}"#.to_string()));

        // Miss: different request_count
        let miss = get_supervisor_cache(&conn, "s1", "get_session_summary", 6, 1).unwrap();
        assert!(miss.is_none());

        // Miss: different pending_count
        let miss = get_supervisor_cache(&conn, "s1", "get_session_summary", 5, 0).unwrap();
        assert!(miss.is_none());

        // Miss: different tool
        let miss = get_supervisor_cache(&conn, "s1", "get_file_coverage", 5, 1).unwrap();
        assert!(miss.is_none());
    }

    #[test]
    fn delete_session_cascades_to_file_access_and_cache() {
        let conn = setup();
        upsert_session(&conn, &sample_session("s1")).unwrap();
        insert_request(&conn, &sample_request("r1", "s1")).unwrap();
        insert_file_access(&conn, "s1", "r1", "/src/main.rs", "read", "2024-01-01T00:00:00Z").unwrap();
        set_supervisor_cache(&conn, "s1", "summary", 1, 0, "{}").unwrap();

        delete_session(&conn, "s1").unwrap();

        let accesses = get_file_access_by_session(&conn, "s1").unwrap();
        assert!(accesses.is_empty());
        let cache = get_supervisor_cache(&conn, "s1", "summary", 1, 0).unwrap();
        assert!(cache.is_none());
    }
}

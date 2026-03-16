//! Supervisor module — file access parsing, session summary, and pattern detection.
//!
//! Provides data for MCP supervisor tools. No LLM calls — Claude Code is the brain.

use serde_json::{json, Value};
use crate::db::FileAccessRecord;
use crate::types::RequestRecord;

// ── File access parsing ──────────────────────────────────────────────────────

/// Shared helper: derive (file_path, access_type, read_range) from a tool_use name + input.
fn file_access_from_tool_use(name: &str, input: &Value) -> Option<(String, String, String)> {
    let access_type = match name {
        "Read" => "read",
        "Write" => "write",
        "Edit" => "edit",
        "Grep" | "Glob" => "search",
        _ => return None,
    };
    let file_path = input
        .get("file_path")
        .or_else(|| input.get("path"))
        .and_then(|p| p.as_str())?;
    if file_path.is_empty() {
        return None;
    }
    let read_range = if access_type == "read" {
        let offset = input.get("offset").and_then(|v| v.as_i64());
        let limit  = input.get("limit").and_then(|v| v.as_i64());
        match (offset, limit) {
            (None, None)       => "full".to_string(),
            (Some(o), Some(l)) => format!("offset:{o},limit:{l}"),
            (Some(o), None)    => format!("offset:{o}"),
            (None, Some(l))    => format!("limit:{l}"),
        }
    } else {
        String::new()
    };
    Some((file_path.to_string(), access_type.to_string(), read_range))
}

/// Extract file accesses from a raw Anthropic SSE response.
/// Parses only the tool_use content blocks that Claude generated in THIS response turn,
/// so each access is correctly attributed to the specific request that produced it.
pub fn extract_file_accesses_from_sse(raw_sse: &[u8]) -> Vec<(String, String, String)> {
    let text = String::from_utf8_lossy(raw_sse);
    // index → (tool_name, accumulated_input_json)
    let mut tool_blocks: std::collections::HashMap<u64, (String, String)> = std::collections::HashMap::new();

    for line in text.lines() {
        let Some(json_str) = line.strip_prefix("data: ") else { continue };
        let Ok(json) = serde_json::from_str::<Value>(json_str) else { continue };

        match json.get("type").and_then(|t| t.as_str()) {
            Some("content_block_start") => {
                let Some(idx) = json.get("index").and_then(|i| i.as_u64()) else { continue };
                let Some(block) = json.get("content_block") else { continue };
                if block.get("type").and_then(|t| t.as_str()) != Some("tool_use") { continue }
                let name = block.get("name").and_then(|n| n.as_str()).unwrap_or("").to_string();
                tool_blocks.insert(idx, (name, String::new()));
            }
            Some("content_block_delta") => {
                let Some(idx) = json.get("index").and_then(|i| i.as_u64()) else { continue };
                let Some(delta) = json.get("delta") else { continue };
                if delta.get("type").and_then(|t| t.as_str()) != Some("input_json_delta") { continue }
                let Some(partial) = delta.get("partial_json").and_then(|p| p.as_str()) else { continue };
                if let Some(entry) = tool_blocks.get_mut(&idx) {
                    entry.1.push_str(partial);
                }
            }
            _ => {}
        }
    }

    let mut accesses = Vec::new();
    for (_, (name, input_json)) in &tool_blocks {
        if input_json.is_empty() { continue }
        let Ok(input) = serde_json::from_str::<Value>(input_json) else { continue };
        if let Some(acc) = file_access_from_tool_use(name, &input) {
            accesses.push(acc);
        }
    }
    accesses
}

/// Extract file accesses from a non-streaming Anthropic response JSON body.
pub fn extract_file_accesses_from_response(resp_body: &str) -> Vec<(String, String, String)> {
    let Ok(body) = serde_json::from_str::<Value>(resp_body) else { return Vec::new() };
    let Some(content) = body.get("content").and_then(|c| c.as_array()) else { return Vec::new() };
    let mut accesses = Vec::new();
    for block in content {
        if block.get("type").and_then(|t| t.as_str()) != Some("tool_use") { continue }
        let name = block.get("name").and_then(|n| n.as_str()).unwrap_or("");
        let Some(input) = block.get("input") else { continue };
        if let Some(acc) = file_access_from_tool_use(name, input) {
            accesses.push(acc);
        }
    }
    accesses
}

/// Extract file accesses from request_body.
/// NOTE: This iterates ALL accumulated messages — only use in tests.
/// For production use extract_file_accesses_from_sse / extract_file_accesses_from_response.
#[cfg(test)]
fn extract_file_accesses(request_body: &str) -> Vec<(String, String, String)> {
    let mut accesses = Vec::new();

    let body: Value = match serde_json::from_str(request_body) {
        Ok(v) => v,
        Err(_) => return accesses,
    };

    let messages = match body.get("messages").and_then(|m| m.as_array()) {
        Some(msgs) => msgs,
        None => return accesses,
    };

    for msg in messages {
        let role = msg.get("role").and_then(|r| r.as_str()).unwrap_or("");
        if role != "assistant" {
            continue;
        }

        let content = match msg.get("content") {
            Some(c) => c,
            None => continue,
        };

        let blocks = if let Some(arr) = content.as_array() {
            arr.clone()
        } else {
            continue;
        };

        for block in &blocks {
            if block.get("type").and_then(|t| t.as_str()) != Some("tool_use") {
                continue;
            }

            let name = match block.get("name").and_then(|n| n.as_str()) {
                Some(n) => n,
                None => continue,
            };

            let input = match block.get("input") {
                Some(i) => i,
                None => continue,
            };

            let access_type = match name {
                "Read" => "read",
                "Write" => "write",
                "Edit" => "edit",
                "Grep" | "Glob" => "search",
                _ => continue,
            };

            // Extract file_path from input.file_path or input.path
            let file_path = input
                .get("file_path")
                .or_else(|| input.get("path"))
                .and_then(|p| p.as_str());

            // For Read: extract offset/limit to determine if full or partial read
            let read_range = if access_type == "read" {
                let offset = input.get("offset").and_then(|v| v.as_i64());
                let limit = input.get("limit").and_then(|v| v.as_i64());
                match (offset, limit) {
                    (None, None) => "full".to_string(),
                    (Some(o), Some(l)) => format!("offset:{o},limit:{l}"),
                    (Some(o), None) => format!("offset:{o}"),
                    (None, Some(l)) => format!("limit:{l}"),
                }
            } else {
                String::new()
            };

            if let Some(path) = file_path {
                if !path.is_empty() {
                    accesses.push((path.to_string(), access_type.to_string(), read_range));
                }
            }
        }
    }

    accesses
}

// ── Session summary ──────────────────────────────────────────────────────────

/// Build a structured summary of a session's request flow.
/// Excludes non-message requests like count_tokens.
pub fn build_session_summary(requests: &[RequestRecord]) -> Value {
    let mut total_input: i64 = 0;
    let mut total_output: i64 = 0;
    let mut error_count: i64 = 0;
    let mut error_details: Vec<Value> = Vec::new();

    let filtered: Vec<&RequestRecord> = requests
        .iter()
        .filter(|r| !r.path.contains("count_tokens"))
        .collect();

    let req_summaries: Vec<Value> = filtered
        .iter()
        .map(|r| {
            total_input += r.input_tokens.unwrap_or(0);
            total_output += r.output_tokens.unwrap_or(0);

            if r.status == "error" {
                error_count += 1;
                error_details.push(json!({
                    "request_id": r.id,
                    "response_status": r.response_status,
                    "timestamp": r.timestamp,
                }));
            }

            // Extract model from request_body
            let model = serde_json::from_str::<Value>(&r.request_body)
                .ok()
                .and_then(|b| b.get("model").and_then(|m| m.as_str()).map(|s| s.to_string()))
                .unwrap_or_default();

            // Count tool_use blocks from response (accurate per-request attribution)
            let tool_calls = count_tool_calls(r.response_body.as_deref().unwrap_or(""));

            json!({
                "request_id": r.id,
                "timestamp": r.timestamp,
                "agent_type": r.agent_type,
                "agent_task": r.agent_task,
                "status": r.status,
                "model": model,
                "input_tokens": r.input_tokens,
                "output_tokens": r.output_tokens,
                "duration_ms": r.duration_ms,
                "tool_calls": tool_calls,
            })
        })
        .collect();

    json!({
        "request_count": filtered.len(),
        "excluded_count": requests.len() - filtered.len(),
        "total_input_tokens": total_input,
        "total_output_tokens": total_output,
        "total_tokens": total_input + total_output,
        "error_count": error_count,
        "error_details": error_details,
        "requests": req_summaries,
    })
}

fn count_tool_calls(response_body: &str) -> Value {
    let mut counts: std::collections::HashMap<String, i64> = std::collections::HashMap::new();

    if let Ok(body) = serde_json::from_str::<Value>(response_body) {
        // SSE response: {"accumulated_content": "...", "raw_sse": "..."}
        if let Some(raw_sse) = body.get("raw_sse").and_then(|v| v.as_str()) {
            for line in raw_sse.lines() {
                let Some(json_str) = line.strip_prefix("data: ") else { continue };
                let Ok(json) = serde_json::from_str::<Value>(json_str) else { continue };
                if json.get("type").and_then(|t| t.as_str()) == Some("content_block_start") {
                    if let Some(block) = json.get("content_block") {
                        if block.get("type").and_then(|t| t.as_str()) == Some("tool_use") {
                            if let Some(name) = block.get("name").and_then(|n| n.as_str()) {
                                *counts.entry(name.to_string()).or_insert(0) += 1;
                            }
                        }
                    }
                }
            }
        } else if let Some(content) = body.get("content").and_then(|c| c.as_array()) {
            // Non-streaming response: {"content": [...]}
            for block in content {
                if block.get("type").and_then(|t| t.as_str()) == Some("tool_use") {
                    if let Some(name) = block.get("name").and_then(|n| n.as_str()) {
                        *counts.entry(name.to_string()).or_insert(0) += 1;
                    }
                }
            }
        }
    }

    json!(counts)
}

// ── Pattern detection ────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct Pattern {
    pub pattern_type: String,
    pub severity: String,
    pub description: String,
    pub affected_request_ids: Vec<String>,
}

impl Pattern {
    fn to_json(&self) -> Value {
        json!({
            "type": self.pattern_type,
            "severity": self.severity,
            "description": self.description,
            "affected_request_ids": self.affected_request_ids,
        })
    }
}

/// Detect problematic patterns in a session.
pub fn detect_patterns(
    requests: &[RequestRecord],
    file_accesses: &[FileAccessRecord],
) -> Vec<Pattern> {
    let mut patterns = Vec::new();

    // Loop detection: same file edited/written 3+ times
    detect_loops(file_accesses, &mut patterns);

    // Error repetition: 2+ consecutive error responses
    detect_error_repeats(requests, &mut patterns);

    // Stall detection: 3+ consecutive requests with 0 file writes/edits
    detect_stalls(requests, file_accesses, &mut patterns);

    patterns
}

fn detect_loops(file_accesses: &[FileAccessRecord], patterns: &mut Vec<Pattern>) {
    let mut write_counts: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();

    for fa in file_accesses {
        if fa.access_type == "write" || fa.access_type == "edit" {
            write_counts
                .entry(fa.file_path.clone())
                .or_default()
                .push(fa.request_id.clone());
        }
    }

    for (path, request_ids) in &write_counts {
        if request_ids.len() >= 3 {
            let unique_ids: Vec<String> = {
                let mut seen = std::collections::HashSet::new();
                request_ids.iter().filter(|id| seen.insert((*id).clone())).cloned().collect()
            };
            patterns.push(Pattern {
                pattern_type: "loop".to_string(),
                severity: "warn".to_string(),
                description: format!(
                    "File '{}' was modified {} times across {} requests",
                    path,
                    request_ids.len(),
                    unique_ids.len()
                ),
                affected_request_ids: unique_ids,
            });
        }
    }
}

fn detect_error_repeats(requests: &[RequestRecord], patterns: &mut Vec<Pattern>) {
    let mut consecutive_errors: Vec<String> = Vec::new();

    for r in requests {
        if r.status == "error" {
            consecutive_errors.push(r.id.clone());
        } else {
            if consecutive_errors.len() >= 2 {
                patterns.push(Pattern {
                    pattern_type: "error_repeat".to_string(),
                    severity: "error".to_string(),
                    description: format!(
                        "{} consecutive error responses detected",
                        consecutive_errors.len()
                    ),
                    affected_request_ids: consecutive_errors.clone(),
                });
            }
            consecutive_errors.clear();
        }
    }

    // Check trailing errors
    if consecutive_errors.len() >= 2 {
        patterns.push(Pattern {
            pattern_type: "error_repeat".to_string(),
            severity: "error".to_string(),
            description: format!(
                "{} consecutive error responses detected",
                consecutive_errors.len()
            ),
            affected_request_ids: consecutive_errors,
        });
    }
}

fn detect_stalls(
    requests: &[RequestRecord],
    file_accesses: &[FileAccessRecord],
    patterns: &mut Vec<Pattern>,
) {
    // Build set of request_ids that have write/edit file accesses
    let write_request_ids: std::collections::HashSet<String> = file_accesses
        .iter()
        .filter(|fa| fa.access_type == "write" || fa.access_type == "edit")
        .map(|fa| fa.request_id.clone())
        .collect();

    let mut consecutive_no_writes: Vec<String> = Vec::new();

    for r in requests {
        if r.status != "complete" {
            continue;
        }
        if write_request_ids.contains(&r.id) {
            if consecutive_no_writes.len() >= 3 {
                patterns.push(Pattern {
                    pattern_type: "stall".to_string(),
                    severity: "info".to_string(),
                    description: format!(
                        "{} consecutive requests with no file modifications",
                        consecutive_no_writes.len()
                    ),
                    affected_request_ids: consecutive_no_writes.clone(),
                });
            }
            consecutive_no_writes.clear();
        } else {
            consecutive_no_writes.push(r.id.clone());
        }
    }

    if consecutive_no_writes.len() >= 3 {
        patterns.push(Pattern {
            pattern_type: "stall".to_string(),
            severity: "info".to_string(),
            description: format!(
                "{} consecutive requests with no file modifications",
                consecutive_no_writes.len()
            ),
            affected_request_ids: consecutive_no_writes,
        });
    }
}

/// Serialize patterns to JSON Value for MCP response.
pub fn patterns_to_json(patterns: &[Pattern]) -> Value {
    let arr: Vec<Value> = patterns.iter().map(|p| p.to_json()).collect();
    json!({
        "pattern_count": arr.len(),
        "patterns": arr,
    })
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::FileAccessRecord;
    use crate::types::RequestRecord;

    fn sample_request(id: &str, status: &str) -> RequestRecord {
        RequestRecord {
            id: id.to_string(),
            session_id: Some("s1".to_string()),
            timestamp: "2024-01-01T00:00:00Z".to_string(),
            method: "POST".to_string(),
            path: "/v1/messages".to_string(),
            request_headers: "{}".to_string(),
            request_body: r#"{"model":"claude-sonnet-4-20250514","messages":[]}"#.to_string(),
            response_status: Some(200),
            response_headers: None,
            response_body: None,
            is_streaming: false,
            input_tokens: Some(10),
            output_tokens: Some(5),
            duration_ms: Some(100),
            status: status.to_string(),
            starred: false,
            memo: String::new(),
            agent_type: "main".to_string(),
            agent_task: String::new(),
            routing_category: String::new(),
            routed_to_url: String::new(),
        }
    }

    fn fa(request_id: &str, path: &str, atype: &str) -> FileAccessRecord {
        FileAccessRecord {
            session_id: "s1".to_string(),
            request_id: request_id.to_string(),
            file_path: path.to_string(),
            access_type: atype.to_string(),
            read_range: String::new(),
            timestamp: "2024-01-01T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn extract_file_accesses_read_tool() {
        let body = r#"{"model":"claude","messages":[
            {"role":"assistant","content":[
                {"type":"tool_use","name":"Read","input":{"file_path":"/src/main.rs"}}
            ]}
        ]}"#;
        let result = extract_file_accesses(body);
        assert_eq!(result, vec![("/src/main.rs".to_string(), "read".to_string(), "full".to_string())]);
    }

    #[test]
    fn extract_file_accesses_write_edit() {
        let body = r#"{"model":"claude","messages":[
            {"role":"assistant","content":[
                {"type":"tool_use","name":"Write","input":{"file_path":"/src/new.rs"}},
                {"type":"tool_use","name":"Edit","input":{"file_path":"/src/db.rs"}}
            ]}
        ]}"#;
        let result = extract_file_accesses(body);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0], ("/src/new.rs".to_string(), "write".to_string(), String::new()));
        assert_eq!(result[1], ("/src/db.rs".to_string(), "edit".to_string(), String::new()));
    }

    #[test]
    fn extract_file_accesses_grep_glob() {
        let body = r#"{"model":"claude","messages":[
            {"role":"assistant","content":[
                {"type":"tool_use","name":"Grep","input":{"path":"/src"}},
                {"type":"tool_use","name":"Glob","input":{"path":"/home/user"}}
            ]}
        ]}"#;
        let result = extract_file_accesses(body);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].1, "search");
        assert_eq!(result[0].2, "");
        assert_eq!(result[1].1, "search");
    }

    #[test]
    fn extract_file_accesses_empty_body() {
        assert!(extract_file_accesses("").is_empty());
        assert!(extract_file_accesses("{}").is_empty());
        assert!(extract_file_accesses("invalid json").is_empty());
    }

    #[test]
    fn extract_file_accesses_ignores_user_messages() {
        let body = r#"{"model":"claude","messages":[
            {"role":"user","content":[
                {"type":"tool_use","name":"Read","input":{"file_path":"/src/main.rs"}}
            ]}
        ]}"#;
        let result = extract_file_accesses(body);
        assert!(result.is_empty());
    }

    #[test]
    fn build_session_summary_basic() {
        let reqs = vec![
            sample_request("r1", "complete"),
            sample_request("r2", "error"),
        ];
        let summary = build_session_summary(&reqs);
        assert_eq!(summary["request_count"], 2);
        assert_eq!(summary["total_input_tokens"], 20);
        assert_eq!(summary["total_output_tokens"], 10);
        assert_eq!(summary["error_count"], 1);
        assert_eq!(summary["requests"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn detect_patterns_loop() {
        let reqs = vec![sample_request("r1", "complete")];
        let accesses = vec![
            fa("r1", "/src/main.rs", "edit"),
            fa("r2", "/src/main.rs", "edit"),
            fa("r3", "/src/main.rs", "edit"),
        ];
        let patterns = detect_patterns(&reqs, &accesses);
        assert_eq!(patterns.len(), 1);
        assert_eq!(patterns[0].pattern_type, "loop");
        assert_eq!(patterns[0].severity, "warn");
    }

    #[test]
    fn detect_patterns_error_repeat() {
        let reqs = vec![
            sample_request("r1", "error"),
            sample_request("r2", "error"),
            sample_request("r3", "complete"),
        ];
        let patterns = detect_patterns(&reqs, &[]);
        assert_eq!(patterns.len(), 1);
        assert_eq!(patterns[0].pattern_type, "error_repeat");
        assert_eq!(patterns[0].severity, "error");
        assert_eq!(patterns[0].affected_request_ids.len(), 2);
    }

    #[test]
    fn detect_patterns_stall() {
        let reqs = vec![
            sample_request("r1", "complete"),
            sample_request("r2", "complete"),
            sample_request("r3", "complete"),
        ];
        // No file accesses → 3 consecutive requests with no writes
        let patterns = detect_patterns(&reqs, &[]);
        assert_eq!(patterns.len(), 1);
        assert_eq!(patterns[0].pattern_type, "stall");
        assert_eq!(patterns[0].severity, "info");
    }

    #[test]
    fn detect_patterns_clean() {
        let reqs = vec![
            sample_request("r1", "complete"),
            sample_request("r2", "complete"),
        ];
        let accesses = vec![
            fa("r1", "/src/a.rs", "edit"),
            fa("r2", "/src/b.rs", "write"),
        ];
        let patterns = detect_patterns(&reqs, &accesses);
        assert!(patterns.is_empty());
    }
}

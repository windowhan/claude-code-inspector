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

// ── Bash command parsing ─────────────────────────────────────────────────────

/// Paths that should be excluded from file access tracking.
const IGNORED_PATHS: &[&str] = &["/dev/null", "/dev/stdout", "/dev/stderr", "/dev/stdin"];

/// Extract file accesses from a Bash tool_use input.
/// Parses the shell command string using shlex for proper tokenization,
/// then splits on shell operators (|, &&, ;, ||) and extracts file operands
/// based on the recognized command.
fn file_accesses_from_bash(input: &Value) -> Vec<(String, String, String)> {
    let command = match input.get("command").and_then(|c| c.as_str()) {
        Some(c) => c,
        None => return Vec::new(),
    };

    // Tokenize the full command with shlex first (handles quotes/escapes correctly)
    let tokens = match shlex::split(command) {
        Some(t) => t,
        None => return Vec::new(), // Unmatched quotes — silently skip
    };

    // Split token list on shell operators to get command segments
    let segments = split_on_shell_operators(&tokens);
    let mut accesses = Vec::new();

    for segment in &segments {
        if segment.is_empty() { continue; }

        // Extract redirections first, then process the command
        let (cmd_tokens, redirections) = extract_redirections(segment);

        // Add write accesses for redirection targets
        for redir_target in &redirections {
            if !redir_target.is_empty() && !IGNORED_PATHS.contains(&redir_target.as_str()) {
                accesses.push((redir_target.clone(), "write".to_string(), String::new()));
            }
        }

        if cmd_tokens.is_empty() { continue; }
        let program = cmd_tokens[0].as_str();
        // Strip path prefix (e.g., /usr/bin/cat -> cat)
        let program = program.rsplit('/').next().unwrap_or(program);
        // Skip prefix commands like sudo, env
        let (program, args) = skip_prefix_commands(program, &cmd_tokens[1..]);

        let file_accesses = extract_files_for_command(program, args);
        for (path, access_type) in file_accesses {
            if !path.is_empty() && !IGNORED_PATHS.contains(&path.as_str()) {
                accesses.push((path, access_type, String::new()));
            }
        }
    }

    accesses
}

/// Split a token list on shell operators: |, &&, ||, ;
fn split_on_shell_operators(tokens: &[String]) -> Vec<Vec<String>> {
    let mut segments: Vec<Vec<String>> = Vec::new();
    let mut current: Vec<String> = Vec::new();

    for token in tokens {
        if token == "|" || token == "&&" || token == "||" || token == ";" {
            if !current.is_empty() {
                segments.push(current);
                current = Vec::new();
            }
        } else {
            current.push(token.clone());
        }
    }
    if !current.is_empty() {
        segments.push(current);
    }
    segments
}

/// Extract redirection targets from a token list.
/// Returns (remaining_command_tokens, redirection_target_paths).
/// Handles both `> file` (space) and `>file` (no space) forms.
fn extract_redirections(tokens: &[String]) -> (Vec<String>, Vec<String>) {
    let mut cmd_tokens = Vec::new();
    let mut redirections = Vec::new();
    let mut skip_next = false;

    for (i, token) in tokens.iter().enumerate() {
        if skip_next {
            skip_next = false;
            continue;
        }
        if token == ">" || token == ">>" {
            // Next token is the target file
            if let Some(next) = tokens.get(i + 1) {
                redirections.push(next.clone());
                skip_next = true;
            }
        } else if let Some(path) = token.strip_prefix(">>") {
            // >>file (no space)
            if !path.is_empty() {
                redirections.push(path.to_string());
            }
        } else if let Some(path) = token.strip_prefix('>') {
            // >file (no space), but skip fd redirections like 2>
            if !path.is_empty() {
                redirections.push(path.to_string());
            }
        } else if token.contains('>') && token.ends_with('>') {
            // e.g., "2>" — next token is target
            if let Some(next) = tokens.get(i + 1) {
                redirections.push(next.clone());
                skip_next = true;
            }
        } else {
            cmd_tokens.push(token.clone());
        }
    }

    (cmd_tokens, redirections)
}

/// Skip prefix commands like sudo, env, nohup and return the real program + args.
fn skip_prefix_commands<'a>(program: &'a str, args: &'a [String]) -> (&'a str, &'a [String]) {
    match program {
        "sudo" | "env" | "nohup" | "nice" | "time" => {
            if let Some(_first) = args.first() {
                // Skip env's KEY=VALUE args
                let mut skip = 0;
                for arg in args {
                    if arg.contains('=') && program == "env" {
                        skip += 1;
                    } else {
                        break;
                    }
                }
                if skip < args.len() {
                    return (args[skip].as_str(), &args[skip + 1..]);
                }
            }
            (program, args)
        }
        _ => (program, args),
    }
}

/// Extract file paths from a recognized command's arguments.
fn extract_files_for_command(program: &str, args: &[String]) -> Vec<(String, String)> {
    match program {
        // Read commands: all non-flag positional args are files
        "cat" | "head" | "tail" | "less" | "more" | "bat" | "wc" => {
            extract_positional_file_args(args, "read")
        }
        // Search commands: grep/rg/ag — first non-flag arg is pattern, rest are paths
        "grep" | "rg" | "ag" => {
            extract_grep_files(args)
        }
        // find: first non-flag arg is the search directory
        "find" => {
            extract_find_directory(args)
        }
        // sed -i: last non-flag arg is the file (write access)
        "sed" => {
            // Only track as write if -i flag is present
            if args.iter().any(|a| a == "-i" || a.starts_with("-i")) {
                extract_last_positional_arg(args, "write")
            } else {
                Vec::new()
            }
        }
        // awk: last non-flag arg is the file (read access)
        "awk" => {
            extract_last_positional_arg(args, "read")
        }
        // tee: all non-flag args are write targets
        "tee" => {
            extract_positional_file_args(args, "write")
        }
        _ => Vec::new(),
    }
}

/// Extract all positional (non-flag) arguments as file paths.
fn extract_positional_file_args(args: &[String], access_type: &str) -> Vec<(String, String)> {
    let mut files = Vec::new();
    let mut skip_next = false;
    for arg in args {
        if skip_next { skip_next = false; continue; }
        if arg.starts_with('-') {
            // Flags with values: -n 10, -c 5, etc. Skip the flag and its value.
            if arg.len() == 2 && arg != "--" {
                skip_next = true;
            }
            continue;
        }
        if arg.starts_with('$') || arg.starts_with('`') { continue; } // Skip variables/subshells
        files.push((arg.clone(), access_type.to_string()));
    }
    files
}

/// Extract file paths from grep/rg/ag arguments.
/// Algorithm: skip flags (tokens starting with -). First non-flag token is the pattern.
/// All subsequent non-flag tokens are file paths (search access).
fn extract_grep_files(args: &[String]) -> Vec<(String, String)> {
    let mut files = Vec::new();
    let mut found_pattern = false;
    let mut skip_next = false;

    // Flags that take a value argument
    let flags_with_value = ["-e", "-f", "-m", "-A", "-B", "-C", "--include", "--exclude",
                            "--include-dir", "--exclude-dir", "--max-count", "--color",
                            "--type", "-t", "-g", "--glob"];

    for arg in args {
        if skip_next { skip_next = false; continue; }
        if arg.starts_with('-') {
            if flags_with_value.iter().any(|f| arg == *f) {
                skip_next = true;
            }
            continue;
        }
        if arg.starts_with('$') || arg.starts_with('`') { continue; }
        if !found_pattern {
            found_pattern = true; // This is the search pattern, skip it
            continue;
        }
        files.push((arg.clone(), "search".to_string()));
    }
    files
}

/// Extract find's search directory (first non-flag argument).
fn extract_find_directory(args: &[String]) -> Vec<(String, String)> {
    // find [path...] [expression...]
    // Path args come before any expression (which start with - or ()
    let mut dirs = Vec::new();
    for arg in args {
        if arg.starts_with('-') || arg == "(" || arg == ")" || arg == "!" {
            break; // Start of expression
        }
        if arg.starts_with('$') || arg.starts_with('`') { continue; }
        dirs.push((arg.clone(), "search".to_string()));
    }
    dirs
}

/// Extract the last positional (non-flag) argument as a file path.
fn extract_last_positional_arg(args: &[String], access_type: &str) -> Vec<(String, String)> {
    let positional: Vec<&String> = args.iter()
        .filter(|a| !a.starts_with('-') && !a.starts_with('$') && !a.starts_with('`'))
        .collect();
    if let Some(last) = positional.last() {
        vec![((*last).clone(), access_type.to_string())]
    } else {
        Vec::new()
    }
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
        if name == "Bash" {
            accesses.extend(file_accesses_from_bash(&input));
        } else if let Some(acc) = file_access_from_tool_use(name, &input) {
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
        if name == "Bash" {
            accesses.extend(file_accesses_from_bash(input));
        } else if let Some(acc) = file_access_from_tool_use(name, input) {
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

            // Bash tool: delegate to bash parser
            if name == "Bash" {
                accesses.extend(file_accesses_from_bash(input));
                continue;
            }

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
            source: "claude_code".to_string(),
            target_host: "api.anthropic.com".to_string(),
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

    // ── Bash file access parsing tests ──────────────────────────────────────

    #[test]
    fn bash_cat_read() {
        let input = serde_json::json!({"command": "cat /path/file.rs", "description": "Read file"});
        let result = file_accesses_from_bash(&input);
        assert_eq!(result, vec![("/path/file.rs".into(), "read".into(), String::new())]);
    }

    #[test]
    fn bash_cat_multiple_files() {
        let input = serde_json::json!({"command": "cat a.rs b.rs"});
        let result = file_accesses_from_bash(&input);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0], ("a.rs".into(), "read".into(), String::new()));
        assert_eq!(result[1], ("b.rs".into(), "read".into(), String::new()));
    }

    #[test]
    fn bash_grep_search() {
        let input = serde_json::json!({"command": "grep -r pattern src/"});
        let result = file_accesses_from_bash(&input);
        assert_eq!(result, vec![("src/".into(), "search".into(), String::new())]);
    }

    #[test]
    fn bash_grep_multiple_files() {
        let input = serde_json::json!({"command": "grep pattern file1.rs file2.rs"});
        let result = file_accesses_from_bash(&input);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0], ("file1.rs".into(), "search".into(), String::new()));
        assert_eq!(result[1], ("file2.rs".into(), "search".into(), String::new()));
    }

    #[test]
    fn bash_sed_write() {
        let input = serde_json::json!({"command": "sed -i 's/foo/bar/' file.rs"});
        let result = file_accesses_from_bash(&input);
        assert_eq!(result, vec![("file.rs".into(), "write".into(), String::new())]);
    }

    #[test]
    fn bash_sed_without_i_no_write() {
        let input = serde_json::json!({"command": "sed 's/foo/bar/' file.rs"});
        let result = file_accesses_from_bash(&input);
        assert!(result.is_empty()); // sed without -i is not a write
    }

    #[test]
    fn bash_redirect_write() {
        let input = serde_json::json!({"command": "echo data > output.txt"});
        let result = file_accesses_from_bash(&input);
        assert_eq!(result, vec![("output.txt".into(), "write".into(), String::new())]);
    }

    #[test]
    fn bash_redirect_no_space() {
        let input = serde_json::json!({"command": "echo data >output.txt"});
        let result = file_accesses_from_bash(&input);
        assert_eq!(result, vec![("output.txt".into(), "write".into(), String::new())]);
    }

    #[test]
    fn bash_append_redirect() {
        let input = serde_json::json!({"command": "echo data >> log.txt"});
        let result = file_accesses_from_bash(&input);
        assert_eq!(result, vec![("log.txt".into(), "write".into(), String::new())]);
    }

    #[test]
    fn bash_pipe_cat_grep() {
        let input = serde_json::json!({"command": "cat a.rs | grep foo"});
        let result = file_accesses_from_bash(&input);
        assert_eq!(result, vec![("a.rs".into(), "read".into(), String::new())]);
    }

    #[test]
    fn bash_quoted_pipe_in_pattern() {
        // grep "foo|bar" should NOT split on | inside quotes
        let input = serde_json::json!({"command": "grep \"foo|bar\" src/main.rs"});
        let result = file_accesses_from_bash(&input);
        assert_eq!(result, vec![("src/main.rs".into(), "search".into(), String::new())]);
    }

    #[test]
    fn bash_and_chain() {
        let input = serde_json::json!({"command": "cd /tmp && cat file.rs"});
        let result = file_accesses_from_bash(&input);
        assert_eq!(result, vec![("file.rs".into(), "read".into(), String::new())]);
    }

    #[test]
    fn bash_semicolon_chain() {
        let input = serde_json::json!({"command": "cat a.rs ; cat b.rs"});
        let result = file_accesses_from_bash(&input);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0], ("a.rs".into(), "read".into(), String::new()));
        assert_eq!(result[1], ("b.rs".into(), "read".into(), String::new()));
    }

    #[test]
    fn bash_head_with_flags() {
        let input = serde_json::json!({"command": "head -n 20 /src/main.rs"});
        let result = file_accesses_from_bash(&input);
        assert_eq!(result, vec![("/src/main.rs".into(), "read".into(), String::new())]);
    }

    #[test]
    fn bash_find_directory() {
        let input = serde_json::json!({"command": "find src/ -name '*.rs'"});
        let result = file_accesses_from_bash(&input);
        assert_eq!(result, vec![("src/".into(), "search".into(), String::new())]);
    }

    #[test]
    fn bash_tee_write() {
        let input = serde_json::json!({"command": "cat input.rs | tee output.rs"});
        let result = file_accesses_from_bash(&input);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0], ("input.rs".into(), "read".into(), String::new()));
        assert_eq!(result[1], ("output.rs".into(), "write".into(), String::new()));
    }

    #[test]
    fn bash_dev_null_filtered() {
        let input = serde_json::json!({"command": "echo data > /dev/null"});
        let result = file_accesses_from_bash(&input);
        assert!(result.is_empty());
    }

    #[test]
    fn bash_sudo_prefix() {
        let input = serde_json::json!({"command": "sudo cat /etc/passwd"});
        let result = file_accesses_from_bash(&input);
        assert_eq!(result, vec![("/etc/passwd".into(), "read".into(), String::new())]);
    }

    #[test]
    fn bash_empty_command() {
        let input = serde_json::json!({"command": ""});
        let result = file_accesses_from_bash(&input);
        assert!(result.is_empty());
    }

    #[test]
    fn bash_no_command_field() {
        let input = serde_json::json!({"description": "something"});
        let result = file_accesses_from_bash(&input);
        assert!(result.is_empty());
    }

    #[test]
    fn bash_unrecognized_command() {
        let input = serde_json::json!({"command": "cargo build --release"});
        let result = file_accesses_from_bash(&input);
        assert!(result.is_empty());
    }

    #[test]
    fn bash_in_extract_file_accesses() {
        // Test the #[cfg(test)] helper integration
        let body = r#"{"model":"claude","messages":[
            {"role":"assistant","content":[
                {"type":"tool_use","name":"Bash","input":{"command":"cat /src/main.rs"}}
            ]}
        ]}"#;
        let result = extract_file_accesses(body);
        assert_eq!(result, vec![("/src/main.rs".into(), "read".into(), String::new())]);
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

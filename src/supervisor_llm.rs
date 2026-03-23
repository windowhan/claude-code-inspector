//! LLM-based session supervisor.
//!
//! Periodically analyzes session requests against user-defined goals and
//! reports findings via Discord webhook (if configured) and the dashboard.
//!
//! All LLM calls and HTTP calls happen with the db mutex RELEASED (scoped-lock pattern).

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use tracing::{info, warn};
use uuid::Uuid;

use crate::db;
use crate::llm::{LlmConfig, call_llm};
use crate::types::{AppState, SupervisorAnalysis};

/// Ambiguity score threshold for goal refinement (0.0 = clear, 1.0 = vague).
const AMBIGUITY_THRESHOLD: f64 = 0.5;

/// Goal refinement result returned by the refine_goal endpoint.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct GoalRefinement {
    pub ambiguity_score: f64,
    pub questions: Vec<String>,
}

/// Compact summary of a single request sent to the LLM (never full request_body).
#[derive(Debug, serde::Serialize)]
struct RequestSummary {
    agent_type: String,
    first_user_message: String,  // truncated to 200 chars
    input_tokens: Option<i64>,
    output_tokens: Option<i64>,
}

/// Main supervisor background loop. Spawned on startup when enabled + api_key set.
/// Reads config at the start of each cycle to pick up interval changes.
pub async fn run_supervisor_loop(state: Arc<AppState>) {
    info!("Supervisor LLM loop started");
    loop {
        // Read config with scoped lock — released before any HTTP call
        let config = {
            let db = state.db.lock().await;
            db::get_supervisor_config(&db).unwrap_or_default()
        };

        // Guard: stop loop if disabled or api_key cleared
        if !config.enabled || config.api_key.is_empty() {
            info!("Supervisor LLM disabled (enabled={}, api_key_set={}), stopping loop",
                config.enabled, !config.api_key.is_empty());
            return;
        }

        // Sleep first (batch-over-real-time: first analysis after one full interval)
        tokio::time::sleep(Duration::from_secs(config.interval_minutes as u64 * 60)).await;

        // Concurrent analysis guard
        if state.supervisor_analyzing.swap(true, Ordering::SeqCst) {
            warn!("Previous supervisor analysis still running, skipping cycle");
            continue;
        }

        if let Err(e) = run_analysis_cycle(&state, &config).await {
            warn!("Supervisor analysis cycle error: {e}");
        }

        state.supervisor_analyzing.store(false, Ordering::SeqCst);
    }
}

/// Run one analysis cycle: process all active sessions with goals.
async fn run_analysis_cycle(state: &Arc<AppState>, config: &db::SupervisorConfig) -> Result<(), String> {
    // Read all active sessions with goals — scoped lock, released before HTTP calls
    let sessions = {
        let db = state.db.lock().await;
        db::get_active_sessions_with_goals(&db)
            .map_err(|e| format!("DB error getting sessions: {e}"))?
    };
    // db lock dropped here

    if sessions.is_empty() {
        return Ok(());
    }

    let llm_config = LlmConfig {
        provider: config.provider.clone(),
        base_url: config.base_url.clone(),
        api_key: config.api_key.clone(),
        model: config.model.clone(),
    };

    for (session_goal, _session_id) in &sessions {
        let sid = &session_goal.session_id;

        // Get watermark — scoped lock
        let watermark = {
            let db = state.db.lock().await;
            db::get_latest_supervisor_analysis(&db, sid)
                .ok()
                .flatten()
                .and_then(|a| a.last_request_id)
        };
        // db lock dropped here

        // Get unanalyzed requests — scoped lock, LIMIT 50
        let requests = {
            let db = state.db.lock().await;
            db::get_unanalyzed_requests(&db, sid, watermark.as_deref())
                .map_err(|e| format!("DB error getting requests: {e}"))?
        };
        // db lock dropped here

        if requests.is_empty() {
            continue;
        }

        let last_request_id = requests.last().map(|r| r.id.clone());

        // Build request summaries (never send full request_body)
        let summaries: Vec<RequestSummary> = requests.iter().map(|r| {
            let first_msg = extract_first_user_message(&r.request_body);
            RequestSummary {
                agent_type: r.agent_type.clone(),
                first_user_message: first_msg,
                input_tokens: r.input_tokens,
                output_tokens: r.output_tokens,
            }
        }).collect();

        // Call LLM — no db lock held during HTTP call
        let goal_text = session_goal.refined_goal.as_deref()
            .unwrap_or(&session_goal.goal);

        let analysis = match analyze_session(&llm_config, goal_text, &summaries, sid, last_request_id).await {
            Ok(a) => a,
            Err(e) => {
                warn!("Supervisor LLM analysis failed for session {sid}: {e}. Watermark NOT advanced.");
                continue;  // Do not advance watermark on failure
            }
        };

        // Save analysis — scoped lock
        {
            let db = state.db.lock().await;
            if let Err(e) = db::insert_supervisor_analysis(&db, &analysis) {
                warn!("Failed to save supervisor analysis for {sid}: {e}");
                continue;
            }
        }
        // db lock dropped here

        // Send Discord if configured — no db lock held
        if !config.discord_webhook_url.is_empty() {
            match send_discord_report(&config.discord_webhook_url, &analysis, goal_text).await {
                Ok(()) => {
                    // Mark as sent — scoped lock
                    let db = state.db.lock().await;
                    let _ = db::mark_analysis_discord_sent(&db, &analysis.id);
                }
                Err(e) => {
                    warn!("Discord webhook failed for {sid}: {e}. Will retry next cycle.");
                }
            }
        }
    }

    Ok(())
}

/// Analyze a session's requests against the goal using LLM.
/// Returns Err without advancing watermark if LLM call fails.
async fn analyze_session(
    config: &LlmConfig,
    goal: &str,
    summaries: &[RequestSummary],
    session_id: &str,
    last_request_id: Option<String>,
) -> Result<SupervisorAnalysis, String> {
    let system_prompt = "You are an AI development supervisor. Analyze the following coding session \
        requests against the user's stated goal. Respond ONLY with valid JSON containing these fields:\n\
        - goal_alignment_score (number 0.0-1.0): how well the work aligns with the goal\n\
        - efficiency_score (number 0.0-1.0): how efficiently tokens/requests are being used\n\
        - intent_execution_match (string): brief assessment of intent vs actual execution\n\
        - token_summary (object): { total_input: number, total_output: number, request_count: number }\n\
        - issues (array of strings): identified issues or concerns, empty array if none\n\
        - recommendation (string): one actionable suggestion for improvement";

    let total_input: i64 = summaries.iter().filter_map(|r| r.input_tokens).sum();
    let total_output: i64 = summaries.iter().filter_map(|r| r.output_tokens).sum();

    let requests_text: String = summaries.iter().enumerate().map(|(i, r)| {
        format!("{}. [{}] tokens_in={} tokens_out={} message=\"{}\"",
            i + 1,
            r.agent_type,
            r.input_tokens.unwrap_or(0),
            r.output_tokens.unwrap_or(0),
            r.first_user_message,
        )
    }).collect::<Vec<_>>().join("\n");

    let user_message = format!(
        "Session goal: {goal}\n\nRecent requests ({} total, {total_input} input tokens, {total_output} output tokens):\n{requests_text}",
        summaries.len()
    );

    let raw_response = call_llm(config, Some(system_prompt), &user_message, 512).await?;

    // Parse LLM JSON response — failure is propagated as Err so watermark is NOT advanced
    let parsed: serde_json::Value = serde_json::from_str(&raw_response)
        .map_err(|e| format!("LLM response JSON parse failed: {e}"))?;

    let goal_alignment_score = parsed.get("goal_alignment_score").and_then(|v| v.as_f64());
    let efficiency_score = parsed.get("efficiency_score").and_then(|v| v.as_f64());
    let intent_execution_match = parsed.get("intent_execution_match")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let token_summary = parsed.get("token_summary")
        .map(|v| v.to_string())
        .or_else(|| Some(format!(r#"{{"total_input":{total_input},"total_output":{total_output},"request_count":{}}}"#, summaries.len())));
    let issues = parsed.get("issues")
        .map(|v| v.to_string())
        .or_else(|| Some("[]".to_string()));
    let recommendation = parsed.get("recommendation")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    Ok(SupervisorAnalysis {
        id: Uuid::new_v4().to_string(),
        session_id: session_id.to_string(),
        analyzed_at: chrono::Utc::now().to_rfc3339(),
        last_request_id,
        goal_alignment_score,
        efficiency_score,
        intent_execution_match,
        token_summary,
        issues,
        recommendation,
        raw_llm_response: Some(raw_response),
        discord_sent: false,
    })
}

/// Send analysis report to Discord webhook.
async fn send_discord_report(webhook_url: &str, analysis: &SupervisorAnalysis, goal: &str) -> Result<(), String> {
    let alignment = analysis.goal_alignment_score
        .map(|s| format!("{:.0}%", s * 100.0))
        .unwrap_or_else(|| "N/A".to_string());
    let efficiency = analysis.efficiency_score
        .map(|s| format!("{:.0}%", s * 100.0))
        .unwrap_or_else(|| "N/A".to_string());
    let recommendation = analysis.recommendation.as_deref().unwrap_or("No recommendation");

    let content = format!(
        "**Supervisor Report**\n**Goal:** {goal}\n**Analysis period:** {}\n**Evaluation**\n- Goal alignment: {alignment}\n- Efficiency: {efficiency}\n**Recommendation:** {recommendation}",
        analysis.analyzed_at
    );

    let body = serde_json::json!({"content": content});
    let client = reqwest::Client::new();
    let resp = client
        .post(webhook_url)
        .header("content-type", "application/json")
        .body(serde_json::to_string(&body).map_err(|e| e.to_string())?)
        .send()
        .await
        .map_err(|e| format!("Discord request failed: {e}"))?;

    if resp.status().is_success() {
        Ok(())
    } else {
        Err(format!("Discord returned HTTP {}", resp.status()))
    }
}

/// Analyze a goal text for ambiguity and return clarifying questions if needed.
pub async fn refine_goal(config: &LlmConfig, goal_text: &str) -> Result<GoalRefinement, String> {
    let system_prompt = "You are a software project clarity assistant. \
        Analyze the given development goal for ambiguity. \
        Respond ONLY with valid JSON: { \"ambiguity_score\": <number 0.0-1.0>, \"questions\": [<string>, ...] }. \
        ambiguity_score: 0.0 means completely clear, 1.0 means very vague. \
        questions: if ambiguity_score > 0.5, provide 1-2 clarifying questions; otherwise empty array.";

    let user_message = format!("Development goal: {goal_text}");

    let raw = call_llm(config, Some(system_prompt), &user_message, 256).await?;

    let parsed: serde_json::Value = serde_json::from_str(&raw)
        .map_err(|e| format!("Goal refinement JSON parse failed: {e}"))?;

    let ambiguity_score = parsed.get("ambiguity_score")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.5)
        .clamp(0.0, 1.0);

    let questions = if ambiguity_score > AMBIGUITY_THRESHOLD {
        parsed.get("questions")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter()
                .filter_map(|q| q.as_str())
                .map(|s| s.to_string())
                .collect())
            .unwrap_or_default()
    } else {
        Vec::new()
    };

    Ok(GoalRefinement { ambiguity_score, questions })
}

/// Extract the first user text message from a request body JSON (truncated to 200 chars).
fn extract_first_user_message(request_body: &str) -> String {
    let val: serde_json::Value = serde_json::from_str(request_body).unwrap_or_default();
    let messages = val.get("messages").and_then(|m| m.as_array());
    if let Some(msgs) = messages {
        for msg in msgs {
            if msg.get("role").and_then(|r| r.as_str()) == Some("user") {
                let content = msg.get("content");
                let text = match content {
                    Some(serde_json::Value::String(s)) => s.clone(),
                    Some(serde_json::Value::Array(arr)) => {
                        arr.iter()
                            .find(|c| c.get("type").and_then(|t| t.as_str()) == Some("text"))
                            .and_then(|c| c.get("text"))
                            .and_then(|t| t.as_str())
                            .unwrap_or("")
                            .to_string()
                    }
                    _ => String::new(),
                };
                if !text.is_empty() {
                    let truncated = text.chars().take(200).collect::<String>();
                    return truncated;
                }
            }
        }
    }
    String::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_first_user_message_string_content() {
        let body = r#"{"messages":[{"role":"user","content":"Refactor the auth module"}]}"#;
        assert_eq!(extract_first_user_message(body), "Refactor the auth module");
    }

    #[test]
    fn extract_first_user_message_array_content() {
        let body = r#"{"messages":[{"role":"user","content":[{"type":"text","text":"Fix the bug in proxy.rs"}]}]}"#;
        assert_eq!(extract_first_user_message(body), "Fix the bug in proxy.rs");
    }

    #[test]
    fn extract_first_user_message_truncates_at_200() {
        let long = "x".repeat(300);
        let body = format!(r#"{{"messages":[{{"role":"user","content":"{long}"}}]}}"#);
        let result = extract_first_user_message(&body);
        assert_eq!(result.len(), 200);
    }

    #[test]
    fn extract_first_user_message_empty_on_invalid() {
        assert_eq!(extract_first_user_message("not json"), "");
        assert_eq!(extract_first_user_message("{}"), "");
    }

    #[test]
    fn goal_refinement_serialize() {
        let r = GoalRefinement {
            ambiguity_score: 0.7,
            questions: vec!["Which module?".to_string()],
        };
        let json = serde_json::to_string(&r).unwrap();
        assert!(json.contains("0.7"));
        assert!(json.contains("Which module?"));
    }
}

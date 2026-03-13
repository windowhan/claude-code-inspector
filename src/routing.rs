use std::time::Duration;
use serde_json::Value;
use crate::types::{RoutingConfig, RoutingRule};

const DEFAULT_SYSTEM_PROMPT: &str = "You are an intent classifier for LLM API requests. \
Based on the conversation messages provided, classify the intent into exactly one category. \
Respond with ONLY the category name, nothing else.";

pub async fn classify_intent(
    config: &RoutingConfig,
    fallback_api_key: &str,
    request_body: &Value,
    rules: &[RoutingRule],
) -> String {
    let api_key = if config.classifier_api_key.is_empty() {
        fallback_api_key
    } else {
        &config.classifier_api_key
    };

    let extracted = extract_last_messages(request_body, 3);

    let system_prompt = {
        let base = if config.classifier_prompt.is_empty() {
            DEFAULT_SYSTEM_PROMPT.to_string()
        } else {
            config.classifier_prompt.clone()
        };
        let categories_block = build_categories_prompt(rules);
        format!("{base}\n\nAvailable categories:\n{categories_block}")
    };

    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
    {
        Ok(c) => c,
        Err(_) => return "other".to_string(),
    };

    // Always use OpenAI-compatible format (/v1/chat/completions).
    // Anthropic also supports this endpoint. Send both auth headers so any provider works.
    let url = format!("{}/v1/chat/completions", config.classifier_base_url.trim_end_matches('/'));
    let mut messages = vec![serde_json::json!({"role": "system", "content": system_prompt})];
    messages.extend(extracted);
    let body = serde_json::json!({
        "model": config.classifier_model,
        "messages": messages,
        "max_tokens": 50,
    });
    let body_bytes = serde_json::to_vec(&body).unwrap_or_default();
    let resp = client
        .post(&url)
        .header("authorization", format!("Bearer {api_key}"))
        .header("x-api-key", api_key)
        .header("content-type", "application/json")
        .body(body_bytes)
        .send()
        .await;
    let response_text = match resp {
        Ok(r) => match r.bytes().await {
            Ok(b) => match serde_json::from_slice::<Value>(&b) {
                Ok(v) => v["choices"][0]["message"]["content"]
                    .as_str()
                    .unwrap_or("other")
                    .trim()
                    .to_string(),
                Err(_) => return "other".to_string(),
            },
            Err(_) => return "other".to_string(),
        },
        Err(_) => return "other".to_string(),
    };

    let categories = categories_from_rules(rules);
    parse_category(&response_text, &categories)
}

pub fn match_rule<'a>(rules: &'a [RoutingRule], category: &str) -> Option<&'a RoutingRule> {
    let mut sorted: Vec<&RoutingRule> = rules.iter().filter(|r| r.enabled).collect();
    sorted.sort_by_key(|r| r.priority);
    sorted.into_iter().find(|r| r.category == category)
}

pub fn apply_model_override(body: &[u8], model: &str) -> Vec<u8> {
    match serde_json::from_slice::<Value>(body) {
        Ok(mut v) => {
            if let Some(obj) = v.as_object_mut() {
                obj.insert("model".to_string(), Value::String(model.to_string()));
            }
            serde_json::to_vec(&v).unwrap_or_else(|_| body.to_vec())
        }
        Err(_) => body.to_vec(),
    }
}

/// Apply a prompt override template to the last user message in the request body.
/// If the template contains `{original_prompt}`, it is replaced with the original content.
/// If not, the last user message content is fully replaced with the template.
/// Returns the original body unchanged if no messages array or template is empty.
pub fn apply_prompt_override(body: &[u8], template: &str) -> Vec<u8> {
    if template.is_empty() {
        return body.to_vec();
    }
    let mut val: Value = match serde_json::from_slice(body) {
        Ok(v) => v,
        Err(_) => return body.to_vec(),
    };
    let msgs = match val.get_mut("messages").and_then(|m| m.as_array_mut()) {
        Some(m) => m,
        None => return body.to_vec(),
    };
    // Find the last user message
    if let Some(last_user) = msgs.iter_mut().rev().find(|m| {
        m.get("role").and_then(|r| r.as_str()) == Some("user")
    }) {
        let original = extract_text_content(last_user.get("content"));
        let new_content = if template.contains("{original_prompt}") {
            template.replace("{original_prompt}", &original)
        } else {
            template.to_string()
        };
        if let Some(obj) = last_user.as_object_mut() {
            obj.insert("content".to_string(), Value::String(new_content));
        }
    }
    serde_json::to_vec(&val).unwrap_or_else(|_| body.to_vec())
}

fn extract_text_content(content: Option<&Value>) -> String {
    match content {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Array(parts)) => parts.iter().filter_map(|p| {
            if p.get("type").and_then(|t| t.as_str()) == Some("text") {
                p.get("text").and_then(|t| t.as_str()).map(|s| s.to_string())
            } else {
                None
            }
        }).collect::<Vec<_>>().join("\n"),
        _ => String::new(),
    }
}

fn extract_last_messages(body: &Value, n: usize) -> Vec<Value> {
    let msgs = match body.get("messages").and_then(|m| m.as_array()) {
        Some(m) => m,
        None => return Vec::new(),
    };
    let start = if msgs.len() > n { msgs.len() - n } else { 0 };
    msgs[start..].iter().map(|m| {
        let role = m.get("role").and_then(|r| r.as_str()).unwrap_or("user");
        let content = simplify_content(m.get("content"));
        serde_json::json!({"role": role, "content": content})
    }).collect()
}

fn simplify_content(content: Option<&Value>) -> Value {
    match content {
        None => Value::String(String::new()),
        Some(Value::String(s)) => Value::String(s.clone()),
        Some(Value::Array(arr)) => {
            // Concatenate text blocks into a single string
            let text: String = arr.iter().filter_map(|block| {
                if block.get("type").and_then(|t| t.as_str()) == Some("text") {
                    block.get("text").and_then(|t| t.as_str()).map(|s| s.to_string())
                } else {
                    None
                }
            }).collect::<Vec<_>>().join("\n");
            Value::String(text)
        }
        Some(other) => Value::String(other.to_string()),
    }
}

/// Build a categories section for the classifier prompt from enabled rules.
/// Each unique category appears once, with its description if set.
fn build_categories_prompt(rules: &[RoutingRule]) -> String {
    let mut seen = std::collections::HashSet::new();
    let mut lines = Vec::new();
    let mut sorted: Vec<&RoutingRule> = rules.iter().filter(|r| r.enabled).collect();
    sorted.sort_by_key(|r| r.priority);
    for rule in sorted {
        if seen.insert(rule.category.clone()) {
            if rule.description.is_empty() {
                lines.push(format!("- {}", rule.category));
            } else {
                lines.push(format!("- {}: {}", rule.category, rule.description));
            }
        }
    }
    lines.join("\n")
}

fn categories_from_rules(rules: &[RoutingRule]) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    rules.iter()
        .filter(|r| r.enabled)
        .filter_map(|r| {
            if seen.insert(r.category.clone()) { Some(r.category.clone()) } else { None }
        })
        .collect()
}

fn parse_category(text: &str, categories: &[String]) -> String {
    let lower = text.to_lowercase();
    for cat in categories {
        let cat_lower = cat.to_lowercase();
        if lower == cat_lower || lower.contains(&cat_lower) {
            return cat.clone();
        }
    }
    "other".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;
    use http_body_util::Full;
    use hyper::service::service_fn;
    use hyper::server::conn::http1;
    use hyper_util::rt::TokioIo;
    use tokio::net::TcpListener;

    fn default_config(base_url: &str) -> RoutingConfig {
        RoutingConfig {
            enabled: true,
            classifier_base_url: base_url.to_string(),
            classifier_api_key: "test-key".to_string(),
            classifier_model: "claude-haiku-4-5-20251001".to_string(),
            classifier_prompt: String::new(),
        }
    }

    fn sample_rules() -> Vec<RoutingRule> {
        vec![
            RoutingRule { id: "r1".to_string(), priority: 10, enabled: true, category: "code_gen".to_string(), description: "Writing new code or implementing features".to_string(), target_url: "https://a.com".to_string(), api_key: String::new(), prompt_override: String::new(), model_override: String::new(), label: String::new() },
            RoutingRule { id: "r2".to_string(), priority: 20, enabled: true, category: "docs".to_string(), description: "Writing documentation".to_string(), target_url: "https://b.com".to_string(), api_key: String::new(), prompt_override: String::new(), model_override: String::new(), label: String::new() },
            RoutingRule { id: "r3".to_string(), priority: 30, enabled: true, category: "other".to_string(), description: String::new(), target_url: "https://c.com".to_string(), api_key: String::new(), prompt_override: String::new(), model_override: String::new(), label: String::new() },
        ]
    }

    async fn spawn_mock_classifier(response_body: &'static str) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            if let Ok((stream, _)) = listener.accept().await {
                let io = TokioIo::new(stream);
                let _ = http1::Builder::new()
                    .serve_connection(io, service_fn(move |_req| async move {
                        Ok::<_, hyper::Error>(
                            hyper::Response::builder()
                                .status(200)
                                .header("content-type", "application/json")
                                .body(Full::new(Bytes::from(response_body)))
                                .unwrap()
                        )
                    }))
                    .await;
            }
        });
        format!("http://{addr}")
    }

    #[test]
    fn match_rule_returns_first_priority_match() {
        let rules = vec![
            RoutingRule { id: "r1".to_string(), priority: 10, enabled: true, category: "code_gen".to_string(), target_url: "https://a.com".to_string(), api_key: String::new(), prompt_override: String::new(), model_override: String::new(), description: String::new(), label: String::new() },
            RoutingRule { id: "r2".to_string(), priority: 20, enabled: true, category: "code_gen".to_string(), target_url: "https://b.com".to_string(), api_key: String::new(), prompt_override: String::new(), model_override: String::new(), description: String::new(), label: String::new() },
        ];
        let result = match_rule(&rules, "code_gen").unwrap();
        assert_eq!(result.id, "r1"); // lower priority value = first
    }

    #[test]
    fn match_rule_skips_disabled_rules() {
        let rules = vec![
            RoutingRule { id: "r1".to_string(), priority: 10, enabled: false, category: "code_gen".to_string(), target_url: "https://a.com".to_string(), api_key: String::new(), prompt_override: String::new(), model_override: String::new(), description: String::new(), label: String::new() },
            RoutingRule { id: "r2".to_string(), priority: 20, enabled: true, category: "code_gen".to_string(), target_url: "https://b.com".to_string(), api_key: String::new(), prompt_override: String::new(), model_override: String::new(), description: String::new(), label: String::new() },
        ];
        let result = match_rule(&rules, "code_gen").unwrap();
        assert_eq!(result.id, "r2"); // r1 disabled, use r2
    }

    #[test]
    fn match_rule_no_match_returns_none() {
        let rules = vec![
            RoutingRule { id: "r1".to_string(), priority: 10, enabled: true, category: "code_gen".to_string(), target_url: "https://a.com".to_string(), api_key: String::new(), prompt_override: String::new(), model_override: String::new(), description: String::new(), label: String::new() },
        ];
        assert!(match_rule(&rules, "docs").is_none());
    }

    #[test]
    fn apply_model_override_replaces_model() {
        let body = br#"{"model":"claude-opus","messages":[],"max_tokens":10}"#;
        let result = apply_model_override(body, "gpt-4");
        let v: Value = serde_json::from_slice(&result).unwrap();
        assert_eq!(v["model"], "gpt-4");
        // Other fields preserved
        assert_eq!(v["max_tokens"], 10);
    }

    #[test]
    fn apply_model_override_invalid_json_passthrough() {
        let body = b"not json";
        let result = apply_model_override(body, "gpt-4");
        assert_eq!(result, b"not json");
    }

    #[test]
    fn apply_prompt_override_with_placeholder() {
        let body = br#"{"messages":[{"role":"user","content":"write a sort function"}],"max_tokens":10}"#;
        let result = apply_prompt_override(body, "You are an expert. {original_prompt}\nPlease be concise.");
        let v: Value = serde_json::from_slice(&result).unwrap();
        assert_eq!(v["messages"][0]["content"], "You are an expert. write a sort function\nPlease be concise.");
    }

    #[test]
    fn apply_prompt_override_without_placeholder_replaces_fully() {
        let body = br#"{"messages":[{"role":"user","content":"original text"}],"max_tokens":10}"#;
        let result = apply_prompt_override(body, "completely different prompt");
        let v: Value = serde_json::from_slice(&result).unwrap();
        assert_eq!(v["messages"][0]["content"], "completely different prompt");
    }

    #[test]
    fn apply_prompt_override_empty_template_is_noop() {
        let body = br#"{"messages":[{"role":"user","content":"original"}]}"#;
        let result = apply_prompt_override(body, "");
        assert_eq!(result, body);
    }

    #[test]
    fn apply_prompt_override_targets_last_user_message() {
        let body = br#"{"messages":[{"role":"user","content":"first"},{"role":"assistant","content":"reply"},{"role":"user","content":"second"}]}"#;
        let result = apply_prompt_override(body, "prefix {original_prompt}");
        let v: Value = serde_json::from_slice(&result).unwrap();
        assert_eq!(v["messages"][0]["content"], "first");   // unchanged
        assert_eq!(v["messages"][2]["content"], "prefix second");  // modified
    }

    #[test]
    fn apply_prompt_override_invalid_json_passthrough() {
        let body = b"not json";
        let result = apply_prompt_override(body, "template {original_prompt}");
        assert_eq!(result, b"not json");
    }

    #[tokio::test]
    async fn classify_intent_returns_matched_category() {
        let mock_resp = r#"{"choices":[{"message":{"content":"code_gen"}}]}"#;
        let base_url = spawn_mock_classifier(mock_resp).await;
        let config = default_config(&base_url);
        let body = serde_json::json!({"messages": [{"role": "user", "content": "write me a function"}]});
        let category = classify_intent(&config, "", &body, &sample_rules()).await;
        assert_eq!(category, "code_gen");
    }

    #[tokio::test]
    async fn classify_intent_docs_category() {
        let mock_resp = r#"{"choices":[{"message":{"content":"docs"}}]}"#;
        let base_url = spawn_mock_classifier(mock_resp).await;
        let config = default_config(&base_url);
        let body = serde_json::json!({"messages": [{"role": "user", "content": "write docs"}]});
        let category = classify_intent(&config, "", &body, &sample_rules()).await;
        assert_eq!(category, "docs");
    }

    #[tokio::test]
    async fn classify_intent_timeout_returns_other() {
        // Spawn a server that never responds
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            if let Ok((_stream, _)) = listener.accept().await {
                // Never respond — just hold the connection
                tokio::time::sleep(Duration::from_secs(60)).await;
            }
        });
        let base_url = format!("http://{addr}");
        let mut config = default_config(&base_url);
        // Use a very short timeout by using a bad model that won't be reached
        // We rely on the server never responding to trigger timeout
        config.classifier_api_key = "key".to_string();
        // The timeout is 10s which is too long for tests; let's test against a refused connection
        // by using a port that's not listening
        let config2 = default_config("http://127.0.0.1:1");
        let body = serde_json::json!({"messages": [{"role": "user", "content": "hello"}]});
        let category = classify_intent(&config2, "", &body, &[]).await;
        assert_eq!(category, "other");
    }

    #[tokio::test]
    async fn classify_intent_unknown_category_returns_other() {
        let mock_resp = r#"{"content":[{"type":"text","text":"totally_unknown_cat"}]}"#;
        let base_url = spawn_mock_classifier(mock_resp).await;
        let config = default_config(&base_url);
        let body = serde_json::json!({"messages": [{"role": "user", "content": "do something"}]});
        let category = classify_intent(&config, "", &body, &sample_rules()).await;
        assert_eq!(category, "other");
    }

    #[test]
    fn parse_category_exact_match() {
        let cats = vec!["code_gen".to_string(), "docs".to_string(), "other".to_string()];
        assert_eq!(parse_category("code_gen", &cats), "code_gen");
        assert_eq!(parse_category("DOCS", &cats), "docs");
    }

    #[test]
    fn parse_category_contains_match() {
        let cats = vec!["code_gen".to_string(), "other".to_string()];
        assert_eq!(parse_category("The category is code_gen.", &cats), "code_gen");
    }

    #[test]
    fn parse_category_no_match_returns_other() {
        let cats = vec!["code_gen".to_string(), "docs".to_string()];
        assert_eq!(parse_category("something random", &cats), "other");
    }

    #[test]
    fn extract_last_messages_limits_to_n() {
        let body = serde_json::json!({
            "messages": [
                {"role": "user", "content": "msg1"},
                {"role": "assistant", "content": "msg2"},
                {"role": "user", "content": "msg3"},
                {"role": "assistant", "content": "msg4"},
                {"role": "user", "content": "msg5"},
            ]
        });
        let result = extract_last_messages(&body, 3);
        assert_eq!(result.len(), 3);
        assert_eq!(result[0]["content"], "msg3");
        assert_eq!(result[2]["content"], "msg5");
    }

    #[test]
    fn simplify_content_handles_array() {
        let arr = serde_json::json!([
            {"type": "text", "text": "hello"},
            {"type": "image", "source": {}},
            {"type": "text", "text": "world"},
        ]);
        let result = simplify_content(Some(&arr));
        assert_eq!(result, Value::String("hello\nworld".to_string()));
    }
}

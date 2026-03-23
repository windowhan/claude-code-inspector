//! Shared LLM call helper used by both the summarizer and supervisor.

use reqwest;

pub struct LlmConfig {
    pub provider: String,
    pub base_url: String,
    pub api_key: String,
    pub model: String,
}

/// Call an LLM and return the text response.
/// Supports "anthropic" and OpenAI-compatible providers ("openai", "deepseek", "kimi").
/// On error, returns an Err with a descriptive message.
pub async fn call_llm(
    config: &LlmConfig,
    system_prompt: Option<&str>,
    user_message: &str,
    max_tokens: u32,
) -> Result<String, String> {
    let client = reqwest::Client::new();
    let is_anthropic = config.provider == "anthropic";

    let (url, api_body, auth_header_name, auth_header_value) = if is_anthropic {
        let messages = vec![serde_json::json!({"role": "user", "content": user_message})];
        let mut body = serde_json::json!({
            "model": config.model,
            "max_tokens": max_tokens,
            "messages": messages,
        });
        if let Some(sp) = system_prompt {
            body["system"] = serde_json::Value::String(sp.to_string());
        }
        (
            format!("{}/v1/messages", config.base_url),
            body,
            "x-api-key".to_string(),
            config.api_key.clone(),
        )
    } else {
        let mut messages = Vec::new();
        if let Some(sp) = system_prompt {
            messages.push(serde_json::json!({"role": "system", "content": sp}));
        }
        messages.push(serde_json::json!({"role": "user", "content": user_message}));
        (
            format!("{}/v1/chat/completions", config.base_url),
            serde_json::json!({
                "model": config.model,
                "max_tokens": max_tokens,
                "messages": messages,
            }),
            "authorization".to_string(),
            format!("Bearer {}", config.api_key),
        )
    };

    let mut req_builder = client
        .post(&url)
        .header("content-type", "application/json")
        .header(&auth_header_name, &auth_header_value);

    if is_anthropic {
        req_builder = req_builder.header("anthropic-version", "2023-06-01");
    }

    let resp = req_builder
        .body(serde_json::to_string(&api_body).map_err(|e| e.to_string())?)
        .send()
        .await
        .map_err(|e| format!("LLM request failed: {e}"))?;

    let resp_text = resp.text().await
        .map_err(|e| format!("LLM response read failed: {e}"))?;

    let resp_json: serde_json::Value = serde_json::from_str(&resp_text)
        .map_err(|e| format!("LLM response parse failed: {e} — raw: {}", &resp_text[..resp_text.len().min(200)]))?;

    if is_anthropic {
        resp_json.get("content")
            .and_then(|c| c.as_array())
            .and_then(|a| a.first())
            .and_then(|b| b.get("text"))
            .and_then(|t| t.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| format!("Anthropic response missing content: {}", &resp_text[..resp_text.len().min(200)]))
    } else {
        resp_json.get("choices")
            .and_then(|c| c.as_array())
            .and_then(|a| a.first())
            .and_then(|c| c.get("message"))
            .and_then(|m| m.get("content"))
            .and_then(|t| t.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| format!("OpenAI response missing content: {}", &resp_text[..resp_text.len().min(200)]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn llm_config_can_be_constructed() {
        let config = LlmConfig {
            provider: "anthropic".to_string(),
            base_url: "https://api.anthropic.com".to_string(),
            api_key: "sk-test".to_string(),
            model: "claude-haiku-4-5-20251001".to_string(),
        };
        assert_eq!(config.provider, "anthropic");
        assert_eq!(config.model, "claude-haiku-4-5-20251001");
    }
}

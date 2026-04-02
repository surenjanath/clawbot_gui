use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use futures_util::StreamExt;
use reqwest::blocking::Client;
use serde_json::{json, Value};

use super::types::ChatMessage;

/// Expand `{{date}}`, `{{time}}`, `{{os}}` in system prompt templates.
pub fn expand_system_prompt(template: &str) -> String {
    let now = chrono_now_string();
    let time = chrono_time_string();
    let os = std::env::consts::OS;
    template
        .replace("{{date}}", &now)
        .replace("{{time}}", &time)
        .replace("{{os}}", os)
}

fn chrono_now_string() -> String {
    // Avoid chrono dependency: use simple RFC3339-ish from SystemTime
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(d) => format!("{}.{:03}Z", d.as_secs(), d.subsec_millis()),
        Err(_) => "unknown".to_string(),
    }
}

fn chrono_time_string() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let h = (now / 3600) % 24;
    let m = (now / 60) % 60;
    let s = now % 60;
    format!("{h:02}:{m:02}:{s:02}")
}

/// Build OpenAI-style message array from UI history (no system here).
/// Skips welcome assistant bubble (id 0, assistant). Optionally drops trailing empty assistant.
pub fn build_api_history(
    messages: &[ChatMessage],
    exclude_last_empty_assistant: bool,
    max_pairs: usize,
    max_chars: usize,
) -> Vec<Value> {
    let mut list: Vec<ChatMessage> = messages
        .iter()
        .filter(|m| m.role == "user" || m.role == "assistant")
        .filter(|m| !(m.id == 0 && m.role == "assistant"))
        .cloned()
        .collect();

    if exclude_last_empty_assistant {
        if let Some(last) = list.last() {
            if last.role == "assistant" && last.content.trim().is_empty() {
                list.pop();
            }
        }
    }

    // Truncate by pairs from the front
    if max_pairs > 0 {
        let max_msgs = max_pairs.saturating_mul(2);
        while list.len() > max_msgs {
            list.remove(0);
        }
    }

    if max_chars > 0 {
        while !list.is_empty() && count_history_chars(&list) > max_chars {
            list.remove(0);
        }
    }

    list.into_iter()
        .filter(|m| !m.content.trim().is_empty())
        .map(|m| json!({ "role": m.role, "content": m.content }))
        .collect()
}

fn count_history_chars(messages: &[ChatMessage]) -> usize {
    messages.iter().map(|m| m.content.len()).sum()
}

pub fn ollama_tool_definitions() -> Vec<Value> {
    vec![
        json!({
            "type": "function",
            "function": {
                "name": "get_current_time",
                "description": "Returns the current wall-clock time (UTC-based, best effort).",
                "parameters": { "type": "object", "properties": {}, "additionalProperties": false }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "word_count",
                "description": "Counts whitespace-separated words in the given text.",
                "parameters": {
                    "type": "object",
                    "properties": { "text": { "type": "string", "description": "Text to count" } },
                    "required": ["text"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "math_add",
                "description": "Adds two integers.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "a": { "type": "integer" },
                        "b": { "type": "integer" }
                    },
                    "required": ["a", "b"]
                }
            }
        }),
    ]
}

fn format_time_rough_utc() -> String {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(d) => format!(
            "UTC instant: {}s {}ns since Unix epoch",
            d.as_secs(),
            d.subsec_nanos()
        ),
        Err(_) => "time unavailable".to_string(),
    }
}

fn json_int(args: &Value, key: &str) -> Option<i64> {
    args.get(key).and_then(|x| {
        x.as_i64()
            .or_else(|| x.as_f64().map(|f| f as i64))
    })
}

pub fn run_builtin_tool(name: &str, arguments: &str) -> Result<String, String> {
    let args: Value = serde_json::from_str(arguments).unwrap_or_else(|_| json!({}));
    match name {
        "get_current_time" => Ok(format_time_rough_utc()),
        "word_count" => {
            let text = args
                .get("text")
                .and_then(|t| t.as_str())
                .unwrap_or("");
            let n = text.split_whitespace().count();
            Ok(format!("Word count: {n}"))
        }
        "math_add" => {
            let a = json_int(&args, "a").ok_or_else(|| "missing or invalid integer field 'a'".to_string())?;
            let b = json_int(&args, "b").ok_or_else(|| "missing or invalid integer field 'b'".to_string())?;
            Ok(format!("{}", a + b))
        }
        _ => Err(format!("unknown tool '{name}'")),
    }
}

pub fn ollama_chat_completion(
    client: &Client,
    base: &str,
    model: &str,
    messages: &[Value],
    temperature: f32,
    max_tokens: u32,
    tools: Option<&[Value]>,
) -> Result<Value, String> {
    let url = format!("{}/v1/chat/completions", base.trim_end_matches('/'));
    let mut body = json!({
        "model": model,
        "messages": messages,
        "temperature": temperature,
        "max_tokens": max_tokens,
        "stream": false
    });
    if let Some(t) = tools {
        body.as_object_mut()
            .expect("body object")
            .insert("tools".to_string(), json!(t));
        body.as_object_mut()
            .expect("body object")
            .insert("tool_choice".to_string(), json!("auto"));
    }
    let resp = client
        .post(&url)
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .map_err(|e| format!("request failed: {e}"))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let txt = resp.text().unwrap_or_default();
        return Err(format!("HTTP {status}: {txt}"));
    }
    let json_val: Value = resp
        .json()
        .map_err(|e| format!("invalid JSON: {e}"))?;
    json_val
        .pointer("/choices/0/message")
        .cloned()
        .ok_or_else(|| "missing choices[0].message".to_string())
}

pub fn probe_ollama_tool_support(
    client: &Client,
    base: &str,
    model: &str,
    temperature: f32,
    max_tokens: u32,
) -> (bool, String, String) {
    let tools = ollama_tool_definitions();
    let probe_messages = vec![
        json!({"role": "system", "content": "You are a test harness. When asked, you must call the provided function get_current_time using the tools API. Do not answer with plain text only."}),
        json!({"role": "user", "content": "Call the get_current_time tool now. Use tool_calls."}),
    ];
    match ollama_chat_completion(
        client,
        base,
        model,
        &probe_messages,
        temperature,
        max_tokens.min(512),
        Some(&tools),
    ) {
        Ok(msg) => {
            let tool_calls = msg.get("tool_calls").and_then(|t| t.as_array());
            let has_tools = tool_calls.map_or(false, |a| !a.is_empty());
            let raw = msg
                .get("tool_calls")
                .map(|v| v.to_string())
                .unwrap_or_else(|| "null".to_string());
            let content = msg
                .get("content")
                .and_then(|c| c.as_str())
                .unwrap_or("")
                .trim();
            let summary = if has_tools {
                format!(
                    "This model returned {} tool call(s). Tool calling is likely supported.",
                    tool_calls.map(|a| a.len()).unwrap_or(0)
                )
            } else if !content.is_empty() {
                "Model replied with text only (no tool_calls). It may not follow OpenAI-style tools, or the prompt was ignored.".to_string()
            } else {
                "Empty assistant message (no tool_calls, no content). Tool calling may be unsupported or misconfigured.".to_string()
            };
            (has_tools, summary, raw)
        }
        Err(e) => (
            false,
            format!("Probe request failed: {e}"),
            String::new(),
        ),
    }
}

pub fn run_chat_with_optional_tools(
    client: &Client,
    base: &str,
    model: &str,
    system: &str,
    history: &[Value],
    temperature: f32,
    max_tokens: u32,
    enable_tools: bool,
) -> Result<String, String> {
    let tools = ollama_tool_definitions();
    let mut messages: Vec<Value> = Vec::new();
    messages.push(json!({"role": "system", "content": system}));
    messages.extend_from_slice(history);

    let mut trace = String::new();
    const MAX_TOOL_ROUNDS: u32 = 8;

    for _ in 0..MAX_TOOL_ROUNDS {
        let tool_slice = if enable_tools {
            Some(tools.as_slice())
        } else {
            None
        };
        let msg = ollama_chat_completion(
            client,
            base,
            model,
            &messages,
            temperature,
            max_tokens,
            tool_slice,
        )?;

        let tool_calls = msg.get("tool_calls").and_then(|t| t.as_array()).cloned();
        let content = msg
            .get("content")
            .and_then(|c| c.as_str())
            .unwrap_or("")
            .to_string();

        if let Some(calls) = tool_calls.filter(|c| !c.is_empty()) {
            if !enable_tools {
                return Err(
                    "Model emitted tool_calls but tools are disabled in settings.".to_string(),
                );
            }
            let content_val = if content.is_empty() {
                Value::Null
            } else {
                json!(content)
            };
            messages.push(json!({
                "role": "assistant",
                "content": content_val,
                "tool_calls": calls.clone(),
            }));

            for call in &calls {
                let id = call
                    .get("id")
                    .and_then(|i| i.as_str())
                    .unwrap_or("call");
                let func = call.get("function");
                let name = func
                    .and_then(|f| f.get("name"))
                    .and_then(|n| n.as_str())
                    .unwrap_or("");
                let args = func
                    .and_then(|f| f.get("arguments"))
                    .and_then(|a| a.as_str())
                    .unwrap_or("{}");
                let result = run_builtin_tool(name, args);
                match &result {
                    Ok(out) => {
                        trace.push_str(&format!("\n[tool {name}] → {out}\n"));
                    }
                    Err(e) => {
                        trace.push_str(&format!("\n[tool {name} ERROR] {e}\n"));
                    }
                }
                let tool_content = result.unwrap_or_else(|e| format!("ERROR: {e}"));
                messages.push(json!({
                    "role": "tool",
                    "tool_call_id": id,
                    "content": tool_content
                }));
            }
            continue;
        }

        if !content.is_empty() {
            let mut out = trace;
            out.push_str(&content);
            return Ok(out);
        }

        return Err(
            "Assistant message had no content and no tool_calls (unexpected).".to_string(),
        );
    }

    Err(format!(
        "Stopped after {MAX_TOOL_ROUNDS} tool rounds (possible loop)."
    ))
}

/// OpenAI-compat SSE: extract incremental assistant text from `data: {...}` lines.
///
/// Handles plain `delta.content` strings and array-shaped content (e.g. `{ "text": "..." }` parts).
fn openai_delta_content(delta: &Value) -> Option<String> {
    let c = delta.get("content")?;
    match c {
        Value::Null => None,
        Value::String(s) => {
            if s.is_empty() {
                None
            } else {
                Some(s.clone())
            }
        }
        Value::Array(parts) => {
            let mut out = String::new();
            for p in parts {
                if let Some(s) = p.as_str() {
                    out.push_str(s);
                } else if let Some(t) = p.get("text").and_then(|x| x.as_str()) {
                    out.push_str(t);
                }
            }
            if out.is_empty() {
                None
            } else {
                Some(out)
            }
        }
        _ => None,
    }
}

pub fn parse_sse_data_json_line(payload: &str) -> Option<String> {
    let t = payload.trim();
    if t.is_empty() || t == "[DONE]" {
        return None;
    }
    let v: Value = serde_json::from_str(t).ok()?;
    if let Some(delta) = v.pointer("/choices/0/delta") {
        if let Some(s) = openai_delta_content(delta) {
            return Some(s);
        }
    }
    None
}

/// Run streaming chat completion; sends text deltas via callback. Returns Ok(()) or error string.
pub fn run_ollama_stream_blocking(
    base: &str,
    model: &str,
    system: &str,
    history: &[Value],
    temperature: f32,
    max_tokens: u32,
    cancel: Arc<AtomicBool>,
    mut on_delta: impl FnMut(String),
) -> Result<(), String> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| format!("tokio runtime: {e}"))?;

    rt.block_on(async move {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(180))
            .build()
            .map_err(|e| format!("client: {e}"))?;

        let url = format!("{}/v1/chat/completions", base.trim_end_matches('/'));
        let mut messages = vec![json!({"role": "system", "content": system})];
        messages.extend_from_slice(history);

        let body = json!({
            "model": model,
            "messages": messages,
            "temperature": temperature,
            "max_tokens": max_tokens,
            "stream": true
        });

        let resp = client
            .post(&url)
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("request: {e}"))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let txt = resp.text().await.unwrap_or_default();
            return Err(format!("HTTP {status}: {txt}"));
        }

        let mut stream = resp.bytes_stream();
        let mut buf: Vec<u8> = Vec::new();

        while let Some(chunk) = stream.next().await {
            if cancel.load(Ordering::SeqCst) {
                return Ok(());
            }
            let chunk = chunk.map_err(|e| format!("stream: {e}"))?;
            buf.extend_from_slice(&chunk);

            while let Some(frame) = pop_sse_frame(&mut buf) {
                let frame_str = String::from_utf8_lossy(&frame);
                for line in frame_str.lines() {
                    let line = line.trim_end();
                    if let Some(data) = line.strip_prefix("data:") {
                        let data = data.trim_start();
                        if data == "[DONE]" {
                            return Ok(());
                        }
                        if let Some(s) = parse_sse_data_json_line(data) {
                            if !s.is_empty() {
                                on_delta(s);
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    })
}

fn pop_sse_frame(buf: &mut Vec<u8>) -> Option<Vec<u8>> {
    if let Some(pos) = buf.windows(2).position(|w| w == b"\n\n") {
        let frame = buf[..pos].to_vec();
        buf.drain(..pos + 2);
        return Some(frame);
    }
    if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
        let frame = buf[..pos].to_vec();
        buf.drain(..pos + 4);
        return Some(frame);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_history_skips_welcome_and_truncates_pairs() {
        let msgs = vec![
            ChatMessage::assistant("welcome".to_string(), 0),
            ChatMessage::user("a".to_string(), 1),
            ChatMessage::assistant("A".to_string(), 2),
            ChatMessage::user("b".to_string(), 3),
            ChatMessage::assistant("B".to_string(), 4),
        ];
        let h = build_api_history(&msgs, false, 1, 0);
        assert_eq!(h.len(), 2);
        assert_eq!(h[0]["content"], "b");
        assert_eq!(h[1]["content"], "B");
    }

    #[test]
    fn expand_template_replaces_placeholders() {
        let s = expand_system_prompt("x {{date}} {{os}}");
        assert!(s.contains("x "));
        assert!(s.contains(std::env::consts::OS));
    }

    #[test]
    fn parse_sse_delta_string_and_array_content() {
        let s = r#"{"choices":[{"delta":{"content":"hi"}}]}"#;
        assert_eq!(parse_sse_data_json_line(s).as_deref(), Some("hi"));

        let arr = r#"{"choices":[{"delta":{"content":[{"text":"a"},{"text":"b"}]}}]}"#;
        assert_eq!(parse_sse_data_json_line(arr).as_deref(), Some("ab"));
    }
}

//! Model allowlist, price table and cost estimation. Money is integer micro-USD.
use crate::{api, ApiError};
use serde_json::Value;

pub struct Model {
    pub vendor: &'static str,
    pub name: &'static str,
    /// micro-USD per million tokens
    pub in_price: i64,
    pub out_price: i64,
    pub default_max: i64,
    pub max_cap: i64,
}

// PLACEHOLDER PRICES: verify against the vendors' pricing pages before any real deployment.
const fn m(vendor: &'static str, name: &'static str, i: i64, o: i64) -> Model {
    Model { vendor, name, in_price: i, out_price: o, default_max: 1024, max_cap: 4096 }
}
pub static MODELS: &[Model] = &[
    m("openai", "gpt-4o", 2_500_000, 10_000_000),
    m("openai", "gpt-4o-mini", 150_000, 600_000),
    m("openai", "gpt-4.1", 2_000_000, 8_000_000),
    m("openai", "gpt-4.1-mini", 400_000, 1_600_000),
    m("anthropic", "claude-opus-5-5", 15_000_000, 75_000_000),
    m("anthropic", "claude-sonnet-5-5", 3_000_000, 15_000_000),
    m("anthropic", "claude-haiku-5-5", 1_000_000, 5_000_000),
];

pub fn lookup(vendor: &str, name: &str) -> Option<&'static Model> {
    MODELS.iter().find(|m| m.vendor == vendor && m.name == name)
}

/// ceil(tokens * price / 1e6)
pub fn cost(tokens: i64, price: i64) -> i64 {
    ((tokens.max(0) as i128 * price as i128 + 999_999) / 1_000_000) as i64
}

pub struct Checked {
    pub model: &'static Model,
    pub estimate_micro: i64,
}

fn unsupported() -> ApiError {
    api(400, "unsupported_content", "only plain text messages are supported (no images, audio, files or tools)")
}

/// Text bytes of a content value (string or array of `{type:"text"}` parts).
fn content_bytes(c: &Value) -> Result<usize, ApiError> {
    match c {
        Value::String(s) => Ok(s.len()),
        Value::Array(parts) => parts.iter().try_fold(0, |n, p| match (p.get("type").and_then(Value::as_str), p.get("text").and_then(Value::as_str)) {
            (Some("text"), Some(t)) => Ok(n + t.len()),
            _ => Err(unsupported()),
        }),
        _ => Err(unsupported()),
    }
}

/// Validates the body, injects the default output cap if missing, and computes the reservation:
/// (bytes/2 + 8 per message) * input price + max_tokens * output price.
pub fn validate(vendor: &str, body: &mut Value) -> Result<Checked, ApiError> {
    let obj = body.as_object_mut().ok_or_else(|| api(400, "invalid_request", "body must be a JSON object"))?;
    if !matches!(obj.get("stream"), None | Some(Value::Null) | Some(Value::Bool(false))) {
        return Err(api(400, "streaming_unsupported", "stream:true is not supported in v1"));
    }
    let name = obj.get("model").and_then(Value::as_str).unwrap_or("");
    let model = lookup(vendor, name).ok_or_else(|| api(400, "model_not_allowed", format!("model '{name}' is not allowed for {vendor}")))?;
    if let Some(n) = obj.get("n") {
        if n.as_i64() != Some(1) {
            return Err(api(400, "unsupported_option", "n > 1 is not supported"));
        }
    }
    for k in ["tools", "functions", "tool_choice", "function_call", "web_search_options", "audio", "modalities", "mcp_servers"] {
        if obj.contains_key(k) {
            return Err(unsupported());
        }
    }
    // output cap
    let keys: &[&str] = if vendor == "openai" { &["max_tokens", "max_completion_tokens"] } else { &["max_tokens"] };
    let mut max_tokens: Option<i64> = None;
    for k in keys {
        match obj.get(*k) {
            None | Some(Value::Null) => {}
            Some(v) => match v.as_i64() {
                Some(t) if t >= 1 && t <= model.max_cap => max_tokens = Some(max_tokens.unwrap_or(0).max(t)),
                Some(t) if t > model.max_cap => {
                    return Err(api(400, "max_tokens_too_large", format!("{k} {t} exceeds the cap of {} for {}", model.max_cap, model.name)))
                }
                _ => return Err(api(400, "invalid_request", format!("{k} must be a positive integer"))),
            },
        }
    }
    let max_tokens = match max_tokens {
        Some(t) => t,
        None => {
            obj.insert("max_tokens".into(), model.default_max.into());
            model.default_max
        }
    };
    // text size
    let msgs = obj.get("messages").and_then(Value::as_array).filter(|a| !a.is_empty()).ok_or_else(|| api(400, "invalid_request", "messages must be a non-empty array"))?;
    let mut bytes = 0usize;
    for msg in msgs {
        bytes += content_bytes(msg.get("content").unwrap_or(&Value::Null))?;
    }
    if vendor == "anthropic" {
        if let Some(sys) = obj.get("system") {
            bytes += content_bytes(sys)?;
        }
    }
    let in_tokens = (bytes / 2) as i64 + 8 * msgs.len() as i64;
    Ok(Checked { model, estimate_micro: cost(in_tokens, model.in_price) + cost(max_tokens, model.out_price) })
}

/// Actual cost from the vendor `usage` block; None when usage is missing.
pub fn actual_cost(vendor: &str, model: &Model, resp: &Value) -> Option<i64> {
    let u = resp.get("usage")?;
    let (i, o) = if vendor == "openai" { ("prompt_tokens", "completion_tokens") } else { ("input_tokens", "output_tokens") };
    Some(cost(u.get(i)?.as_i64()?, model.in_price) + cost(u.get(o)?.as_i64()?, model.out_price))
}

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
    /// operator has checked these prices against the vendor pricing page
    pub verified: bool,
}

// PLACEHOLDER PRICES: verify against the vendors' pricing pages before any real deployment.
const fn m(vendor: &'static str, name: &'static str, i: i64, o: i64) -> Model {
    Model { vendor, name, in_price: i, out_price: o, default_max: 1024, max_cap: 4096, verified: false }
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
fn unsupported_opt(k: &str) -> ApiError {
    api(400, "unsupported_option", format!("'{k}' is not supported"))
}

const OPENAI_KEYS: &[&str] = &[
    "model", "messages", "max_tokens", "max_completion_tokens", "temperature", "top_p", "stop", "presence_penalty",
    "frequency_penalty", "seed", "user", "response_format", "n", "stream",
];
const ANTHROPIC_KEYS: &[&str] = &["model", "messages", "max_tokens", "system", "temperature", "top_p", "top_k", "stop_sequences", "metadata", "stream"];

/// Text bytes of a content value: a string, or an array of `{type:"text", text}` parts (nothing else).
fn content_bytes(c: &Value) -> Result<usize, ApiError> {
    match c {
        Value::String(s) => Ok(s.len()),
        Value::Array(parts) => parts.iter().try_fold(0, |n, p| {
            let o = p.as_object().ok_or_else(unsupported)?;
            if o.get("type").and_then(Value::as_str) != Some("text") {
                return Err(unsupported());
            }
            if let Some(k) = o.keys().find(|k| !matches!(k.as_str(), "type" | "text")) {
                return Err(unsupported_opt(k));
            }
            Ok(n + o.get("text").and_then(Value::as_str).ok_or_else(|| api(400, "invalid_request", "text part needs a string 'text'"))?.len())
        }),
        _ => Err(unsupported()),
    }
}

fn type_check(obj: &serde_json::Map<String, Value>) -> Result<(), ApiError> {
    for (k, v) in obj {
        let ok = match k.as_str() {
            "temperature" | "top_p" | "top_k" | "presence_penalty" | "frequency_penalty" => v.is_number(),
            "seed" => v.is_i64() || v.is_u64(),
            "user" => v.is_string(),
            "metadata" => v.is_object(),
            "stop" => v.is_string() || v.as_array().is_some_and(|a| a.iter().all(Value::is_string)),
            "stop_sequences" => v.as_array().is_some_and(|a| a.iter().all(Value::is_string)),
            _ => true,
        };
        if !ok {
            return Err(api(400, "invalid_request", format!("'{k}' has the wrong type")));
        }
    }
    Ok(())
}

/// Validates the body against explicit per-vendor allowlists, injects the default output cap if
/// missing, and computes the reservation CEILING: input tokens <= len(normalized request JSON)
/// plus 16 per message plus 64 (1 token per byte is a safe upper bound for BPE tokenizers), output = max_tokens.
/// `allow_unpriced`: mock mode or operator asserted prices (MK_PRICES_VERIFIED=1).
pub fn validate(vendor: &str, body: &mut Value, allow_unpriced: bool) -> Result<Checked, ApiError> {
    let obj = body.as_object_mut().ok_or_else(|| api(400, "invalid_request", "body must be a JSON object"))?;
    if !matches!(obj.get("stream"), None | Some(Value::Null) | Some(Value::Bool(false))) {
        return Err(api(400, "streaming_unsupported", "stream:true is not supported in v1"));
    }
    let allowed = if vendor == "openai" { OPENAI_KEYS } else { ANTHROPIC_KEYS };
    if let Some(k) = obj.keys().find(|k| !allowed.contains(&k.as_str())) {
        return Err(unsupported_opt(k));
    }
    type_check(obj)?;
    let name = obj.get("model").and_then(Value::as_str).unwrap_or("");
    let model = lookup(vendor, name).ok_or_else(|| api(400, "model_not_allowed", format!("model '{name}' is not allowed for {vendor}")))?;
    if !model.verified && !allow_unpriced {
        return Err(api(400, "model_not_priced", format!("prices for '{name}' are not verified on this server (operator: check pricing.rs, then set MK_PRICES_VERIFIED=1)")));
    }
    if obj.get("n").is_some_and(|n| n.as_i64() != Some(1)) {
        return Err(unsupported_opt("n"));
    }
    if let Some(rf) = obj.get("response_format") {
        let o = rf.as_object().ok_or_else(|| unsupported_opt("response_format"))?;
        if let Some(k) = o.keys().find(|k| *k != "type") {
            return Err(unsupported_opt(&format!("response_format.{k}")));
        }
        if !matches!(o.get("type").and_then(Value::as_str), Some("text" | "json_object")) {
            return Err(unsupported_opt("response_format.type"));
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
    // messages
    let roles: &[&str] = if vendor == "openai" { &["user", "assistant", "system", "developer"] } else { &["user", "assistant"] };
    let msg_keys: &[&str] = if vendor == "openai" { &["role", "content", "name"] } else { &["role", "content"] };
    let msgs = obj.get("messages").and_then(Value::as_array).filter(|a| !a.is_empty()).ok_or_else(|| api(400, "invalid_request", "messages must be a non-empty array"))?;
    for msg in msgs {
        let m = msg.as_object().ok_or_else(|| api(400, "invalid_request", "each message must be an object"))?;
        if let Some(k) = m.keys().find(|k| !msg_keys.contains(&k.as_str())) {
            return Err(unsupported_opt(&format!("messages[].{k}")));
        }
        if !m.get("role").and_then(Value::as_str).is_some_and(|r| roles.contains(&r)) {
            return Err(api(400, "invalid_request", format!("message role must be one of {roles:?}")));
        }
        if m.get("name").is_some_and(|n| !n.is_string()) {
            return Err(api(400, "invalid_request", "name must be a string"));
        }
        content_bytes(m.get("content").unwrap_or(&Value::Null))?; // validation only
    }
    if let Some(sys) = obj.get("system") {
        content_bytes(sys)?; // validation only; the system prompt is inside the JSON measured below
    }
    let n_msgs = msgs.len() as i64;
    let json_bytes = serde_json::to_string(&*body).map(|s| s.len()).unwrap_or(usize::MAX / 4) as i64; // exactly what is forwarded
    let in_tokens = json_bytes + 16 * n_msgs + 64;
    Ok(Checked { model, estimate_micro: cost(in_tokens, model.in_price) + cost(max_tokens, model.out_price) })
}

/// Actual cost from the vendor `usage` block; None when usage is missing.
pub fn actual_cost(vendor: &str, model: &Model, resp: &Value) -> Option<i64> {
    let u = resp.get("usage")?;
    let (i, o) = if vendor == "openai" { ("prompt_tokens", "completion_tokens") } else { ("input_tokens", "output_tokens") };
    Some(cost(u.get(i)?.as_i64()?, model.in_price) + cost(u.get(o)?.as_i64()?, model.out_price))
}

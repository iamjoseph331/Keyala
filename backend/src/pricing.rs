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
    /// prompts with more than LONG_PROMPT input tokens bill at these rates instead (0 = no tier)
    pub long_in: i64,
    pub long_out: i64,
    /// prices checked against the vendor pricing page; unverified models need KEYALA_PRICES_VERIFIED=1
    pub verified: bool,
}

pub const LONG_PROMPT: i64 = 100_000;

impl Model {
    /// (input, output) micro-USD per million tokens for a prompt of `in_tokens`.
    pub fn rates(&self, in_tokens: i64) -> (i64, i64) {
        if self.long_in > 0 && in_tokens > LONG_PROMPT {
            (self.long_in, self.long_out)
        } else {
            (self.in_price, self.out_price)
        }
    }
}

// Verified 2026-11-15 (standard tier, USD per 1M tokens, no cache/batch discounts):
//   https://developers.openai.com/api/docs/pricing
//   https://platform.claude.com/docs/en/about-claude/pricing
// OpenRouter entries verified 2026-11-18 against https://openrouter.ai/api/v1/models (same per-token
// prices as the providers). Only exact slugs: fallbacks (`models`), `provider` routing and plugins are rejected.
// When adding a model, leave `verified: false` until its price is checked the same way.
const fn m(vendor: &'static str, name: &'static str, i: i64, o: i64) -> Model {
    Model {
        vendor,
        name,
        in_price: i,
        out_price: o,
        default_max: 1024,
        max_cap: 4096,
        long_in: 0,
        long_out: 0,
        verified: true,
    }
}
pub static MODELS: &[Model] = &[
    m("openai", "gpt-4o", 2_500_000, 10_000_000),
    m("openai", "gpt-4o-mini", 150_000, 600_000),
    m("openai", "gpt-4.1", 2_000_000, 8_000_000),
    m("openai", "gpt-4.1-mini", 400_000, 1_600_000),
    m("anthropic", "claude-opus-5-5", 4_000_000, 20_000_000),
    m("anthropic", "claude-sonnet-5-5", 2_000_000, 10_000_000),
    // Haiku 5.5 bills prompts over 100K tokens at $0.50 / $2.50.
    Model {
        long_in: 500_000,
        long_out: 2_500_000,
        ..m("anthropic", "claude-haiku-5-5", 100_000, 500_000)
    },
    m("openrouter", "openai/gpt-4o", 2_500_000, 10_000_000),
    m("openrouter", "openai/gpt-4o-mini", 150_000, 600_000),
    m("openrouter", "openai/gpt-4.1", 2_000_000, 8_000_000),
    m("openrouter", "openai/gpt-4.1-mini", 400_000, 1_600_000),
    m(
        "openrouter",
        "anthropic/claude-opus-5.5",
        4_000_000,
        20_000_000,
    ),
    m(
        "openrouter",
        "anthropic/claude-sonnet-5.5",
        2_000_000,
        10_000_000,
    ),
    // long-prompt tier kept from Anthropic's pricing: conservative if OpenRouter passes it through
    Model {
        long_in: 500_000,
        long_out: 2_500_000,
        ..m("openrouter", "anthropic/claude-haiku-5.5", 100_000, 500_000)
    },
];

/// OpenRouter speaks the OpenAI chat-completions shape.
pub fn openai_shape(vendor: &str) -> bool {
    vendor != "anthropic"
}

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
    /// the ceilings the estimate was built from, so a routed request can be re-priced with the OpenRouter model's rates
    pub in_tokens: i64,
    pub max_tokens: i64,
}

impl Checked {
    /// Same token ceilings, another model's rates (used for OpenRouter-routed requests).
    pub fn reprice(&self, m: &Model) -> i64 {
        let (ip, op) = m.rates(self.in_tokens);
        cost(self.in_tokens, ip) + cost(self.max_tokens, op)
    }
}

/// Static, verified mapping from a direct-vendor model to its exact OpenRouter slug. No match = no routing
/// (fail closed): nothing is guessed, and OpenRouter's own fallback/`models` features are never used.
pub fn or_slug(vendor: &str, name: &str) -> Option<&'static str> {
    Some(match (vendor, name) {
        ("openai", "gpt-4o") => "openai/gpt-4o",
        ("openai", "gpt-4o-mini") => "openai/gpt-4o-mini",
        ("openai", "gpt-4.1") => "openai/gpt-4.1",
        ("openai", "gpt-4.1-mini") => "openai/gpt-4.1-mini",
        ("anthropic", "claude-opus-5-5") => "anthropic/claude-opus-5.5",
        ("anthropic", "claude-sonnet-5-5") => "anthropic/claude-sonnet-5.5",
        ("anthropic", "claude-haiku-5-5") => "anthropic/claude-haiku-5.5",
        _ => return None,
    })
}

/// Extra limits when a direct-protocol request is forwarded through OpenRouter. We have not verified that
/// OpenRouter handles these options identically, so they are rejected instead of assumed to work.
pub fn check_routable(vendor: &str, body: &Value) -> Result<(), ApiError> {
    let obj = body
        .as_object()
        .ok_or_else(|| api(400, "invalid_request", "body must be a JSON object"))?;
    let banned: &[&str] = if vendor == "anthropic" {
        &["metadata", "top_k"]
    } else {
        &["max_completion_tokens", "n"]
    };
    if let Some(k) = banned.iter().find(|k| obj.contains_key(**k)) {
        return Err(api(
            400,
            "unsupported_option",
            format!("'{k}' is not supported when this app is routed through OpenRouter"),
        ));
    }
    let developer = obj
        .get("messages")
        .and_then(Value::as_array)
        .is_some_and(|a| a.iter().any(|m| m["role"] == "developer"));
    if developer {
        return Err(api(
            400,
            "unsupported_option",
            "role 'developer' is not supported when this app is routed through OpenRouter",
        ));
    }
    Ok(())
}

fn unsupported() -> ApiError {
    api(
        400,
        "unsupported_content",
        "only plain text messages are supported (no images, audio, files or tools)",
    )
}
fn unsupported_opt(k: &str) -> ApiError {
    api(400, "unsupported_option", format!("'{k}' is not supported"))
}

const OPENAI_KEYS: &[&str] = &[
    "model",
    "messages",
    "max_tokens",
    "max_completion_tokens",
    "temperature",
    "top_p",
    "stop",
    "presence_penalty",
    "frequency_penalty",
    "seed",
    "user",
    "response_format",
    "n",
    "stream",
];
const ANTHROPIC_KEYS: &[&str] = &[
    "model",
    "messages",
    "max_tokens",
    "system",
    "temperature",
    "top_p",
    "top_k",
    "stop_sequences",
    "metadata",
    "stream",
];

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
            Ok(n + o
                .get("text")
                .and_then(Value::as_str)
                .ok_or_else(|| api(400, "invalid_request", "text part needs a string 'text'"))?
                .len())
        }),
        _ => Err(unsupported()),
    }
}

fn type_check(obj: &serde_json::Map<String, Value>) -> Result<(), ApiError> {
    for (k, v) in obj {
        let ok = match k.as_str() {
            "temperature" | "top_p" | "top_k" | "presence_penalty" | "frequency_penalty" => {
                v.is_number()
            }
            "seed" => v.is_i64() || v.is_u64(),
            "user" => v.is_string(),
            "metadata" => v.is_object(),
            "stop" => v.is_string() || v.as_array().is_some_and(|a| a.iter().all(Value::is_string)),
            "stop_sequences" => v.as_array().is_some_and(|a| a.iter().all(Value::is_string)),
            _ => true,
        };
        if !ok {
            return Err(api(
                400,
                "invalid_request",
                format!("'{k}' has the wrong type"),
            ));
        }
    }
    Ok(())
}

/// Validates the body against explicit per-vendor allowlists, injects the default output cap if
/// missing, and computes the reservation CEILING: input tokens <= len(normalized request JSON)
/// plus 16 per message plus 64 (1 token per byte is a safe upper bound for BPE tokenizers), output = max_tokens.
/// `allow_unpriced`: mock mode or operator asserted prices (KEYALA_PRICES_VERIFIED=1).
pub fn validate(vendor: &str, body: &mut Value, allow_unpriced: bool) -> Result<Checked, ApiError> {
    let obj = body
        .as_object_mut()
        .ok_or_else(|| api(400, "invalid_request", "body must be a JSON object"))?;
    if !matches!(
        obj.get("stream"),
        None | Some(Value::Null) | Some(Value::Bool(false))
    ) {
        return Err(api(
            400,
            "streaming_unsupported",
            "stream:true is not supported in v1",
        ));
    }
    let allowed = if openai_shape(vendor) {
        OPENAI_KEYS
    } else {
        ANTHROPIC_KEYS
    };
    if let Some(k) = obj.keys().find(|k| !allowed.contains(&k.as_str())) {
        return Err(unsupported_opt(k));
    }
    type_check(obj)?;
    let name = obj.get("model").and_then(Value::as_str).unwrap_or("");
    let model = lookup(vendor, name).ok_or_else(|| {
        api(
            400,
            "model_not_allowed",
            format!("model '{name}' is not allowed for {vendor}"),
        )
    })?;
    if !model.verified && !allow_unpriced {
        return Err(api(400, "model_not_priced", format!("prices for '{name}' are not verified on this server (operator: check pricing.rs, then set KEYALA_PRICES_VERIFIED=1)")));
    }
    if obj.get("n").is_some_and(|n| n.as_i64() != Some(1)) {
        return Err(unsupported_opt("n"));
    }
    if let Some(rf) = obj.get("response_format") {
        let o = rf
            .as_object()
            .ok_or_else(|| unsupported_opt("response_format"))?;
        if let Some(k) = o.keys().find(|k| *k != "type") {
            return Err(unsupported_opt(&format!("response_format.{k}")));
        }
        if !matches!(
            o.get("type").and_then(Value::as_str),
            Some("text" | "json_object")
        ) {
            return Err(unsupported_opt("response_format.type"));
        }
    }
    // output cap
    let keys: &[&str] = if openai_shape(vendor) {
        &["max_tokens", "max_completion_tokens"]
    } else {
        &["max_tokens"]
    };
    let mut max_tokens: Option<i64> = None;
    for k in keys {
        match obj.get(*k) {
            None | Some(Value::Null) => {}
            Some(v) => match v.as_i64() {
                Some(t) if t >= 1 && t <= model.max_cap => {
                    max_tokens = Some(max_tokens.unwrap_or(0).max(t))
                }
                Some(t) if t > model.max_cap => {
                    return Err(api(
                        400,
                        "max_tokens_too_large",
                        format!(
                            "{k} {t} exceeds the cap of {} for {}",
                            model.max_cap, model.name
                        ),
                    ))
                }
                _ => {
                    return Err(api(
                        400,
                        "invalid_request",
                        format!("{k} must be a positive integer"),
                    ))
                }
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
    let roles: &[&str] = if openai_shape(vendor) {
        &["user", "assistant", "system", "developer"]
    } else {
        &["user", "assistant"]
    };
    let msg_keys: &[&str] = if openai_shape(vendor) {
        &["role", "content", "name"]
    } else {
        &["role", "content"]
    };
    let msgs = obj
        .get("messages")
        .and_then(Value::as_array)
        .filter(|a| !a.is_empty())
        .ok_or_else(|| api(400, "invalid_request", "messages must be a non-empty array"))?;
    for msg in msgs {
        let m = msg
            .as_object()
            .ok_or_else(|| api(400, "invalid_request", "each message must be an object"))?;
        if let Some(k) = m.keys().find(|k| !msg_keys.contains(&k.as_str())) {
            return Err(unsupported_opt(&format!("messages[].{k}")));
        }
        if !m
            .get("role")
            .and_then(Value::as_str)
            .is_some_and(|r| roles.contains(&r))
        {
            return Err(api(
                400,
                "invalid_request",
                format!("message role must be one of {roles:?}"),
            ));
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
    let json_bytes = serde_json::to_string(&*body)
        .map(|s| s.len())
        .unwrap_or(usize::MAX / 4) as i64; // exactly what is forwarded
    let in_tokens = json_bytes + 16 * n_msgs + 64;
    // the estimate overcounts input tokens, so picking the tier from it is also conservative.
    let (ip, op) = model.rates(in_tokens);
    Ok(Checked {
        model,
        estimate_micro: cost(in_tokens, ip) + cost(max_tokens, op),
        in_tokens,
        max_tokens,
    })
}

/// Actual cost from the vendor `usage` block; None when usage is missing.
pub fn actual_cost(vendor: &str, model: &Model, resp: &Value) -> Option<i64> {
    let u = resp.get("usage")?;
    let (i, o) = if openai_shape(vendor) {
        ("prompt_tokens", "completion_tokens")
    } else {
        ("input_tokens", "output_tokens")
    };
    let (it, ot) = (u.get(i)?.as_i64()?, u.get(o)?.as_i64()?);
    let (ip, op) = model.rates(it);
    Some(cost(it, ip) + cost(ot, op))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every direct model must map to an existing, verified OpenRouter entry; anything else maps to nothing (fail closed).
    #[test]
    fn every_direct_model_has_a_verified_openrouter_mapping_and_unknowns_have_none() {
        for m in MODELS.iter().filter(|m| m.vendor != "openrouter") {
            let slug = or_slug(m.vendor, m.name)
                .unwrap_or_else(|| panic!("{} {} has no mapping", m.vendor, m.name));
            let om = lookup("openrouter", slug)
                .unwrap_or_else(|| panic!("{slug} missing from the OpenRouter table"));
            assert!(om.verified && om.max_cap >= m.max_cap);
        }
        assert_eq!(or_slug("openai", "gpt-unknown"), None);
        assert_eq!(or_slug("anthropic", "gpt-4o"), None);
        assert_eq!(
            or_slug("openrouter", "openai/gpt-4o"),
            None,
            "OpenRouter apps are never re-routed"
        );
    }
}

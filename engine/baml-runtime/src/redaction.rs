use std::collections::HashMap;

/// Tools for redacting sensitive information from requests.
///
/// General rules:
/// If a value is equal to any env var value, replace it with $THE_ENV_VAR.
/// If a value appears to be sensitive (because it's key was similar to "api_key"),
/// replace it with $REDACTED.
use serde_json::Value as JsonValue;

fn is_sensitive_key(key: &str) -> bool {
    let k = key.to_ascii_lowercase();
    matches!(
        k.as_str(),
        "api_key"
            | "apikey"
            | "api-key"
            | "x-api-key"
            | "x_api_key"
            | "authorization"
            | "access_token"
            | "secret"
            | "token"
    )
}

fn is_env_placeholder(s: &str, env_vars: &HashMap<String, String>) -> bool {
    if let Some(name) = s.strip_prefix('$') {
        return env_vars.contains_key(name);
    }
    if let Some(rest) = s.strip_prefix("Bearer ") {
        if let Some(name) = rest.strip_prefix('$') {
            return env_vars.contains_key(name);
        }
    }
    false
}

fn redact_by_env(value: &str, env_vars: &HashMap<String, String>) -> Option<String> {
    // If already a placeholder, keep as-is
    if is_env_placeholder(value, env_vars) {
        return Some(value.to_string());
    }
    for (k, v) in env_vars {
        if value == v {
            return Some(format!("${k}"));
        }
        if let Some(stripped) = value.strip_prefix("Bearer ") {
            if stripped == v {
                return Some(format!("Bearer ${k}"));
            }
        }
    }
    None
}

pub fn scrub_header_value(
    key: &str,
    value: &str,
    env_vars: &HashMap<String, String>,
    expose_secrets: bool,
) -> String {
    if expose_secrets {
        return value.to_string();
    }
    if let Some(redacted) = redact_by_env(value, env_vars) {
        return redacted;
    }
    if is_sensitive_key(key) {
        return "$REDACTED".to_string();
    }
    value.to_string()
}

pub fn scrub_json_value(
    value: &JsonValue,
    env_vars: &HashMap<String, String>,
    expose_secrets: bool,
    parent_key: Option<&str>,
) -> JsonValue {
    if expose_secrets {
        return value.clone();
    }
    match value {
        JsonValue::String(s) => {
            if let Some(r) = redact_by_env(s, env_vars) {
                JsonValue::String(r)
            } else if parent_key.is_some_and(is_sensitive_key) {
                JsonValue::String("$REDACTED".to_string())
            } else {
                value.clone()
            }
        }
        JsonValue::Array(arr) => JsonValue::Array(
            arr.iter()
                .map(|v| scrub_json_value(v, env_vars, expose_secrets, parent_key))
                .collect(),
        ),
        JsonValue::Object(map) => {
            let mut out = serde_json::Map::new();
            for (k, v) in map.iter() {
                out.insert(
                    k.clone(),
                    scrub_json_value(v, env_vars, expose_secrets, Some(k)),
                );
            }
            JsonValue::Object(out)
        }
        _ => value.clone(),
    }
}

pub fn scrub_body_string(
    body: &str,
    env_vars: &HashMap<String, String>,
    expose_secrets: bool,
) -> String {
    if expose_secrets {
        return body.to_string();
    }
    if let Ok(mut v) = serde_json::from_str::<JsonValue>(body) {
        v = scrub_json_value(&v, env_vars, expose_secrets, None);
        serde_json::to_string_pretty(&v).unwrap_or_else(|_| body.to_string())
    } else {
        // Non-JSON body; best-effort replacement of env values
        let mut out = body.to_string();
        for (k, v) in env_vars {
            if !v.is_empty() && out.contains(v) {
                out = out.replace(v, &format!("${k}"));
            }
        }
        out
    }
}

pub fn scrub_baml_options(
    options: &baml_types::BamlMap<String, JsonValue>,
    env_vars: &HashMap<String, String>,
    expose_secrets: bool,
) -> JsonValue {
    let mut obj = serde_json::Map::new();
    for (k, v) in options.iter() {
        obj.insert(
            k.clone(),
            scrub_json_value(v, env_vars, expose_secrets, Some(k)),
        );
    }
    JsonValue::Object(obj)
}

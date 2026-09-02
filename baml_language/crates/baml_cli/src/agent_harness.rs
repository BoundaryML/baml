//! Detect the coding-agent harness invoking the CLI, if any.
//!
//! This is the Rust equivalent of the environment-based detection used by
//! Vercel's `@vercel/detect-agent`. Keep output policy and telemetry on this
//! shared detector so they cannot disagree about whether an agent is present.

use std::ffi::OsString;

/// Return a short, stable harness name, or `None` for an apparently human
/// invocation.
pub(crate) fn detect() -> Option<String> {
    detect_with(|name| std::env::var_os(name))
}

fn detect_with(lookup: impl Fn(&str) -> Option<OsString>) -> Option<String> {
    let value = |name: &str| {
        lookup(name)
            .and_then(|value| value.into_string().ok())
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty() && value != "0")
    };

    // `AI_AGENT` is the generic convention supported by
    // `@vercel/detect-agent`; its value is already the harness name.
    if let Some(name) = value("AI_AGENT") {
        return Some(name);
    }

    for (variable, harness) in [
        ("CLAUDECODE", "claude"),
        ("CODEX_SANDBOX", "codex"),
        ("PI_CODING_AGENT", "pi"),
        ("OPENCODE_CLIENT", "opencode"),
        ("CURSOR_TRACE_ID", "cursor"),
        ("REPL_ID", "replit"),
    ] {
        if value(variable).is_some() {
            return Some(harness.to_string());
        }
    }

    // `AGENT` is only a boolean-ish compatibility marker, so it establishes
    // that an agent is present without claiming to know which harness it is.
    value("AGENT").map(|_| "unknown".to_string())
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    fn detect_from(values: &[(&str, &str)]) -> Option<String> {
        let values = values
            .iter()
            .map(|(key, value)| ((*key).to_string(), OsString::from(value)))
            .collect::<HashMap<_, _>>();
        detect_with(|key| values.get(key).cloned())
    }

    #[test]
    fn reports_specific_harness_names() {
        for (variable, harness) in [
            ("CLAUDECODE", "claude"),
            ("CODEX_SANDBOX", "codex"),
            ("PI_CODING_AGENT", "pi"),
            ("OPENCODE_CLIENT", "opencode"),
            ("CURSOR_TRACE_ID", "cursor"),
            ("REPL_ID", "replit"),
        ] {
            assert_eq!(
                detect_from(&[(variable, "1")]).as_deref(),
                Some(harness),
                "variable: {variable}",
            );
        }
    }

    #[test]
    fn generic_ai_agent_value_is_the_harness_name() {
        assert_eq!(
            detect_from(&[("AI_AGENT", "custom-harness")]).as_deref(),
            Some("custom-harness"),
        );
    }

    #[test]
    fn generic_agent_marker_does_not_invent_a_name() {
        assert_eq!(detect_from(&[("AGENT", "1")]).as_deref(), Some("unknown"),);
    }

    #[test]
    fn empty_and_zero_values_do_not_detect_an_agent() {
        assert_eq!(detect_from(&[("CLAUDECODE", "")]), None);
        assert_eq!(detect_from(&[("CODEX_SANDBOX", "0")]), None);
    }
}

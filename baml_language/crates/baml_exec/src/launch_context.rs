// Process launch context for `baml run --context` / `BAML_CONTEXT` and
// packed binaries (`BAML_CONTEXT` only, since a packed binary's argv
// belongs to its target).

use anyhow::{Context as _, Result, anyhow, bail};
use btel_types::context::{Context, ContextPatch, ContextValue};
use serde_json::Value;

/// Environment variable carrying a launch context source. `baml run
/// --context` overrides it.
pub const CONTEXT_ENV: &str = "BAML_CONTEXT";

/// Read [`CONTEXT_ENV`] the way clap reads an `env`-backed flag: a set but
/// empty value is still a source, so it fails to parse rather than being
/// ignored.
pub fn context_source_from_env() -> Result<Option<String>> {
    match std::env::var(CONTEXT_ENV) {
        Ok(source) => Ok(Some(source)),
        Err(std::env::VarError::NotPresent) => Ok(None),
        Err(std::env::VarError::NotUnicode(_)) => bail!("{CONTEXT_ENV} is not valid UTF-8"),
    }
}

/// Load a launch context from inline JSON, `@file`, or `-` for stdin.
///
/// Accepts `distinct_id` (string or null) and `metadata` (a map whose values
/// are strings, integers, floats, or booleans).
pub fn load_launch_context(source: &str) -> Result<Context> {
    let value = crate::load_json_source(source).context("invalid context source")?;
    let object = value
        .as_object()
        .ok_or_else(|| anyhow!("context must be a JSON object"))?;
    let mut patch = ContextPatch::default();
    for (key, value) in object {
        match (key.as_str(), value) {
            ("distinct_id", Value::Null) => {}
            ("distinct_id", Value::String(id)) => patch.distinct_id = Some(id.clone()),
            ("distinct_id", _) => bail!("context distinct_id must be a string or null"),
            ("metadata", Value::Object(metadata)) => {
                for (key, value) in metadata {
                    let scalar = match value {
                        Value::String(value) => ContextValue::String(value.clone()),
                        Value::Bool(value) => ContextValue::Bool(*value),
                        Value::Number(value) if value.is_i64() => {
                            ContextValue::Int(value.as_i64().expect("checked integer"))
                        }
                        Value::Number(value) if value.is_f64() => {
                            ContextValue::Float(value.as_f64().expect("checked float"))
                        }
                        _ => bail!(
                            "context metadata[{key:?}] must be a string, int64, float64, or bool"
                        ),
                    };
                    patch.metadata.insert(key.clone(), Some(scalar));
                }
            }
            ("metadata", _) => bail!("context metadata must be an object"),
            _ => bail!("unknown context field {key:?}; expected distinct_id or metadata"),
        }
    }
    Ok(Context::default().with_patch(&patch))
}

/// Reject a context and an input that would both read stdin. Call before
/// loading either so a conflict exits without consuming stdin.
///
/// `context_label` and `input_label` name the sources in the error, e.g.
/// `--context` and `--json-args`.
pub fn ensure_single_stdin_consumer(
    context_source: Option<&str>,
    context_label: &str,
    input_source: Option<&str>,
    input_label: &str,
) -> Result<()> {
    if context_source == Some("-") && input_source == Some("-") {
        bail!(
            "{context_label} - and {input_label} - both read stdin; only one stdin consumer is allowed"
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn launch_context_validates_shape_and_preserves_scalar_types() {
        for source in ["{}", r#"{"distinct_id":null,"metadata":{}}"#] {
            assert!(load_launch_context(source).unwrap().is_empty());
        }
        let context = load_launch_context(
            r#"{"distinct_id":"user","metadata":{"s":"","i":-9223372036854775808,"f":1.0,"b":false}}"#,
        )
        .unwrap();
        assert_eq!(context.distinct_id(), Some("user"));
        assert_eq!(context.metadata()["s"], ContextValue::String(String::new()));
        assert_eq!(context.metadata()["i"], ContextValue::Int(i64::MIN));
        assert_eq!(context.metadata()["f"], ContextValue::Float(1.0));
        assert_eq!(context.metadata()["b"], ContextValue::Bool(false));
        for invalid in [
            "not JSON",
            "[]",
            "null",
            r#"{"unknown":1}"#,
            r#"{"distinct_id":42}"#,
            r#"{"metadata":null}"#,
            r#"{"metadata":{"x":null}}"#,
            r#"{"metadata":{"x":[]}}"#,
            r#"{"metadata":{"x":{}}}"#,
            r#"{"metadata":{"x":9223372036854775808}}"#,
            r#"{"metadata":{"x":18446744073709551616}}"#,
            r#"{"metadata":{"x":-9223372036854775809}}"#,
            r#"{"metadata":{"x":1e400}}"#,
        ] {
            assert!(load_launch_context(invalid).is_err(), "{invalid}");
        }
        let file = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(file.path(), r#"{"metadata":{"x":null}}"#).unwrap();
        assert!(load_launch_context(&format!("@{}", file.path().display())).is_err());
        std::fs::write(file.path(), "{").unwrap();
        assert!(load_launch_context(&format!("@{}", file.path().display())).is_err());
    }

    #[test]
    fn stdin_conflict_only_when_both_read_stdin() {
        assert!(ensure_single_stdin_consumer(Some("-"), "a", Some("-"), "b").is_err());
        assert!(ensure_single_stdin_consumer(Some("-"), "a", Some("{}"), "b").is_ok());
        assert!(ensure_single_stdin_consumer(None, "a", Some("-"), "b").is_ok());
        assert!(ensure_single_stdin_consumer(Some("-"), "a", None, "b").is_ok());
    }
}

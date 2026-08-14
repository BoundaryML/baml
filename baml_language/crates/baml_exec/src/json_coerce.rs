// JSON source loading for `--json-args`.
//
// Coercion-to-typed-value used to live here, but it now goes through
// `baml.json.deserialize<T>` (see `dispatch::build_args_from_signature`)
// so user `from_json` overrides on classes are honored. Only the I/O
// part — reading the JSON string from inline/@file/stdin — remains here.

use anyhow::{Context, Result};

/// Load JSON from the `--json-args` source: inline string, `@file`, or `-` for stdin.
pub fn load_json_source(source: &str) -> Result<serde_json::Value> {
    if source == "-" {
        let input =
            std::io::read_to_string(std::io::stdin()).context("failed to read JSON from stdin")?;
        serde_json::from_str(&input).context("invalid JSON from stdin")
    } else if let Some(path) = source.strip_prefix('@') {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read file: {path}"))?;
        serde_json::from_str(&content).with_context(|| format!("invalid JSON in file: {path}"))
    } else {
        serde_json::from_str(source).context("invalid inline JSON for `--json-args`")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_json_source_inline_object() {
        let v = load_json_source(r#"{"x": 1}"#).unwrap();
        assert_eq!(v["x"], serde_json::json!(1));
    }

    #[test]
    fn load_json_source_inline_array() {
        let v = load_json_source(r#"[1, 2, 3]"#).unwrap();
        assert!(v.is_array());
    }

    #[test]
    fn load_json_source_invalid_inline_errors() {
        assert!(load_json_source("not json").is_err());
    }

    #[test]
    fn load_json_source_file_at_prefix() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("args.json");
        std::fs::write(&path, r#"{"k": "v"}"#).unwrap();
        let source = format!("@{}", path.display());
        let v = load_json_source(&source).unwrap();
        assert_eq!(v["k"], serde_json::json!("v"));
    }

    #[test]
    fn load_json_source_missing_file_errors() {
        let err = load_json_source("@/nonexistent/baml-test-args.json").unwrap_err();
        let msg = format!("{err:?}");
        assert!(
            msg.contains("failed to read") || msg.contains("nonexistent"),
            "got: {msg}"
        );
    }

    #[test]
    fn load_json_source_file_with_invalid_json_errors() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("bad.json");
        std::fs::write(&path, "not json").unwrap();
        let source = format!("@{}", path.display());
        let err = load_json_source(&source).unwrap_err();
        assert!(format!("{err}").contains("invalid JSON in file"));
    }
}

pub mod config;
pub mod eval;
pub mod yaml2jinja;

use std::path::Path;

use config::UDFConfig;

pub fn read_udf_config(path: impl AsRef<Path>) -> anyhow::Result<UDFConfig> {
    use anyhow::Context;
    let contents = std::fs::read_to_string(path).context("read UDF config from disk")?;

    serde_yaml::from_str(&contents).context("deserialize UDF config file")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IntrusiveStack<'a, T> {
    pub prev: Option<&'a IntrusiveStack<'a, T>>,
    pub cur: T,
}

/// Adds `date_between` filter to the environment.
pub fn get_env<'s>() -> minijinja::Environment<'s> {
    let mut env = internal_baml_core::ir::jinja_helpers::get_env();

    env.add_filter("date_between", date_between);
    env
}

// NOTE: (Jesus) Could use real date type
fn date_between(date: String, begin: String, end: String) -> Result<bool, minijinja::Error> {
    fn parse_date(date: &str) -> chrono::ParseResult<chrono::NaiveDate> {
        chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d")
    }

    // NOTE: (Jesus) This will be parsing `begin` and `end` dates for all rows and `date` for all date
    // comparisons.
    let Ok(date) = parse_date(&date) else {
        return Ok(false);
    };
    let begin = parse_date(&begin).map_err(|_| {
        minijinja::Error::new(minijinja::ErrorKind::SyntaxError, "Cannot parse begin date")
    })?;
    let end = parse_date(&end).map_err(|_| {
        minijinja::Error::new(minijinja::ErrorKind::SyntaxError, "Cannot parse end date")
    })?;

    Ok(date >= begin && date <= end)
}

pub mod tests {
    //! Utilities for testing
    use crate::{config::UDFConfig, read_udf_config};

    pub fn load_sample_udf() -> UDFConfig {
        use anyhow::Context;
        read_udf_config("./sample-prices.yaml")
            .context("load sample UDF config")
            .unwrap()
    }

    // NOTE: sample data in here has been generated with the help of AI.
    pub mod data {

        use serde::Serialize;
        use serde_json::{json, Map};

        type Dict = Map<String, serde_json::Value>;

        pub fn gemini() -> DbHttpMetadata {
            DbHttpMetadata {
                client: DbHttpClientDetails {
                    name: "gemini/gemini-pro".into(),
                    options: json!({
                        "model": "gemini-pro"
                    })
                    .as_object()
                    .unwrap()
                    .clone(),
                    provider: "gemini".into(),
                },
                http_request_id: "_gemini_test_456".into(),
                response: DbHttpResponseMetadata {
                    body: json!({
                        "usageMetadata": {
                            "promptTokenCount": 900,
                            "cachedTokenCount": 100,
                            "candidatesTokenCount": 400
                        }
                    })
                    .as_object()
                    .unwrap()
                    .clone(),
                    headers: json!({
                        "content-type": "application/json",
                        "date": "2025-06-10 15:30:00.000000000"
                    })
                    .as_object()
                    .unwrap()
                    .clone(),
                    model: Some("gemini-pro".into()),
                    status: "200".into(),
                },
            }
        }

        pub fn anthropic() -> DbHttpMetadata {
            DbHttpMetadata {
                client: DbHttpClientDetails {
                    name: "anthropic/claude-3-opus".into(),
                    options: json!({
                        "model": "claude-3-opus"
                    })
                    .as_object()
                    .unwrap()
                    .clone(),
                    provider: "anthropic".into(),
                },
                http_request_id: "_anthropic_test_123".into(),
                response: DbHttpResponseMetadata {
                    body: json!({
                        "usage": {
                            "input_tokens": 1200,
                            "cached_tokens": 150,
                            "output_tokens": 600
                        }
                    })
                    .as_object()
                    .unwrap()
                    .clone(),
                    headers: json!({
                        "content-type": "application/json",
                        "date": "2025-06-01 12:00:00.000000000"
                    })
                    .as_object()
                    .unwrap()
                    .clone(),
                    model: Some("claude-3-opus".into()),
                    status: "200".into(),
                },
            }
        }

        pub fn none_match() -> DbHttpMetadata {
            DbHttpMetadata {
                client: DbHttpClientDetails {
                    name: "unknown-client".into(),
                    options: json!({
                        "model": "llama-9000"
                    })
                    .as_object()
                    .unwrap()
                    .clone(),
                    provider: "llama-corp".into(), // Not openai, anthropic, or gemini
                },
                http_request_id: "_unknown_test_789".into(),
                response: DbHttpResponseMetadata {
                    body: json!({
                        "error": {
                            "message": "Model not found",
                            "code": 404
                        }
                    })
                    .as_object()
                    .unwrap()
                    .clone(),
                    headers: json!({
                        "content-type": "application/json",
                        "date": "2025-07-01 10:00:00.000000000"
                    })
                    .as_object()
                    .unwrap()
                    .clone(),
                    model: Some("llama-9000".into()),
                    status: "500".into(), // Doesn't match any known expression like "status == 200"
                },
            }
        }

        pub fn anthropic_with_bad_raw() -> DbHttpMetadata {
            DbHttpMetadata {
                client: DbHttpClientDetails {
                    name: "anthropic/claude-3-haiku".into(),
                    options: json!({
                        "model": "claude-3-haiku"
                    })
                    .as_object()
                    .unwrap()
                    .clone(),
                    provider: "anthropic".into(), // ✅ matches provider expression
                },
                http_request_id: "_anthropic_bad_raw_999".into(),
                response: DbHttpResponseMetadata {
                    body: json!({
                        // ❌ Missing `input_tokens`, `cached_tokens`, `output_tokens`
                        "meta": {
                            "token_usage": {
                                "prompt": 123,
                                "completion": 456
                            }
                        },
                        "data": {
                            "some_other_field": true
                        }
                    })
                    .as_object()
                    .unwrap()
                    .clone(),
                    headers: json!({
                        "content-type": "application/json",
                        "date": "2025-06-12 08:45:00.000000000"
                    })
                    .as_object()
                    .unwrap()
                    .clone(),
                    model: Some("claude-3-haiku".into()),
                    status: "200".into(),
                },
            }
        }

        pub fn openai() -> DbHttpMetadata {
            use serde_json::json;

            DbHttpMetadata {
                client: DbHttpClientDetails {
                    name: "openai/gpt-4o".into(),
                    options: json!({
                        "model": "gpt-4o"
                    })
                    .as_object()
                    .unwrap()
                    .clone(),
                    provider: "openai".into(),
                },
                http_request_id: "<idc>".into(),
                response: DbHttpResponseMetadata {
                    body: json!({
                        "usage": {
                            "completion_tokens": "97",
                            "prompt_tokens": "133",
                            "total_tokens": "230"
                        }
                    })
                    .as_object()
                    .unwrap()
                    .clone(),
                    headers: json!({
                        "access-control-expose-headers": "X-Request-ID",
                        "cf-cache-status": "DYNAMIC",
                        "content-type": "application/json",
                        "date": "2025-07-16 04:54:04.000000000",
                        "openai-processing-ms": "1230",
                        "openai-version": "2020-10-01",
                        "server": "cloudflare",
                    })
                    .as_object()
                    .unwrap()
                    .clone(),
                    model: Some("gpt-4o-2024-08-06".into()),
                    status: "200".into(),
                },
            }
        }

        #[derive(Debug, Serialize)]
        struct DbHttpClientDetails {
            /// BAML client name
            name: String,
            /// Request parameters
            options: Dict,
            /// Provider name (openai, anthropic, etc.)
            provider: String,
        }

        #[derive(Debug, Serialize)]
        pub struct DbHttpResponseMetadata {
            /// Full response body parsed as JSON
            body: Dict,
            /// Response headers
            headers: Dict,
            /// Extracted model name
            model: Option<String>,
            /// HTTP status code as string
            status: String,
        }

        #[derive(Debug, Serialize)]
        pub struct DbHttpMetadata {
            client: DbHttpClientDetails,
            http_request_id: String,
            response: DbHttpResponseMetadata,
        }
    }
}

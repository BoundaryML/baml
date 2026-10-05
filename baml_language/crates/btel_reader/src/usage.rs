//! Provider usage and estimated cost, independent of storage and query backends.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::pricing;

/// Usage for one or more model turns. Input excludes cache reads and writes.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Usage {
    pub model_name: Option<String>,
    pub model_calls: i64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_read_tokens: Option<i64>,
    pub cache_write_tokens: Option<i64>,
    pub reasoning_tokens: Option<i64>,
    /// Estimated dollars; unknown when any turn has no known price.
    pub cost: Option<f64>,
}

impl Usage {
    pub fn turn(model: Option<String>, tokens: [Option<i64>; 5]) -> Self {
        let [input, output, cache_read, cache_write, reasoning] = tokens;
        Self {
            cost: pricing::cost(
                model.as_deref(),
                input.unwrap_or(0),
                output.unwrap_or(0),
                cache_read,
                cache_write,
            ),
            model_name: model,
            model_calls: 1,
            input_tokens: input.unwrap_or(0),
            output_tokens: output.unwrap_or(0),
            cache_read_tokens: cache_read,
            cache_write_tokens: cache_write,
            reasoning_tokens: reasoning,
        }
    }

    /// Sum one-turn projections in recording order, preserving unknown counts and prices.
    pub fn sum(turns: impl IntoIterator<Item = Self>) -> Option<Self> {
        let mut sum: Option<Self> = None;
        let mut models = Vec::new();
        let add = |a: Option<i64>, b: Option<i64>| {
            b.map_or(a, |b| Some(a.unwrap_or(0).saturating_add(b)))
        };
        for turn in turns {
            if let Some(model) = &turn.model_name
                && !models.contains(model)
            {
                models.push(model.clone());
            }
            match &mut sum {
                None => sum = Some(turn),
                Some(sum) => {
                    sum.model_calls = sum.model_calls.saturating_add(turn.model_calls);
                    sum.input_tokens = sum.input_tokens.saturating_add(turn.input_tokens);
                    sum.output_tokens = sum.output_tokens.saturating_add(turn.output_tokens);
                    sum.cache_read_tokens = add(sum.cache_read_tokens, turn.cache_read_tokens);
                    sum.cache_write_tokens = add(sum.cache_write_tokens, turn.cache_write_tokens);
                    sum.reasoning_tokens = add(sum.reasoning_tokens, turn.reasoning_tokens);
                    sum.cost = sum.cost.zip(turn.cost).map(|(a, b)| a + b);
                }
            }
        }
        if let Some(sum) = &mut sum {
            sum.model_name = (!models.is_empty()).then(|| models.join(", "));
        }
        sum
    }

    /// Recorded counters are unsigned; the query surface saturates at signed 64-bit counts.
    pub fn recorded(entries: impl IntoIterator<Item = crate::proto::ModelUsage>) -> Option<Self> {
        let fits = |n: u64| i64::try_from(n).unwrap_or(i64::MAX);
        Self::sum(entries.into_iter().map(|entry| {
            Self::turn(
                entry.model,
                [
                    Some(fits(entry.input_tokens)),
                    Some(fits(entry.output_tokens)),
                    entry.cache_read_tokens.map(fits),
                    entry.cache_write_tokens.map(fits),
                    entry.reasoning_tokens.map(fits),
                ],
            )
        }))
    }
}

#[derive(Clone, Copy, Debug)]
enum Provider {
    Anthropic,
    Chat,
    Responses,
    Gemini,
    Bedrock,
    Jev,
}

/// Cumulative response counters. Taking each counter's maximum handles a whole
/// response and streaming updates without billing repeated cumulative counts.
#[derive(Clone, Debug)]
pub struct NetworkUsage {
    provider: Provider,
    url: String,
    model: Option<String>,
    tokens: [Option<i64>; 5],
}

impl NetworkUsage {
    /// Route priority matches the local query surface, including gateway URLs.
    pub fn new(url: &str) -> Option<Self> {
        let lower = url.to_ascii_lowercase();
        let provider = if lower.contains("/chat/completions") {
            Provider::Chat
        } else if lower.contains("/responses") {
            Provider::Responses
        } else if lower.contains("/v1/messages") || lower.contains("rawpredict") {
            Provider::Anthropic
        } else if lower.contains("generatecontent") {
            Provider::Gemini
        } else if lower
            .split_once("/model/")
            .is_some_and(|(_, rest)| rest.contains("/converse"))
        {
            Provider::Bedrock
        } else if lower.contains("/systemone") {
            Provider::Jev
        } else {
            return None;
        };
        Some(Self {
            provider,
            url: url.into(),
            model: None,
            tokens: [None; 5],
        })
    }

    pub fn observe(&mut self, text: &str) {
        let Ok(mut body) = serde_json::from_str::<Value>(text) else {
            return;
        };
        let (envelope, model, paths) = match self.provider {
            Provider::Anthropic => (
                Some("message"),
                Some("/model"),
                [
                    "/usage/input_tokens",
                    "/usage/output_tokens",
                    "/usage/cache_read_input_tokens",
                    "/usage/cache_creation_input_tokens",
                    "/usage/output_tokens_details/thinking_tokens",
                ],
            ),
            Provider::Chat => (
                None,
                Some("/model"),
                [
                    "/usage/prompt_tokens",
                    "/usage/completion_tokens",
                    "/usage/prompt_tokens_details/cached_tokens",
                    "",
                    "/usage/completion_tokens_details/reasoning_tokens",
                ],
            ),
            Provider::Responses => (
                Some("response"),
                Some("/model"),
                [
                    "/usage/input_tokens",
                    "/usage/output_tokens",
                    "/usage/input_tokens_details/cached_tokens",
                    "",
                    "/usage/output_tokens_details/reasoning_tokens",
                ],
            ),
            Provider::Gemini => (
                None,
                Some("/modelVersion"),
                [
                    "/usageMetadata/promptTokenCount",
                    "/usageMetadata/candidatesTokenCount",
                    "/usageMetadata/cachedContentTokenCount",
                    "",
                    "/usageMetadata/thoughtsTokenCount",
                ],
            ),
            Provider::Bedrock => (
                None,
                None,
                [
                    "/usage/inputTokens",
                    "/usage/outputTokens",
                    "/usage/cacheReadInputTokens",
                    "/usage/cacheWriteInputTokens",
                    "",
                ],
            ),
            Provider::Jev => (
                None,
                None,
                ["/usage/input_tokens", "/usage/output_tokens", "", "", ""],
            ),
        };
        if let Some(envelope) = envelope
            && let Some(wrapped) = body.get_mut(envelope)
            && !wrapped.is_null()
        {
            body = wrapped.take();
        }
        if let Some(model) = model.and_then(|p| body.pointer(p)).and_then(Value::as_str)
            && self.model.as_deref().is_none_or(|held| model > held)
        {
            self.model = Some(model.into());
        }
        for (held, path) in self.tokens.iter_mut().zip(paths) {
            if !path.is_empty()
                && let Some(n) = body.pointer(path).and_then(Value::as_i64)
            {
                *held = Some(held.map_or(n, |held| held.max(n)));
            }
        }
    }

    /// A request body is needed only when usage lacks a response model.
    pub fn needs_request_model(&self) -> bool {
        self.model.is_none() && (self.tokens[0].is_some() || self.tokens[1].is_some())
    }

    /// Model precedence is response, request, then the model in the URL.
    pub fn finish(&self, request: Option<&str>) -> Option<Usage> {
        let mut tokens = self.tokens;
        if tokens[0].is_none() && tokens[1].is_none() {
            return None;
        }
        if matches!(
            self.provider,
            Provider::Chat | Provider::Responses | Provider::Gemini
        ) {
            tokens[0] = tokens[0].map(|n| n.saturating_sub(tokens[2].unwrap_or(0)).max(0));
        }
        let model = self
            .model
            .clone()
            .or_else(|| {
                serde_json::from_str::<Value>(request?)
                    .ok()?
                    .get("model")?
                    .as_str()
                    .map(str::to_owned)
            })
            .or_else(|| {
                let marker = match self.provider {
                    Provider::Anthropic | Provider::Gemini => "/models/",
                    Provider::Bedrock => "/model/",
                    _ => return None,
                };
                let (_, rest) = self.url.split_once(marker)?;
                let segment = rest.split(['/', ':', '?', '#']).next()?;
                (!segment.is_empty()).then(|| percent_decode(segment))
            });
        Some(Usage::turn(model, tokens))
    }
}

fn percent_decode(text: &str) -> String {
    let mut bytes = Vec::new();
    let mut rest = text.as_bytes();
    while let Some((&first, tail)) = rest.split_first() {
        if first == b'%'
            && tail.len() >= 2
            && let Ok(hex) = std::str::from_utf8(&tail[..2])
            && let Ok(byte) = u8::from_str_radix(hex, 16)
        {
            bytes.push(byte);
            rest = &tail[2..];
        } else {
            bytes.push(first);
            rest = tail;
        }
    }
    String::from_utf8_lossy(&bytes).into_owned()
}

/// Text of a whole response, an SSE event's `data`, or a request's `request.body`.
/// Missing, private, truncated and non-text bodies supply no usage.
pub fn body_text(source: &impl crate::value::BlobSource, id: crate::CasId) -> Option<String> {
    use crate::value::{Found, Nav, RenderLimits, Root, Segment, navigate};
    let snapshot = source.load(id).ok()?;
    let key = |k: &str| Segment::Key(k.into());
    for path in [vec![], vec![key("data")], vec![key("request"), key("body")]] {
        if let Nav::Value(found) = navigate(
            source,
            &snapshot,
            Root::Value,
            None,
            &path,
            RenderLimits::default().max_blobs,
        ) && let Found::String(text) = found.value()
        {
            return Some(text.to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cumulative_cache_counts_and_model_precedence() {
        let mut usage = NetworkUsage::new("https://provider/v1/messages").unwrap();
        usage.observe(r#"{"message":{"model":"claude-opus-5-5","usage":{"input_tokens":1000,"cache_read_input_tokens":2000,"cache_creation_input_tokens":50}}}"#);
        usage.observe(r#"{"usage":{"output_tokens":10}}"#);
        usage.observe(r#"{"usage":{"output_tokens":30}}"#);
        let u = usage.finish(Some(r#"{"model":"unknown"}"#)).unwrap();
        assert_eq!(u.model_name.as_deref(), Some("claude-opus-5-5"));
        assert_eq!(u.output_tokens, 30);
        assert!((u.cost.unwrap() - 0.00525).abs() < 1e-9);
        let mut chat = NetworkUsage::new("https://provider/chat/completions").unwrap();
        chat.observe(r#"{"usage":{"prompt_tokens":100,"completion_tokens":10,"prompt_tokens_details":{"cached_tokens":20}}}"#);
        let u = chat.finish(Some(r#"{"model":"gpt-6-sol"}"#)).unwrap();
        assert_eq!(u.input_tokens, 80);
        assert_eq!(u.cache_read_tokens, Some(20));
        assert!((u.cost.unwrap() - 0.000_264).abs() < 1e-9);
    }
    #[test]
    fn unknown_and_missing_usage_stay_unknown() {
        assert!(NetworkUsage::new("https://provider/other").is_none());
        let mut usage = NetworkUsage::new("https://provider/responses").unwrap();
        usage.observe("private or malformed");
        assert_eq!(usage.finish(None), None);
        usage.observe(
            r#"{"response":{"model":"unknown","usage":{"input_tokens":1,"output_tokens":2}}}"#,
        );
        assert_eq!(usage.finish(None).unwrap().cost, None);
    }
}

#[cfg(test)]
mod recorded_tests {
    use super::*;
    #[test]
    fn mixed_models_optional_counts_and_unknown_prices() {
        let turn = |model: Option<&str>, input, read| crate::proto::ModelUsage {
            model: model.map(str::to_owned),
            input_tokens: input,
            cache_read_tokens: read,
            ..Default::default()
        };
        let entries = [
            turn(Some("gpt-6-sol"), 100, None),
            turn(Some("gpt-6-sol"), 200, Some(0)),
            turn(None, 1, None),
        ];
        let usage = Usage::recorded(entries).unwrap();
        assert_eq!(usage.model_calls, 3);
        assert_eq!(usage.model_name.as_deref(), Some("gpt-6-sol"));
        assert_eq!(usage.input_tokens, 301);
        assert_eq!(usage.cache_read_tokens, Some(0));
        assert_eq!(usage.cache_write_tokens, None);
        assert_eq!(usage.cost, None);
        assert_eq!(Usage::recorded([]), None);
        let usage = Usage::recorded([
            turn(Some("jev-1.13.0"), u64::MAX, None),
            turn(Some("gpt-6-sol"), 1, None),
        ])
        .unwrap();
        assert_eq!(usage.input_tokens, i64::MAX);
        assert_eq!(usage.model_name.as_deref(), Some("jev-1.13.0, gpt-6-sol"));
        assert!(usage.cost.unwrap().is_finite());
    }
}

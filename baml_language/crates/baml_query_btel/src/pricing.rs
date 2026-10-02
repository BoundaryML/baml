//! Model prices for `spans.temporary_projections.cost`, until journal
//! projections replace the column. Dollars per million tokens.

struct Price {
    model: &'static str,
    input: f64,
    output: f64,
    /// Cache reads, when the provider publishes a rate; else 0.1x input.
    cache_read: Option<f64>,
}

/// Longer IDs first where one is a prefix of another (Opus 5.5 before Opus 5).
const PRICES: &[Price] = &[
    Price {
        model: "jev-1.13.0",
        input: 0.042,
        output: 0.0,
        cache_read: Some(0.042),
    },
    Price {
        model: "claude-fable-5-1",
        input: 10.0,
        output: 50.0,
        cache_read: Some(0.25),
    },
    Price {
        model: "claude-opus-5-5",
        input: 4.0,
        output: 20.0,
        cache_read: Some(0.2),
    },
    Price {
        model: "claude-opus-5",
        input: 5.0,
        output: 25.0,
        cache_read: None,
    },
    Price {
        model: "claude-sonnet-5",
        input: 2.0,
        output: 10.0,
        cache_read: None,
    },
    Price {
        model: "claude-haiku-4-5",
        input: 1.0,
        output: 5.0,
        cache_read: None,
    },
    Price {
        model: "gpt-6-sol",
        input: 2.0,
        output: 10.0,
        cache_read: Some(0.2),
    },
    Price {
        model: "gpt-oss-120b",
        input: 0.15,
        output: 0.6,
        cache_read: None,
    },
    Price {
        model: "gpt-oss-20b",
        input: 0.07,
        output: 0.3,
        cache_read: None,
    },
];

/// A cache write costs this many times the input rate.
const CACHE_WRITE_FACTOR: f64 = 1.25;
/// A cache read costs this share of the input rate unless priced.
const CACHE_READ_FACTOR: f64 = 0.1;

/// Bedrock charges this many times the list price for a Claude model reached
/// through anything but a `global.` inference profile.
const BEDROCK_REGIONAL_FACTOR: f64 = 1.1;

/// The providers whose models Bedrock (Converse and Mantle) serves under a
/// `<provider>.` prefix and that have prices here.
const BEDROCK_PROVIDERS: &[&str] = &["anthropic", "openai"];

fn bedrock_provider(segment: Option<&str>) -> Option<&str> {
    segment.filter(|s| BEDROCK_PROVIDERS.contains(s))
}

/// A model ID as its price is listed, and what to multiply that price by.
/// Bedrock IDs are `[<geo>.]<provider>.<model>[-v1:0]`, bare or as the last
/// segment of an ARN: `us.anthropic.claude-haiku-4-5-20251001-v1:0`,
/// `openai.gpt-oss-20b`. Any other ID is returned as it is.
fn listed(model: &str) -> (&str, f64) {
    let id = match model.strip_prefix("arn:") {
        Some(arn) => arn.rsplit('/').next().unwrap_or(arn),
        None => model,
    };
    let mut segments = id.splitn(3, '.');
    let (first, second, third) = (segments.next(), segments.next(), segments.next());
    let (geo, provider, name) = match (bedrock_provider(first), bedrock_provider(second), third) {
        (Some(provider), ..) if second.is_some() => (None, provider, &id[provider.len() + 1..]),
        (None, Some(provider), Some(name)) => (first, provider, name),
        _ => return (model, 1.0),
    };
    if provider == "anthropic" && geo != Some("global") {
        (name, BEDROCK_REGIONAL_FACTOR)
    } else {
        (name, 1.0)
    }
}

fn price(model: &str) -> Option<&'static Price> {
    PRICES.iter().find(|price| {
        model == price.model
            || model
                .strip_prefix(price.model)
                .is_some_and(|rest| rest.starts_with('-'))
    })
}

/// Dollars for one model turn; `None` for a model without a price. `input`
/// excludes cache reads and writes.
pub(crate) fn cost(
    model: Option<&str>,
    input: i64,
    output: i64,
    cache_read: Option<i64>,
    cache_write: Option<i64>,
) -> Option<f64> {
    let (model, factor) = listed(model?);
    let price = price(model)?;
    #[expect(
        clippy::cast_precision_loss,
        reason = "token counts are far below 2^52"
    )]
    let tokens = |n: i64| n.max(0) as f64;
    let read = price.cache_read.unwrap_or(price.input * CACHE_READ_FACTOR);
    Some(
        (tokens(input) * price.input
            + tokens(cache_write.unwrap_or(0)) * price.input * CACHE_WRITE_FACTOR
            + tokens(cache_read.unwrap_or(0)) * read
            + tokens(output) * price.output)
            * factor
            / 1_000_000.0,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prices_dated_ids_by_prefix_and_never_guesses() {
        // 1M fresh input and 100k output on Opus 5.5.
        let opus = cost(
            Some("claude-opus-5-5-20260915"),
            1_000_000,
            100_000,
            None,
            None,
        );
        assert!((opus.unwrap() - 6.0).abs() < 1e-9);
        // Opus 5.5 is not priced as Opus 5.
        assert_ne!(
            opus,
            cost(Some("claude-opus-5"), 1_000_000, 100_000, None, None)
        );
        // Cache writes cost 1.25x input, reads the published rate.
        let cached = cost(
            Some("claude-opus-5-5"),
            0,
            0,
            Some(1_000_000),
            Some(1_000_000),
        );
        assert!((cached.unwrap() - (0.2 + 5.0)).abs() < 1e-9);
        // Jev: output is free.
        let jev = cost(Some("jev-1.13.0"), 1_000_000, 1_000_000, None, None);
        assert!((jev.unwrap() - 0.042).abs() < 1e-9);
        assert_eq!(cost(Some("some-new-model"), 1, 1, None, None), None);
        assert_eq!(cost(None, 1, 1, None, None), None);
    }

    #[test]
    fn prices_bedrock_ids_under_the_listed_model() {
        let million = |model: &str| cost(Some(model), 1_000_000, 0, None, None);
        let direct = million("claude-haiku-4-5-20251001").unwrap();
        // A global profile is the list price; every other route to a Claude
        // model is 10% over it.
        for global in [
            "global.anthropic.claude-haiku-4-5-20251001-v1:0",
            "arn:aws:bedrock:us-east-1:123456789012:inference-profile/global.anthropic.claude-haiku-4-5-20251001-v1:0",
        ] {
            assert!((million(global).unwrap() - direct).abs() < 1e-9, "{global}");
        }
        for regional in [
            "us.anthropic.claude-haiku-4-5-20251001-v1:0",
            "anthropic.claude-haiku-4-5-20251001-v1:0",
            "arn:aws:bedrock:us-east-1::foundation-model/anthropic.claude-haiku-4-5-20251001-v1:0",
        ] {
            assert!(
                (million(regional).unwrap() - direct * 1.1).abs() < 1e-9,
                "{regional}"
            );
        }
        // An undated ID, and the longer of two IDs that share a prefix.
        assert!((million("us.anthropic.claude-opus-5-5").unwrap() - 4.4).abs() < 1e-9);
        // Mantle and Converse spell the same open-weight model differently.
        for oss in ["openai.gpt-oss-20b", "openai.gpt-oss-20b-1:0"] {
            assert!((million(oss).unwrap() - 0.07).abs() < 1e-9, "{oss}");
        }
        // A dot in a direct model ID is not a Bedrock prefix.
        assert_eq!(million("gpt-5.6-luna"), None);
        assert_eq!(million("gpt-6.1-sol"), None);
        // Unpriced on Bedrock stays unpriced.
        assert_eq!(million("us.anthropic.claude-opus-4-6-v1"), None);
        assert_eq!(million("amazon.nova-micro-v1:0"), None);
        assert_eq!(million("openai"), None);
        assert_eq!(
            million("arn:aws:bedrock:us-east-1:123456789012:application-inference-profile/abc123"),
            None
        );
    }
}

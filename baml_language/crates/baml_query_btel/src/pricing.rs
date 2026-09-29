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
];

/// A cache write costs this many times the input rate.
const CACHE_WRITE_FACTOR: f64 = 1.25;
/// A cache read costs this share of the input rate unless priced.
const CACHE_READ_FACTOR: f64 = 0.1;

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
    let price = price(model?)?;
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
}

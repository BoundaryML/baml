//! Mirrored by the BCS projector:
//! <https://github.com/BoundaryML/bcs/blob/main/data-plane/crates/dataplane/src/projector/usage/pricing.rs>
//! Update both copies until a shared-file arrangement replaces the copy.

//! Estimated model prices in integer nanodollars (10^-9 USD).
//! Rates are nanodollars per token; cache factors stay rational until the turn is rounded.

use serde::{Deserialize, Serialize};

/// USD represented as signed integer nanodollars, including on the serialized wire.
#[repr(transparent)]
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(transparent)]
pub(crate) struct Usd(i64);

impl Usd {
    pub(crate) const fn from_nano_usd(amount: i64) -> Self {
        Self(amount)
    }

    pub(crate) const fn as_nano_usd(self) -> i64 {
        self.0
    }

    pub(crate) fn checked_add(self, other: Self) -> Option<Self> {
        self.0.checked_add(other.0).map(Self)
    }

    /// Round a rational nanodollar amount once, with halfway amounts away from zero.
    /// Nonpositive scales and amounts outside the signed 64-bit range are unknown.
    fn from_scaled_nano_usd(amount: i128, scale: i128) -> Option<Self> {
        if scale <= 0 {
            return None;
        }
        let whole = amount / scale;
        let remainder = amount % scale;
        let half = scale / 2 + scale % 2;
        let rounded = if remainder >= half {
            whole.checked_add(1)?
        } else if remainder <= -half {
            whole.checked_sub(1)?
        } else {
            whole
        };
        i64::try_from(rounded).ok().map(Self)
    }
}

struct Price {
    model: &'static str,
    input: Usd,
    output: Usd,
    /// Cache reads, when the provider publishes a rate; else 1/10 of input.
    cache_read: Option<Usd>,
    /// Rates for the whole turn when its prompt is over a size.
    long_context: Option<LongContext>,
}

/// Rates applied when fresh input, cache reads and cache writes exceed `above` tokens.
struct LongContext {
    above: i64,
    input: Usd,
    output: Usd,
    cache_read: Usd,
}

/// Longer IDs first where one is a prefix of another (Opus 5.5 before Opus 5).
const PRICES: &[Price] = &[
    Price {
        model: "jev-1.13.0",
        input: Usd::from_nano_usd(42),
        output: Usd::from_nano_usd(0),
        cache_read: Some(Usd::from_nano_usd(42)),
        long_context: None,
    },
    Price {
        model: "claude-fable-5-1",
        input: Usd::from_nano_usd(10_000),
        output: Usd::from_nano_usd(50_000),
        cache_read: Some(Usd::from_nano_usd(250)),
        long_context: None,
    },
    Price {
        model: "claude-opus-5-5",
        input: Usd::from_nano_usd(4_000),
        output: Usd::from_nano_usd(20_000),
        cache_read: Some(Usd::from_nano_usd(200)),
        long_context: None,
    },
    Price {
        model: "claude-opus-5",
        input: Usd::from_nano_usd(5_000),
        output: Usd::from_nano_usd(25_000),
        cache_read: None,
        long_context: None,
    },
    Price {
        model: "claude-sonnet-5",
        input: Usd::from_nano_usd(2_000),
        output: Usd::from_nano_usd(10_000),
        cache_read: None,
        long_context: None,
    },
    Price {
        model: "claude-haiku-4-5",
        input: Usd::from_nano_usd(1_000),
        output: Usd::from_nano_usd(5_000),
        cache_read: None,
        long_context: None,
    },
    Price {
        model: "gpt-6-sol",
        input: Usd::from_nano_usd(2_000),
        output: Usd::from_nano_usd(10_000),
        cache_read: Some(Usd::from_nano_usd(200)),
        // Over 272K prompt tokens: 2x input and cache rates, 1.5x output.
        long_context: Some(LongContext {
            above: 272_000,
            input: Usd::from_nano_usd(4_000),
            output: Usd::from_nano_usd(15_000),
            cache_read: Usd::from_nano_usd(400),
        }),
    },
];

// Twentieths keep both the 5/4 cache-write and 1/10 fallback cache-read rates exact.
const RATE_SCALE: i64 = 20;
const CACHE_WRITE_WEIGHT: i64 = 25;
const CACHE_READ_WEIGHT: i64 = 2;

fn price(model: &str) -> Option<&'static Price> {
    PRICES.iter().find(|price| {
        model == price.model
            || model
                .strip_prefix(price.model)
                .is_some_and(|rest| rest.starts_with('-'))
    })
}

/// Nanodollars for one model turn; unknown for an unpriced model or an overflowing amount.
/// `input` excludes cache reads and writes. Negative counters contribute zero.
pub(crate) fn cost_nano_usd(
    model: Option<&str>,
    input: i64,
    output: i64,
    cache_read: Option<i64>,
    cache_write: Option<i64>,
) -> Option<Usd> {
    let price = price(model?)?;
    let cache_read = cache_read.unwrap_or(0).max(0);
    let cache_write = cache_write.unwrap_or(0).max(0);
    let prompt = i128::from(input.max(0)) + i128::from(cache_read) + i128::from(cache_write);
    let (input_rate, output_rate, read, read_weight) = match &price.long_context {
        Some(long) if prompt > i128::from(long.above) => {
            (long.input, long.output, long.cache_read, RATE_SCALE)
        }
        _ => {
            let (read, weight) = price
                .cache_read
                .map_or((price.input, CACHE_READ_WEIGHT), |read| (read, RATE_SCALE));
            (price.input, price.output, read, weight)
        }
    };
    // i128 accommodates all i64 token counts at the table's rates, before rounding to i64.
    let charge = |rate: Usd, tokens: i64, weight: i64| {
        i128::from(rate.as_nano_usd()) * i128::from(tokens.max(0)) * i128::from(weight)
    };
    let total = charge(input_rate, input, RATE_SCALE)
        + charge(input_rate, cache_write, CACHE_WRITE_WEIGHT)
        + charge(read, cache_read, read_weight)
        + charge(output_rate, output, RATE_SCALE);
    Usd::from_scaled_nano_usd(total, i128::from(RATE_SCALE))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prices_dated_ids_by_prefix_and_never_guesses() {
        let opus = cost_nano_usd(
            Some("claude-opus-5-5-20260915"),
            1_000_000,
            100_000,
            None,
            None,
        );
        assert_eq!(opus, Some(Usd::from_nano_usd(6_000_000_000)));
        assert_ne!(
            opus,
            cost_nano_usd(Some("claude-opus-5"), 1_000_000, 100_000, None, None)
        );
        let cached = cost_nano_usd(
            Some("claude-opus-5-5"),
            0,
            0,
            Some(1_000_000),
            Some(1_000_000),
        );
        assert_eq!(cached, Some(Usd::from_nano_usd(5_200_000_000)));
        let jev = cost_nano_usd(Some("jev-1.13.0"), 1_000_000, 1_000_000, None, None);
        assert_eq!(jev, Some(Usd::from_nano_usd(42_000_000)));
        assert_eq!(
            cost_nano_usd(Some("some-new-model"), 1, 1, None, None),
            None
        );
        assert_eq!(cost_nano_usd(None, 1, 1, None, None), None);
    }

    #[test]
    fn fractional_cache_rates_round_once_for_the_whole_turn() {
        let model = Some("jev-1.13.0");
        assert_eq!(cost_nano_usd(model, 1, 0, None, None), Some(Usd(42)));
        assert_eq!(cost_nano_usd(model, 0, 0, None, Some(1)), Some(Usd(53)));
        assert_eq!(cost_nano_usd(model, 0, 0, None, Some(2)), Some(Usd(105)));
        assert_eq!(cost_nano_usd(model, 1, 0, Some(1), Some(1)), Some(Usd(137)));
        assert_eq!(
            cost_nano_usd(Some("claude-opus-5"), 0, 0, Some(1), Some(1)),
            Some(Usd(6_750))
        );
    }

    #[test]
    fn long_context_counts_all_prompt_tokens_and_switches_above_the_boundary() {
        let model = Some("gpt-6-sol");
        assert_eq!(
            cost_nano_usd(model, 100_000, 100_000, Some(100_000), Some(100_000)),
            Some(Usd(2_440_000_000))
        );
        assert_eq!(
            cost_nano_usd(model, 72_000, 100_000, Some(100_000), Some(100_000)),
            Some(Usd(1_414_000_000))
        );
        assert_eq!(
            cost_nano_usd(model, 72_001, 100_000, Some(100_000), Some(100_000)),
            Some(Usd(2_328_004_000))
        );
        assert_eq!(
            cost_nano_usd(model, i64::MAX, 0, Some(i64::MAX), Some(i64::MAX)),
            None
        );
    }

    #[test]
    fn large_counts_remain_exact_and_overflow_is_unknown() {
        let tokens = (1_i64 << 53) + 1;
        assert_eq!(
            cost_nano_usd(Some("jev-1.13.0"), tokens, 0, None, None),
            Some(Usd(tokens * 42))
        );
        assert_eq!(
            cost_nano_usd(Some("gpt-6-sol"), i64::MAX, 0, None, None),
            None
        );
        assert_eq!(
            cost_nano_usd(Some("gpt-6-sol"), -1, -1, Some(-1), Some(-1)),
            Some(Usd(0))
        );
    }

    #[test]
    fn usd_serializes_as_an_integer_and_checks_totals() {
        let usd = Usd::from_nano_usd(4_740_000);
        assert_eq!(serde_json::to_string(&usd).unwrap(), "4740000");
        assert_eq!(serde_json::from_str::<Usd>("4740000").unwrap(), usd);
        assert_eq!(usd.checked_add(usd), Some(Usd(9_480_000)));
        assert_eq!(Usd(i64::MAX).checked_add(Usd(1)), None);
        assert_eq!(Usd::from_scaled_nano_usd(1, 2), Some(Usd(1)));
        assert_eq!(Usd::from_scaled_nano_usd(-1, 2), Some(Usd(-1)));
        assert_eq!(Usd::from_scaled_nano_usd(1, 0), None);
        assert_eq!(
            Usd::from_scaled_nano_usd(i128::from(i64::MAX), 1),
            Some(Usd(i64::MAX))
        );
        assert_eq!(Usd::from_scaled_nano_usd(i128::from(i64::MAX) + 1, 1), None);
    }
}

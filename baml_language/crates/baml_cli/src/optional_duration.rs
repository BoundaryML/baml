use std::time::Duration;

/// A duration flag value that may also be `none` (no limit). Shared by
/// `--shutdown-timeout` and `--test-timeout` so they parse identically.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct OptionalDuration(pub(crate) Option<Duration>);

impl std::str::FromStr for OptionalDuration {
    type Err = String;

    /// Accepts `none`, or an integer followed by `ms`, `s` or `m`
    /// (`500ms`, `15s`, `2m`).
    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        let text = raw.trim().to_ascii_lowercase();
        if text == "none" {
            return Ok(Self(None));
        }
        let invalid = || {
            format!(
                "invalid duration `{raw}`: expected a number with a unit of ms, s or m (for example `15s`), or `none`"
            )
        };
        let split = text
            .find(|c: char| !c.is_ascii_digit())
            .ok_or_else(invalid)?;
        let (digits, unit) = text.split_at(split);
        let amount: u64 = digits.parse().map_err(|_| invalid())?;
        let duration = match unit {
            "ms" => Duration::from_millis(amount),
            "s" => Duration::from_secs(amount),
            "m" => Duration::from_secs(amount.checked_mul(60).ok_or_else(invalid)?),
            _ => return Err(invalid()),
        };
        Ok(Self(Some(duration)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(raw: &str) -> Result<Option<Duration>, String> {
        raw.parse::<OptionalDuration>().map(|d| d.0)
    }

    #[test]
    fn parses_units_and_none() {
        assert_eq!(parse("500ms"), Ok(Some(Duration::from_millis(500))));
        assert_eq!(parse("15s"), Ok(Some(Duration::from_secs(15))));
        assert_eq!(parse("2m"), Ok(Some(Duration::from_secs(120))));
        assert_eq!(parse(" 3S "), Ok(Some(Duration::from_secs(3))));
        assert_eq!(parse("NONE"), Ok(None));
        assert_eq!(parse("0s"), Ok(Some(Duration::ZERO)));
    }

    #[test]
    fn rejects_invalid_values() {
        for raw in ["", "15", "s", "1.5s", "-1s", "10h", "abc", "5 s", "0"] {
            assert!(parse(raw).is_err(), "{raw:?}");
        }
    }
}

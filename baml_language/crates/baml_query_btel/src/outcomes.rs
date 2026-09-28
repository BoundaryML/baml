//! Population outcomes are evidence carried by aggregate deltas, not inferred
//! from retained spans. Absence in an older delta must survive every rollup.
use rusqlite::{
    Connection,
    functions::{Aggregate, Context, FunctionFlags},
};

pub(crate) const RECORDED: i64 = 1;
pub(crate) const MISSING: i64 = 2;
pub(crate) const INVALID: i64 = 4;
const OVERFLOW: i64 = 8;

#[derive(Clone, Copy, Default)]
pub(crate) struct Totals {
    pub evidence: i64,
    pub errored: u128,
    pub cancelled: u128,
}

impl Totals {
    pub(crate) fn from_wire(
        count: u64,
        outcomes: Option<&btel_recorder::proto::AggregateOutcomes>,
    ) -> Self {
        let Some(outcomes) = outcomes else {
            return Self {
                evidence: MISSING,
                ..Self::default()
            };
        };
        let errored = u128::from(outcomes.errored);
        let cancelled = u128::from(outcomes.cancelled);
        if errored + cancelled > u128::from(count) {
            return Self {
                evidence: INVALID,
                ..Self::default()
            };
        }
        Self {
            evidence: RECORDED,
            errored,
            cancelled,
        }
    }

    pub(crate) fn add(self, other: Self) -> Self {
        Self {
            evidence: self.evidence | other.evidence,
            errored: self.errored.saturating_add(other.errored),
            cancelled: self.cancelled.saturating_add(other.cancelled),
        }
    }

    /// Only a fully recorded population supports numeric outcome counts.
    /// Keep exact totals internally so a later unsupported delta cannot
    /// turn a partial population into a misleading success/error rate.
    pub(crate) fn counts(self, count: u128) -> [Option<i64>; 3] {
        if self.evidence != RECORDED {
            return [None; 3];
        }
        let ok = self
            .errored
            .checked_add(self.cancelled)
            .and_then(|failed| count.checked_sub(failed));
        [
            ok.and_then(|n| i64::try_from(n).ok()),
            i64::try_from(self.errored).ok(),
            i64::try_from(self.cancelled).ok(),
        ]
    }
}

fn state(
    evidence: i64,
    count: Option<i64>,
    errored: Option<i64>,
    cancelled: Option<i64>,
) -> &'static str {
    if evidence & INVALID != 0 {
        "invalid"
    } else if evidence & MISSING != 0 {
        if evidence & RECORDED != 0 {
            "partial"
        } else {
            "not_recorded"
        }
    } else if evidence == 0 {
        "none_observed"
    } else if evidence & OVERFLOW != 0
        || count.is_none()
        || errored.is_none()
        || cancelled.is_none()
    {
        "overflow"
    } else {
        "recorded"
    }
}

struct Evidence;
impl Aggregate<i64, i64> for Evidence {
    fn init(&self, _: &mut Context<'_>) -> rusqlite::Result<i64> {
        Ok(0)
    }
    fn step(&self, ctx: &mut Context<'_>, evidence: &mut i64) -> rusqlite::Result<()> {
        *evidence |= match ctx.get::<String>(0)?.as_str() {
            "none_observed" => 0,
            "recorded" => RECORDED,
            "not_recorded" => MISSING,
            "partial" => RECORDED | MISSING,
            "overflow" => RECORDED | OVERFLOW,
            _ => INVALID,
        };
        Ok(())
    }
    fn finalize(&self, _: &mut Context<'_>, evidence: Option<i64>) -> rusqlite::Result<i64> {
        Ok(evidence.unwrap_or(0))
    }
}

pub(crate) fn register(conn: &Connection) -> rusqlite::Result<()> {
    let pure = FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC;
    conn.create_scalar_function("__btel_outcome_state", 4, pure, |ctx| {
        Ok(state(ctx.get(0)?, ctx.get(1)?, ctx.get(2)?, ctx.get(3)?))
    })?;
    conn.create_aggregate_function("__btel_outcome_evidence", 1, pure, Evidence)
}

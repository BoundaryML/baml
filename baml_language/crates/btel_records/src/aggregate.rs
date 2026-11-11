use btel_types::{AwaitDuration, CallPathNodeId, ClockDuration};

/// Raw sums retain the call path's immutable clock context. Never add these
/// across paths/epochs before conversion. Missing clock metadata is unresolved,
/// not a zero duration. Counts remain usable if the epoch is later invalidated.
///
/// `errored` and `cancelled` count completions by outcome within `count`; the
/// rest succeeded. Their sum never exceeds `count`. `panicked` counts the
/// errored completions a panic ended.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(C)]
pub struct AggregateDelta {
    pub node: CallPathNodeId,
    pub count: u64,
    pub total_duration: ClockDuration,
    /// Time the nodes' own frames spent awaiting.
    pub total_self_await: AwaitDuration,
    pub errored: u64,
    pub cancelled: u64,
    pub panicked: u64,
}

const _: () =
    assert!(std::mem::size_of::<AggregateDelta>() == btel_settings::layout::AGGREGATE_DELTA_BYTES);

impl AggregateDelta {
    /// Contribution to the base path's inclusive duration. The node's actual
    /// measured duration remains available in `total_duration`, including reentry.
    pub fn inclusive_duration(&self) -> ClockDuration {
        if self.node.is_reentry() {
            ClockDuration::ZERO
        } else {
            self.total_duration
        }
    }

    /// Combine samples for the same node without losing counts on overflow.
    /// Different nodes or an overflowing sum return `None`; the caller must
    /// retain and publish both deltas separately.
    pub fn checked_merge(self, other: Self) -> Option<Self> {
        if self.node != other.node {
            return None;
        }
        Some(Self {
            node: self.node,
            count: self.count.checked_add(other.count)?,
            total_duration: ClockDuration::from_ticks(
                self.total_duration
                    .get()
                    .checked_add(other.total_duration.get())?,
            ),
            total_self_await: AwaitDuration::ZERO.saturating_add(ClockDuration::from_ticks(
                self.total_self_await
                    .get()
                    .get()
                    .checked_add(other.total_self_await.get().get())?,
            )),
            errored: self.errored.checked_add(other.errored)?,
            cancelled: self.cancelled.checked_add(other.cancelled)?,
            panicked: self.panicked.checked_add(other.panicked)?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use btel_types::CallPathId;

    fn delta(reentry: bool) -> AggregateDelta {
        AggregateDelta {
            node: CallPathNodeId::new(CallPathId::new_non_root(1).unwrap(), reentry),
            count: 1,
            total_duration: ClockDuration::from_ticks(10),
            total_self_await: AwaitDuration::ZERO.saturating_add(ClockDuration::from_ticks(2)),
            errored: 1,
            panicked: 1,
            cancelled: 0,
        }
    }

    #[test]
    fn merge_rejects_other_nodes_and_overflow_without_saturation() {
        let base = delta(false);
        assert!(base.checked_merge(delta(true)).is_none());
        for field in 0..6 {
            let mut full = base;
            match field {
                0 => full.count = u64::MAX,
                1 => full.total_duration = ClockDuration::from_ticks(u64::MAX),
                2 => {
                    full.total_self_await =
                        AwaitDuration::ZERO.saturating_add(ClockDuration::from_ticks(u64::MAX));
                }
                3 => full.errored = u64::MAX,
                4 => {
                    full.cancelled = u64::MAX;
                }
                _ => full.panicked = u64::MAX,
            }
            let mut next = base;
            if field == 4 {
                next.cancelled = 1;
            }
            assert!(full.checked_merge(next).is_none(), "field {field}");
        }
        let merged = base.checked_merge(base).unwrap();
        assert_eq!(merged.count, 2);
        assert_eq!(merged.total_duration.get(), 20);
        assert_eq!(merged.total_self_await.get().get(), 4);
        assert_eq!(delta(true).inclusive_duration(), ClockDuration::ZERO);
        assert_eq!(merged.inclusive_duration().get(), 20);
    }
}

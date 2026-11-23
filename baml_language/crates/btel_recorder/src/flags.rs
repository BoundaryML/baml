//! Completion flags are `btel-core`'s (`btel_core::recording::CompletionFlags`), the one
//! definition of their bits; what is here maps the runtime's outcomes onto the wire's.

use btel_core::recording::CompletionFlags;
use btel_types::InvocationOutcome;

use crate::proto;

pub(crate) const fn outcome(value: InvocationOutcome) -> proto::InvocationOutcome {
    match value {
        InvocationOutcome::Ok => proto::InvocationOutcome::Ok,
        InvocationOutcome::Errored | InvocationOutcome::Panicked => {
            proto::InvocationOutcome::Errored
        }
        InvocationOutcome::Cancelled => proto::InvocationOutcome::Cancelled,
    }
}

/// The flags of a function that completed with `outcome`, which `requires_announcement` when
/// its announcement must be read before it.
pub(crate) const fn completed(
    outcome: proto::InvocationOutcome,
    requires_announcement: bool,
) -> u32 {
    match CompletionFlags::completed(outcome, requires_announcement, false) {
        Some(flags) => flags.bits(),
        None => panic!("a completion has an outcome"),
    }
}

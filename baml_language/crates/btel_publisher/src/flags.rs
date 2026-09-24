use btel_types::InvocationOutcome;

use crate::proto;

/// Explicit v1 wire flags. Outcome occupies bits 0..1; bit 2 is deferred value
/// capture and bit 3 is announcement dependency. Reentry lives in the node ID.
/// Never derived from Rust enum discriminants. Late spans forbid bit 3.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CompletionFlags(u8);
impl CompletionFlags {
    pub(crate) const fn from_variant(base: u8, deferred: bool) -> Self {
        Self(base | ((deferred as u8) << 2))
    }
    pub const fn bits(self) -> u32 {
        self.0 as u32
    }

    /// Validate flags read from a protobuf; reject unknown bits and invalid
    /// outcomes instead of silently interpreting a future format as v1.
    #[expect(
        clippy::verbose_bit_mask,
        reason = "mirror the two-bit outcome field in the wire contract"
    )]
    pub fn from_wire(bits: u32, late: bool) -> Option<Self> {
        if bits & !15 != 0 || bits & 3 == 0 || (late && bits & 8 != 0) {
            None
        } else {
            Some(Self(u8::try_from(bits).expect("four-bit value")))
        }
    }
    pub const fn outcome(self) -> InvocationOutcome {
        match self.0 & 3 {
            1 => InvocationOutcome::Ok,
            2 => InvocationOutcome::Errored,
            3 => InvocationOutcome::Cancelled,
            _ => unreachable!(),
        }
    }
    pub const fn capture_deferred(self) -> bool {
        self.0 & 4 != 0
    }
    pub const fn requires_announcement(self) -> bool {
        self.0 & 8 != 0
    }
}

pub(crate) const fn outcome(value: InvocationOutcome) -> proto::InvocationOutcome {
    match value {
        InvocationOutcome::Ok => proto::InvocationOutcome::Ok,
        InvocationOutcome::Errored => proto::InvocationOutcome::Errored,
        InvocationOutcome::Cancelled => proto::InvocationOutcome::Cancelled,
    }
}

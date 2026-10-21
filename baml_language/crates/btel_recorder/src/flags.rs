use btel_types::InvocationOutcome;

use crate::proto;

/// Explicit v2 wire flags. Outcome occupies bits 0..1; bit 3 is announcement
/// dependency. Capture IDs live in their own optional fields. Reentry lives in the node ID.
/// Never derived from Rust enum discriminants. Late spans forbid bit 3.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CompletionFlags(u8);
impl CompletionFlags {
    pub(crate) const fn from_variant(base: u8) -> Self {
        Self(base)
    }
    pub const fn bits(self) -> u32 {
        self.0 as u32
    }

    /// Validate flags read from a protobuf; reject unknown bits and invalid
    /// outcomes instead of silently interpreting a future format as v2.
    #[expect(
        clippy::verbose_bit_mask,
        reason = "mirror the two-bit outcome field in the wire contract"
    )]
    pub fn from_wire(bits: u32, late: bool) -> Option<Self> {
        let manual = bits & 32 != 0;
        let closed = bits & 16 != 0;
        if bits & !0b11_1011 != 0
            || (closed && bits & 3 != 0)
            || (!closed && bits & 3 == 0)
            || (late && (bits & 8 != 0 || manual || closed))
        {
            None
        } else {
            Some(Self(u8::try_from(bits).expect("six-bit value")))
        }
    }
    pub const fn is_manual(self) -> bool {
        self.0 & 32 != 0
    }
    pub const fn is_closed(self) -> bool {
        self.0 & 16 != 0
    }
    pub const fn outcome(self) -> InvocationOutcome {
        match self.0 & 3 {
            1 => InvocationOutcome::Ok,
            2 => InvocationOutcome::Errored,
            3 => InvocationOutcome::Cancelled,
            _ => unreachable!(),
        }
    }
    pub const fn requires_announcement(self) -> bool {
        self.0 & 8 != 0
    }
}

pub(crate) const fn outcome(value: InvocationOutcome) -> proto::InvocationOutcome {
    match value {
        InvocationOutcome::Ok => proto::InvocationOutcome::Ok,
        InvocationOutcome::Errored | InvocationOutcome::Panicked => {
            proto::InvocationOutcome::Errored
        }
        InvocationOutcome::Cancelled => proto::InvocationOutcome::Cancelled,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn neutral_close_is_general_and_requires_no_outcome_bits() {
        for bits in [16, 24, 48, 56] {
            let flags = CompletionFlags::from_wire(bits, false).unwrap();
            assert_eq!(flags.is_manual(), bits & 32 != 0);
            assert!(flags.is_closed());
            assert_eq!(flags.requires_announcement(), bits & 8 != 0);
            assert!(CompletionFlags::from_wire(bits, true).is_none());
        }
        for bits in [0, 32, 40, 17, 18, 19, 49, 50, 51, 64] {
            assert!(CompletionFlags::from_wire(bits, false).is_none(), "{bits}");
        }
        for bits in [33, 34, 35, 41, 42, 43] {
            let flags = CompletionFlags::from_wire(bits, false).unwrap();
            assert!(flags.is_manual());
            assert!(!flags.is_closed());
        }
    }
}

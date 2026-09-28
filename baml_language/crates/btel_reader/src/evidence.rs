//! Interpretation of individual evidence records. Reconciliation state lives
//! with the caller; these rules are shared by every reader.
use btel_recorder::{CompletionFlags, proto};

/// Observed invocation outcome.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Outcome {
    Ok = 1,
    Errored = 2,
    Cancelled = 3,
}

impl Outcome {
    pub fn from_code(code: i64) -> Option<Self> {
        match code {
            1 => Some(Self::Ok),
            2 => Some(Self::Errored),
            3 => Some(Self::Cancelled),
            _ => None,
        }
    }
    pub fn code(self) -> i64 {
        self as i64
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Errored => "errored",
            Self::Cancelled => "cancelled",
        }
    }
    pub fn from_thread(outcome: i32) -> Option<Self> {
        Self::from_code(i64::from(outcome))
    }
}

/// A validated completion's outcome and whether an announcement carries its
/// captured inputs.
pub fn completion(done: &proto::FunctionCompletion, late: bool) -> Option<(Outcome, bool)> {
    let flags = CompletionFlags::from_wire(done.completion_flags, late)?;
    // Bits 0..1 hold the outcome; from_wire rejected zero.
    let outcome = Outcome::from_code(i64::from(flags.bits() & 3))?;
    Some((outcome, flags.requires_announcement()))
}

/// `(call path, reentry)` from an aggregate/completion node.
pub fn split_node(node: u64) -> (u32, bool) {
    (
        u32::try_from(node >> 1).expect("validated node fits u32"),
        node & 1 == 1,
    )
}

/// A captured-argument slot name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SlotName {
    pub name: Option<String>,
    pub receiver: bool,
}

/// Recorded names for `FunctionArgs` slots.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ArgumentNames {
    pub slots: Vec<SlotName>,
}

impl ArgumentNames {
    pub fn from_wire(layout: &proto::ArgumentLayout) -> Self {
        Self {
            slots: layout
                .slots
                .iter()
                .map(|slot| SlotName {
                    name: slot.name.clone(),
                    receiver: slot.receiver,
                })
                .collect(),
        }
    }

    /// Slot index for a declared parameter name.
    pub fn position(&self, name: &str) -> Option<usize> {
        self.slots
            .iter()
            .position(|slot| slot.name.as_deref() == Some(name))
    }

    /// Compact stable encoding: u16 count, then per slot a flags byte
    /// (bit 0 named, bit 1 receiver), and a u16-length UTF-8 name if named.
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(2 + self.slots.len() * 12);
        let count = u16::try_from(self.slots.len()).unwrap_or(u16::MAX);
        out.extend_from_slice(&count.to_le_bytes());
        for slot in self.slots.iter().take(usize::from(count)) {
            let name = slot.name.as_deref().map(|name| {
                let len = name.len().min(usize::from(u16::MAX));
                &name.as_bytes()[..len]
            });
            out.push(u8::from(name.is_some()) | (u8::from(slot.receiver) << 1));
            if let Some(name) = name {
                let len = u16::try_from(name.len()).expect("name clipped to u16::MAX");
                out.extend_from_slice(&len.to_le_bytes());
                out.extend_from_slice(name);
            }
        }
        out
    }

    pub fn decode(mut bytes: &[u8]) -> Option<Self> {
        let mut take = |n: usize| -> Option<&[u8]> {
            let (head, tail) = bytes.split_at_checked(n)?;
            bytes = tail;
            Some(head)
        };
        let count = u16::from_le_bytes(take(2)?.try_into().ok()?);
        let mut slots = Vec::with_capacity(usize::from(count));
        for _ in 0..count {
            let flags = take(1)?[0];
            let name = if flags & 1 == 1 {
                let len = u16::from_le_bytes(take(2)?.try_into().ok()?);
                Some(String::from_utf8(take(usize::from(len))?.to_vec()).ok()?)
            } else {
                None
            };
            slots.push(SlotName {
                name,
                receiver: flags & 2 == 2,
            });
        }
        Some(Self { slots })
    }
}

pub fn function_kind_label(kind: i32) -> Option<&'static str> {
    Some(match proto::FunctionKind::try_from(kind).ok()? {
        proto::FunctionKind::Bytecode => "bytecode",
        // The old catalog's spelling.
        proto::FunctionKind::SysOp => "sysop",
        proto::FunctionKind::Native => "native",
        proto::FunctionKind::NativeUnresolved => "native_unresolved",
        proto::FunctionKind::Unspecified => return None,
    })
}

pub fn function_origin_label(origin: i32) -> Option<&'static str> {
    Some(match proto::FunctionOrigin::try_from(origin).ok()? {
        proto::FunctionOrigin::UserDefined => "user",
        proto::FunctionOrigin::Companion => "companion",
        proto::FunctionOrigin::Internal => "internal",
        proto::FunctionOrigin::Builtin => "builtin",
        proto::FunctionOrigin::AutoDerive => "auto_derive",
        proto::FunctionOrigin::Unspecified => return None,
    })
}

/// Two definitions of the same function agree on everything the reader
/// exposes. Metadata is immutable, so disagreement is a conflict to report.
pub fn same_metadata(a: &proto::FunctionMetadata, b: &proto::FunctionMetadata) -> bool {
    a == b
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn argument_names_round_trip_including_unnamed_and_receiver_slots() {
        let names = ArgumentNames {
            slots: vec![
                SlotName {
                    name: Some("self".into()),
                    receiver: true,
                },
                SlotName {
                    name: None,
                    receiver: false,
                },
                SlotName {
                    name: Some("customer".into()),
                    receiver: false,
                },
            ],
        };
        let encoded = names.encode();
        assert_eq!(ArgumentNames::decode(&encoded), Some(names.clone()));
        assert_eq!(names.position("customer"), Some(2));
        assert_eq!(names.position("missing"), None);
        assert_eq!(ArgumentNames::decode(&encoded[..encoded.len() - 1]), None);
        let empty = ArgumentNames::default();
        assert_eq!(ArgumentNames::decode(&empty.encode()), Some(empty));
    }
}

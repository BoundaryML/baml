use sha2::{Digest, Sha256};

use crate::{
    ids::{FunctionId, ProgramId},
    prof::record::CallSiteSourceSpan,
};

const CONTEXT_DOMAIN: &[u8] = b"baml-cct-context-v1";
const VALUE_DOMAIN: &[u8] = b"baml-value-v1";

/// Whether a call receives the ordinary or outer user-visible LLM base policy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FunctionCaptureClass {
    Ordinary,
    Llm,
}

/// Tri-state role overrides accumulated by one `boundary.LocalId` before it
/// is consumed at a call site.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LocalIdOverrides {
    pub inputs: Option<bool>,
    pub output: Option<bool>,
    pub error: Option<bool>,
}

/// Enabled evidence value roles. Only the low three bits are valid.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RoleMask(u8);

impl RoleMask {
    pub const INPUT: u8 = 1 << 0;
    pub const OUTPUT: u8 = 1 << 1;
    pub const ERROR: u8 = 1 << 2;
    pub const ALL: Self = Self(Self::INPUT | Self::OUTPUT | Self::ERROR);
    pub const NONE: Self = Self(0);

    #[must_use]
    pub const fn inputs(self) -> bool {
        self.0 & Self::INPUT != 0
    }

    #[must_use]
    pub const fn output(self) -> bool {
        self.0 & Self::OUTPUT != 0
    }

    #[must_use]
    pub const fn error(self) -> bool {
        self.0 & Self::ERROR != 0
    }

    #[must_use]
    pub const fn bits(self) -> u8 {
        self.0
    }

    pub const fn from_bits(bits: u8) -> Option<Self> {
        if bits & !0b111 == 0 {
            Some(Self(bits))
        } else {
            None
        }
    }

    fn set(&mut self, bit: u8, enabled: bool) {
        if enabled {
            self.0 |= bit;
        } else {
            self.0 &= !bit;
        }
    }
}

/// Independent reasons why an exact span was selected.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SelectionReasons(u8);

impl SelectionReasons {
    pub const ROOT: u8 = 1 << 0;
    pub const LLM: u8 = 1 << 1;
    pub const MANUAL: u8 = 1 << 2;
    pub const NONE: Self = Self(0);

    #[must_use]
    pub const fn root(self) -> bool {
        self.0 & Self::ROOT != 0
    }

    #[must_use]
    pub const fn llm(self) -> bool {
        self.0 & Self::LLM != 0
    }

    #[must_use]
    pub const fn manual(self) -> bool {
        self.0 & Self::MANUAL != 0
    }

    #[must_use]
    pub const fn bits(self) -> u8 {
        self.0
    }

    pub const fn from_bits(bits: u8) -> Option<Self> {
        if bits & !0b111 == 0 {
            Some(Self(bits))
        } else {
            None
        }
    }

    fn insert(&mut self, reason: u8) {
        self.0 |= reason;
    }
}

/// Central, host-independent exact-evidence decision made before call start.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CapturePlan {
    pub selected: bool,
    pub roles: RoleMask,
    pub reasons: SelectionReasons,
}

/// Invalid producer flags. Bit seven is reserved by the v1 contract.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CapturePlanDecodeError {
    ReservedBitSet,
    InconsistentSelection,
}

impl CapturePlan {
    const SELECTED: u8 = 1 << 3;
    const ROOT: u8 = 1 << 4;
    const LLM: u8 = 1 << 5;
    const MANUAL: u8 = 1 << 6;
    const RESERVED: u8 = 1 << 7;

    /// Encodes the complete plan into the reserved `CallFunction.flags` byte.
    #[must_use]
    pub const fn to_call_flags(self) -> u8 {
        let mut flags = self.roles.bits();
        if self.selected {
            flags |= Self::SELECTED;
        }
        if self.reasons.root() {
            flags |= Self::ROOT;
        }
        if self.reasons.llm() {
            flags |= Self::LLM;
        }
        if self.reasons.manual() {
            flags |= Self::MANUAL;
        }
        flags
    }

    /// Decodes and validates one producer flag byte.
    pub const fn from_call_flags(flags: u8) -> Result<Self, CapturePlanDecodeError> {
        if flags & Self::RESERVED != 0 {
            return Err(CapturePlanDecodeError::ReservedBitSet);
        }
        let selected = flags & Self::SELECTED != 0;
        let reason_bits = (flags >> 4) & 0b111;
        if selected != (reason_bits != 0) {
            return Err(CapturePlanDecodeError::InconsistentSelection);
        }
        Ok(Self {
            selected,
            roles: RoleMask(flags & 0b111),
            reasons: SelectionReasons(reason_bits),
        })
    }
}

/// Resolves root, LLM, and explicit-`LocalId` policy in the canonical order.
#[must_use]
pub fn resolve_capture_plan(
    is_boundary_root: bool,
    capture_class: FunctionCaptureClass,
    local_id: Option<LocalIdOverrides>,
) -> CapturePlan {
    let mut plan = CapturePlan::default();
    if is_boundary_root {
        plan.selected = true;
        plan.roles = RoleMask::ALL;
        plan.reasons.insert(SelectionReasons::ROOT);
    }
    if capture_class == FunctionCaptureClass::Llm {
        plan.selected = true;
        plan.roles = RoleMask::ALL;
        plan.reasons.insert(SelectionReasons::LLM);
    }
    if let Some(overrides) = local_id {
        plan.selected = true;
        plan.reasons.insert(SelectionReasons::MANUAL);
        if let Some(enabled) = overrides.inputs {
            plan.roles.set(RoleMask::INPUT, enabled);
        }
        if let Some(enabled) = overrides.output {
            plan.roles.set(RoleMask::OUTPUT, enabled);
        }
        if let Some(enabled) = overrides.error {
            plan.roles.set(RoleMask::ERROR, enabled);
        }
    }
    plan
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum EdgeKind {
    Root = 0,
    Call = 1,
    Spawn = 2,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ContextKey(pub [u8; 32]);

/// Canonical CCT identity tuple. Runtime IDs and invocation IDs are
/// intentionally absent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ContextTuple {
    pub program_id: ProgramId,
    pub parent_context_key: Option<ContextKey>,
    pub function_id: FunctionId,
    pub call_site: Option<CallSiteSourceSpan>,
    pub edge_kind: EdgeKind,
}

impl ContextKey {
    /// Hashes a fixed-endian, tag-explicit v1 tuple.
    #[must_use]
    pub fn for_tuple(tuple: &ContextTuple) -> Self {
        let mut hash = Sha256::new();
        hash.update(CONTEXT_DOMAIN);
        hash.update(tuple.program_id.0);
        match tuple.parent_context_key {
            None => hash.update([0]),
            Some(parent) => {
                hash.update([1]);
                hash.update(parent.0);
            }
        }
        hash.update(tuple.function_id.0.to_be_bytes());
        match tuple.call_site {
            None => hash.update([0]),
            Some(site) => {
                hash.update([1]);
                hash.update(site.file_id.to_be_bytes());
                hash.update(site.start_offset.to_be_bytes());
                hash.update(site.end_offset.to_be_bytes());
                hash.update(site.line.to_be_bytes());
            }
        }
        hash.update([tuple.edge_kind as u8]);
        Self(hash.finalize().into())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CodecVersion(pub u16);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ValueCid(pub [u8; 32]);

impl ValueCid {
    /// Byte-identity hash for one complete encoded value body.
    #[must_use]
    pub fn for_encoded(codec: CodecVersion, encoded_body: &[u8]) -> Self {
        let mut hash = Sha256::new();
        hash.update(VALUE_DOMAIN);
        hash.update(codec.0.to_be_bytes());
        hash.update(encoded_body);
        Self(hash.finalize().into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordinary_manual_input_only_is_selected_input_only() {
        let plan = resolve_capture_plan(
            false,
            FunctionCaptureClass::Ordinary,
            Some(LocalIdOverrides {
                inputs: Some(true),
                ..LocalIdOverrides::default()
            }),
        );
        assert!(plan.selected);
        assert_eq!(plan.roles, RoleMask(RoleMask::INPUT));
        assert_eq!(plan.reasons, SelectionReasons(SelectionReasons::MANUAL));
        assert_eq!(plan.to_call_flags(), 0b0100_1001);
        assert_eq!(CapturePlan::from_call_flags(plan.to_call_flags()), Ok(plan));
    }

    #[test]
    fn llm_overrides_apply_after_base_policy() {
        let plan = resolve_capture_plan(
            false,
            FunctionCaptureClass::Llm,
            Some(LocalIdOverrides {
                output: Some(false),
                ..LocalIdOverrides::default()
            }),
        );
        assert!(plan.roles.inputs());
        assert!(!plan.roles.output());
        assert!(plan.roles.error());
        assert!(plan.reasons.llm());
        assert!(plan.reasons.manual());
    }

    #[test]
    fn metadata_only_manual_plan_has_no_roles() {
        let plan = resolve_capture_plan(
            false,
            FunctionCaptureClass::Ordinary,
            Some(LocalIdOverrides::default()),
        );
        assert!(plan.selected);
        assert_eq!(plan.roles, RoleMask::NONE);
    }

    #[test]
    fn root_llm_manual_reasons_accumulate() {
        let plan = resolve_capture_plan(
            true,
            FunctionCaptureClass::Llm,
            Some(LocalIdOverrides {
                error: Some(false),
                ..LocalIdOverrides::default()
            }),
        );
        assert_eq!(plan.reasons.bits(), 0b111);
        assert!(!plan.roles.error());
        assert_eq!(CapturePlan::from_call_flags(plan.to_call_flags()), Ok(plan));
    }

    #[test]
    fn call_flags_reject_reserved_and_inconsistent_bits() {
        assert_eq!(
            CapturePlan::from_call_flags(0x80),
            Err(CapturePlanDecodeError::ReservedBitSet)
        );
        assert_eq!(
            CapturePlan::from_call_flags(CapturePlan::SELECTED),
            Err(CapturePlanDecodeError::InconsistentSelection)
        );
        assert_eq!(
            CapturePlan::from_call_flags(CapturePlan::ROOT),
            Err(CapturePlanDecodeError::InconsistentSelection)
        );
    }

    #[test]
    fn context_key_golden_is_stable() {
        let tuple = ContextTuple {
            program_id: ProgramId([0x11; 16]),
            parent_context_key: Some(ContextKey([0x22; 32])),
            function_id: FunctionId(0x0102_0304),
            call_site: Some(CallSiteSourceSpan {
                file_id: 5,
                start_offset: 6,
                end_offset: 7,
                line: 8,
            }),
            edge_kind: EdgeKind::Spawn,
        };
        assert_eq!(
            hex::encode(ContextKey::for_tuple(&tuple).0),
            "a7ae20370bed3ef26edf6a067c9b14dae5feba1bc870eb5a4e4be55616034d9b"
        );
    }

    #[test]
    fn value_cid_golden_and_domain_separation_are_stable() {
        let cid = ValueCid::for_encoded(CodecVersion(1), b"\x00baml-value-body\xff");
        assert_eq!(
            hex::encode(cid.0),
            "fe620b74bb3027bd1bba4d70cde577cb5b4e0238398f1d97df27dbcb14b29944"
        );
        assert_ne!(
            cid,
            ValueCid::for_encoded(CodecVersion(2), b"\x00baml-value-body\xff")
        );
    }
}

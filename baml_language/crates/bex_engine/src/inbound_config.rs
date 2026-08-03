use std::sync::OnceLock;

/// Process-wide policy for an unannotated inbound value that inhabits more
/// than one member of its declared union.
///
/// A loaded host bridge is process-global, so this is registered once with the
/// bridge identity rather than carried on every call. Direct engine users that
/// do not register a host bridge retain the strict default.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum InboundUnionAmbiguityPolicy {
    #[default]
    Reject,
    SelectDefault,
}

static INBOUND_UNION_AMBIGUITY_POLICY: OnceLock<InboundUnionAmbiguityPolicy> = OnceLock::new();

/// Register the process-wide inbound ambiguity policy.
///
/// Repeating the same registration is idempotent. A conflicting policy means
/// two incompatible host bridges attempted to configure one runtime process.
pub fn register_inbound_union_ambiguity_policy(
    requested: InboundUnionAmbiguityPolicy,
) -> Result<(), String> {
    if let Some(existing) = INBOUND_UNION_AMBIGUITY_POLICY.get() {
        return if *existing == requested {
            Ok(())
        } else {
            Err(format!(
                "inbound union ambiguity policy is already registered as {existing:?}; cannot register {requested:?}"
            ))
        };
    }

    if INBOUND_UNION_AMBIGUITY_POLICY.set(requested).is_err() {
        let existing = INBOUND_UNION_AMBIGUITY_POLICY
            .get()
            .expect("inbound ambiguity policy was initialized concurrently");
        return if *existing == requested {
            Ok(())
        } else {
            Err(format!(
                "inbound union ambiguity policy is already registered as {existing:?}; cannot register {requested:?}"
            ))
        };
    }
    Ok(())
}

pub(crate) fn inbound_union_ambiguity_policy() -> InboundUnionAmbiguityPolicy {
    INBOUND_UNION_AMBIGUITY_POLICY
        .get()
        .copied()
        .unwrap_or_default()
}

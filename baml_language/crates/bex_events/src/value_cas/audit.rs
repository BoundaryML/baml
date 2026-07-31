use super::PromotionAudit;

/// Audit payload emitted when effective capture policy changes while a
/// boundary is open. Policy snapshots are opaque, versioned host strings so
/// this storage foundation does not duplicate compiler policy types.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CapturePolicyChangedAudit {
    pub timestamp_ms: u64,
    pub scope: String,
    pub previous_policy: String,
    pub current_policy: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ValueAuditRecord {
    CapturePolicyChanged(CapturePolicyChangedAudit),
    PromotionOccurred(PromotionAudit),
}

impl From<PromotionAudit> for ValueAuditRecord {
    fn from(value: PromotionAudit) -> Self {
        Self::PromotionOccurred(value)
    }
}

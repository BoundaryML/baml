use baml_base::Name;

use crate::loc::RetryPolicyLoc;

/// Semantic data for a `retry_policy` declaration.
///
/// Carries neither type expressions nor spans, so there is no source-map twin —
/// the data query alone is the firewall. Values stay as raw strings; they are
/// parsed at emit time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetryPolicyData {
    pub name: Name,
    pub max_retries: Option<String>,
    pub initial_delay_ms: Option<String>,
    pub multiplier: Option<String>,
    pub max_delay_ms: Option<String>,
}

#[salsa::tracked(returns(ref))]
pub fn retry_policy_data<'db>(
    db: &'db dyn crate::Db,
    policy: RetryPolicyLoc<'db>,
) -> RetryPolicyData {
    let item_tree = crate::file_item_tree(db, policy.file(db));
    let data = &item_tree[policy.id(db)];

    RetryPolicyData {
        name: data.name.clone(),
        max_retries: data.max_retries.clone(),
        initial_delay_ms: data.initial_delay_ms.clone(),
        multiplier: data.multiplier.clone(),
        max_delay_ms: data.max_delay_ms.clone(),
    }
}

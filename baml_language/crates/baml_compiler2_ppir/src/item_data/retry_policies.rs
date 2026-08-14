use baml_base::Name;
use baml_compiler2_hir::{item_tree::ItemSpans, loc::RetryPolicyLoc};

/// Semantic data for a `retry_policy` declaration.
///
/// Carries no type expressions, so there is no `TypeRefStore`; spans live in
/// the [`retry_policy_source_map`] twin. Values stay as raw strings; they are
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

/// Declaration and name-token spans for one retry policy. Kept separate from
/// [`retry_policy_data`] so a whitespace-only edit invalidates this but not
/// the semantic data.
#[salsa::tracked(returns(ref))]
pub fn retry_policy_source_map<'db>(
    db: &'db dyn crate::Db,
    policy: RetryPolicyLoc<'db>,
) -> ItemSpans {
    let item_source_map = crate::file_item_tree_source_map(db, policy.file(db));
    item_source_map
        .retry_policy_spans
        .get(&policy.id(db))
        .copied()
        .unwrap_or_else(|| unreachable!("spans recorded at allocation"))
}

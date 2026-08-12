use baml_base::Name;
use baml_compiler2_hir::{item_tree::ItemSpans, loc::ClientLoc};

/// Semantic data for a `client` declaration.
///
/// Clients carry no type expressions, so there is no `TypeRefStore`; spans
/// live in the [`client_source_map`] twin.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientData {
    pub name: Name,
    /// Provider name (e.g. "openai", "anthropic", "fallback", "round-robin").
    pub provider: Option<Name>,
    /// Retry policy name, if configured.
    pub retry_policy_name: Option<Name>,
}

#[salsa::tracked(returns(ref))]
pub fn client_data<'db>(db: &'db dyn crate::Db, client: ClientLoc<'db>) -> ClientData {
    let item_tree = crate::file_item_tree(db, client.file(db));
    let data = &item_tree[client.id(db)];

    ClientData {
        name: data.name.clone(),
        provider: data.provider.clone(),
        retry_policy_name: data.retry_policy_name.clone(),
    }
}

/// Declaration and name-token spans for one client. Kept separate from
/// [`client_data`] so a whitespace-only edit invalidates this but not the
/// semantic data.
#[salsa::tracked(returns(ref))]
pub fn client_source_map<'db>(db: &'db dyn crate::Db, client: ClientLoc<'db>) -> ItemSpans {
    let item_source_map = crate::file_item_tree_source_map(db, client.file(db));
    item_source_map
        .client_spans
        .get(&client.id(db))
        .copied()
        .unwrap_or_else(|| unreachable!("spans recorded at allocation"))
}

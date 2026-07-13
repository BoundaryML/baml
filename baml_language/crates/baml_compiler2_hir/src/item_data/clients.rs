use baml_base::Name;

use crate::loc::ClientLoc;

/// Semantic data for a `client` declaration.
///
/// Clients carry neither type expressions nor spans, so there is no
/// `TypeRefStore` and no source-map twin — the data query alone is the firewall.
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

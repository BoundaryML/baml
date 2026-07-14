//! Inbound (host → engine) wire construction helpers for generated code.
//!
//! The inbound path never serializes in-process: these build the prost
//! structs that feed `bridge_ctypes::kwargs_to_bex_values` directly.

use crate::wire::{self, inbound_map_entry::Key};

/// Build the kwargs list for a function call. An entry whose value is
/// `None` is omitted entirely — that is how [`OptionalArg::Unset`] makes
/// the engine evaluate the parameter's BAML default (distinct from an
/// explicit null, which rides as a value-less `InboundValue`).
///
/// [`OptionalArg::Unset`]: crate::OptionalArg::Unset
pub fn kwargs(entries: Vec<(&str, Option<wire::InboundValue>)>) -> Vec<wire::InboundMapEntry> {
    entries
        .into_iter()
        .filter_map(|(name, value)| {
            value.map(|v| wire::InboundMapEntry {
                key: Some(Key::StringKey(name.to_string())),
                value: Some(v),
            })
        })
        .collect()
}

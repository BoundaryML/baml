//! IDE-layer data → LSP protocol types.
//!
//! `baml_ide` returns plain data (files, byte ranges, structured
//! [`TypeInfo`]); this module owns the presentation: URIs through the
//! stdlib-aware [`crate::roots::RootsView`], positions through the session's
//! negotiated [`PositionCodec`], and hover *markdown* — deliberately a
//! protocol-layer rendering of `TypeInfo`, never produced by the ide crate.

use baml_ide::{DefinitionKind, OutlineItem, TypeInfo};
use lsp_types::{DocumentSymbol, SymbolKind};

use crate::{paths, position_codec::PositionCodec, snapshot::Snapshot};

/// The LSP location for an ide-layer location, in the session's encoding.
/// `None` when the target has no client-openable presentation (a stdlib file
/// without a materialized directory).
pub(super) fn location(
    snap: &Snapshot,
    target: baml_ide::resolve::Location,
) -> Option<lsp_types::Location> {
    let db = snap.db();
    let uri = paths::uri_for_db_path(snap.roots(), &target.file.path(db))?;
    let codec = PositionCodec::new(target.file.text(db), snap.cx().encoding);
    Some(lsp_types::Location {
        uri,
        range: codec.byte_range_to_lsp(target.range),
    })
}

/// Hover markdown for a [`TypeInfo`]: the describe block in a `baml` fence,
/// docstring lines inside the fence, and — for a class with methods — a
/// pointer at `baml describe` (hover never inlines method lists).
pub(super) fn hover_markdown(info: &TypeInfo) -> String {
    match info {
        TypeInfo::Class {
            docstring,
            methods,
            canonical_fqn,
            ..
        } => {
            let mut inner = String::new();
            if let Some(doc) = docstring {
                for line in doc.lines() {
                    inner.push_str("/// ");
                    inner.push_str(line);
                    inner.push('\n');
                }
            }
            inner.push_str(&info.to_describe_block());
            let mut out = format!("```baml\n{inner}\n```");
            if !methods.is_empty() {
                out.push_str("\n\nRun `baml describe ");
                out.push_str(canonical_fqn);
                out.push_str("` for methods and details.");
            }
            out
        }
        TypeInfo::Function { note, .. } => {
            let mut out = format!("```baml\n{}\n```", info.to_describe_block());
            if let Some(note) = note {
                out.push_str("\n\n");
                out.push_str(note);
            }
            out
        }
        TypeInfo::Documentation { detail, .. } => {
            format!("```baml\n{}\n```\n\n{detail}", info.to_describe_block())
        }
        TypeInfo::Enum { .. }
        | TypeInfo::TypeAlias { .. }
        | TypeInfo::TemplateString { .. }
        | TypeInfo::LocalVar { .. }
        | TypeInfo::Symbol { .. }
        | TypeInfo::OtherItem { .. } => {
            format!("```baml\n{}\n```", info.to_describe_block())
        }
    }
}

/// The LSP symbol kind for a definition kind. LSP has no alias/template
/// notions, so those map to the nearest renderable icon.
pub(super) fn symbol_kind(kind: DefinitionKind) -> SymbolKind {
    match kind {
        DefinitionKind::Class => SymbolKind::CLASS,
        DefinitionKind::Enum => SymbolKind::ENUM,
        DefinitionKind::Interface => SymbolKind::INTERFACE,
        DefinitionKind::TypeAlias => SymbolKind::OBJECT,
        DefinitionKind::Function => SymbolKind::FUNCTION,
        DefinitionKind::TemplateString => SymbolKind::STRING,
        DefinitionKind::Client => SymbolKind::OBJECT,
        DefinitionKind::Test => SymbolKind::EVENT,
        DefinitionKind::RetryPolicy => SymbolKind::OBJECT,
        DefinitionKind::Let => SymbolKind::CONSTANT,
        DefinitionKind::Field => SymbolKind::FIELD,
        DefinitionKind::AssociatedType => SymbolKind::TYPE_PARAMETER,
        DefinitionKind::Method => SymbolKind::METHOD,
        DefinitionKind::Variant => SymbolKind::ENUM_MEMBER,
        DefinitionKind::Binding => SymbolKind::VARIABLE,
        DefinitionKind::Parameter => SymbolKind::VARIABLE,
    }
}

/// An outline item (and its children) as a `DocumentSymbol` tree.
pub(super) fn document_symbol(item: &OutlineItem, codec: &PositionCodec<'_>) -> DocumentSymbol {
    #[expect(
        deprecated,
        reason = "DocumentSymbol::deprecated is an LSP wire field; lsp_types keeps it and struct construction must fill it"
    )]
    DocumentSymbol {
        name: item.name.clone(),
        detail: None,
        kind: symbol_kind(item.kind),
        tags: None,
        deprecated: None,
        range: codec.byte_range_to_lsp(item.range),
        selection_range: codec.byte_range_to_lsp(item.name_span),
        children: if item.children.is_empty() {
            None
        } else {
            Some(
                item.children
                    .iter()
                    .map(|child| document_symbol(child, codec))
                    .collect(),
            )
        },
    }
}

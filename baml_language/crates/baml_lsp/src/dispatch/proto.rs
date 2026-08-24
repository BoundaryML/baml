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

/// Hover markdown for a [`TypeInfo`], rust-analyzer's shape spelled in
/// BAML's `.`-paths: the owning path in its own fence above the
/// declaration, the declaration block, then docs below a `---` separator —
/// plus, for a class with methods, a pointer at `baml describe` (hover
/// never inlines method lists).
pub(super) fn hover_markdown(info: &TypeInfo) -> String {
    let mut out = String::new();
    if let Some(owner) = info.owner_path() {
        out.push_str("```baml\n");
        out.push_str(owner);
        out.push_str("\n```\n\n---\n\n");
    }
    out.push_str("```baml\n");
    out.push_str(&info.to_hover_block());
    out.push_str("\n```");
    if let Some(docs) = info.docs() {
        out.push_str("\n\n---\n\n");
        out.push_str(docs);
    }
    match info {
        TypeInfo::Class {
            methods,
            canonical_fqn,
            ..
        } if !methods.is_empty() => {
            out.push_str("\n\nRun `baml describe ");
            out.push_str(canonical_fqn);
            out.push_str("` for methods and details.");
        }
        TypeInfo::Interface { canonical_fqn, .. } => {
            out.push_str("\n\nRun `baml describe ");
            out.push_str(canonical_fqn);
            out.push_str("` for the full surface.");
        }
        TypeInfo::Function {
            note: Some(note), ..
        } => {
            out.push_str("\n\n");
            out.push_str(note);
        }
        TypeInfo::Documentation { detail, .. } => {
            out.push_str("\n\n");
            out.push_str(detail);
        }
        TypeInfo::Class { .. }
        | TypeInfo::Function { .. }
        | TypeInfo::Enum { .. }
        | TypeInfo::TypeAlias { .. }
        | TypeInfo::TemplateString { .. }
        | TypeInfo::LocalVar { .. }
        | TypeInfo::Symbol { .. }
        | TypeInfo::OtherItem { .. } => {}
    }
    out
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

/// A completion, on the wire.
///
/// `sort_text` carries the ide layer's ranking: the list arrives best-first,
/// so the index IS the rank, zero-padded so the editor's lexicographic sort
/// reproduces it. `filter_text` is the label — the edit's range starts where
/// the fragment does, so the client filters on the same text it will replace.
pub(super) fn completion_item(
    item: &baml_ide::Completion,
    rank: usize,
    codec: &PositionCodec<'_>,
    snippet_support: bool,
) -> lsp_types::CompletionItem {
    let (new_text, format) = match (&item.insert, snippet_support) {
        (baml_ide::CompletionInsert::Snippet(snippet), true) => {
            (snippet.clone(), Some(lsp_types::InsertTextFormat::SNIPPET))
        }
        // A client without snippet support gets the placeholders collapsed,
        // never the literal `${1:…}` markers.
        (insert, _) => (insert.plain_text(), None),
    };
    lsp_types::CompletionItem {
        label: item.label.clone(),
        kind: Some(match item.kind {
            baml_ide::CompletionKind::Field => lsp_types::CompletionItemKind::FIELD,
            baml_ide::CompletionKind::Method => lsp_types::CompletionItemKind::METHOD,
            baml_ide::CompletionKind::Function => lsp_types::CompletionItemKind::FUNCTION,
            baml_ide::CompletionKind::Local => lsp_types::CompletionItemKind::VARIABLE,
            baml_ide::CompletionKind::Parameter => lsp_types::CompletionItemKind::PROPERTY,
            baml_ide::CompletionKind::Class => lsp_types::CompletionItemKind::CLASS,
            baml_ide::CompletionKind::Enum => lsp_types::CompletionItemKind::ENUM,
            baml_ide::CompletionKind::EnumVariant => lsp_types::CompletionItemKind::ENUM_MEMBER,
            baml_ide::CompletionKind::Interface => lsp_types::CompletionItemKind::INTERFACE,
            baml_ide::CompletionKind::TypeAlias => lsp_types::CompletionItemKind::STRUCT,
            baml_ide::CompletionKind::Client | baml_ide::CompletionKind::RetryPolicy => {
                lsp_types::CompletionItemKind::CONSTANT
            }
            baml_ide::CompletionKind::Package => lsp_types::CompletionItemKind::MODULE,
            baml_ide::CompletionKind::Keyword => lsp_types::CompletionItemKind::KEYWORD,
            baml_ide::CompletionKind::Other => lsp_types::CompletionItemKind::TEXT,
        }),
        detail: item.detail.clone(),
        documentation: item.documentation.as_ref().map(|docs| {
            lsp_types::Documentation::MarkupContent(lsp_types::MarkupContent {
                kind: lsp_types::MarkupKind::Markdown,
                value: docs.clone(),
            })
        }),
        sort_text: Some(format!("{rank:04}")),
        filter_text: Some(item.label.clone()),
        text_edit: Some(lsp_types::CompletionTextEdit::Edit(lsp_types::TextEdit {
            range: codec.byte_range_to_lsp(item.source_range),
            new_text,
        })),
        insert_text_format: format,
        ..Default::default()
    }
}

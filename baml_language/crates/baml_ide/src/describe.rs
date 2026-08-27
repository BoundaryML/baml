//! `describe` — structured symbol description for CLI and agent use.
//!
//! The core `describe()` function takes a symbol name and produces a
//! `SymbolDescription` containing everything needed to understand the symbol:
//! shape (compact representation), full source body, docstring, signature-level
//! dependencies, and reference sites.
//!
//! This is a regular function (not a Salsa query). Internally it calls
//! Salsa-cached queries (`file_outline`, the `item_data` firewall, `syntax_tree`,
//! etc.).

use baml_base::SourceFile;
use baml_compiler_syntax::SyntaxKind;
use baml_compiler2_hir::{
    contributions::{Definition, DefinitionKind},
    scope::{FileScopeId, ScopeKind},
};
use serde::Serialize;
use text_size::TextRange;

use crate::{
    info::type_info_for_definition,
    render,
    search::{SymbolInfo, search_symbols},
    usages::usages_at,
};

// ── Types ────────────────────────────────────────────────────────────────────

/// Complete description of a symbol.
#[derive(Clone, Serialize)]
pub struct SymbolDescription {
    /// The symbol's name.
    pub name: String,
    /// What the symbol IS, with the kind-specific payload inside — a class
    /// cannot lack its method sections, an interface cannot carry another
    /// kind's members, only a member has a container.
    pub kind: SymbolKind,
    /// The file where the symbol is defined (in-memory handle, not serialized).
    #[serde(skip)]
    pub file: SourceFile,
    /// File path as a string, for JSON consumers.
    pub file_path: String,
    /// Byte range of the name token.
    #[serde(serialize_with = "serialize_range")]
    pub name_span: TextRange,
    /// Byte range of the full item node in the CST.
    #[serde(serialize_with = "serialize_range")]
    pub item_range: TextRange,
    /// Compact shape representation (e.g. signature + field list).
    pub shape: String,
    /// Complete source text of the definition.
    pub full_body: String,
    /// Leading `///` docstring, if any.
    pub docstring: Option<String>,
    /// Resolved type string, if applicable (e.g. `(a: string) -> int` for functions).
    pub resolved_type: Option<String>,
    /// Symbols referenced in the signature (parameter types, return type, etc.).
    pub dependencies: Vec<DepRef>,
    /// Sites where this symbol is used.
    pub references: Vec<RefSite>,
}

/// A described symbol's kind, carrying exactly the payload that kind of
/// symbol has. The one total mapping from the compiler's flat
/// [`DefinitionKind`] is `classify_definition_kind`; the reverse
/// projection (for display labels and kind-based coloring) is
/// [`Self::definition_kind`].
#[derive(Clone, Serialize)]
pub enum SymbolKind {
    Class {
        /// See [`SymbolKind::canonical_fqn`].
        canonical_fqn: Option<String>,
        /// Instance methods (first param `self`).
        instance_methods: Vec<MethodRef>,
        /// Static methods (no `self` param).
        static_methods: Vec<MethodRef>,
    },
    Interface {
        /// See [`SymbolKind::canonical_fqn`].
        canonical_fqn: Option<String>,
        /// The declared member surface in rendering priority order —
        /// associated types, fields, required methods, defaulted methods.
        /// Each member carries its facets separately (declaration,
        /// docstring, body) so a renderer can enumerate every declaration
        /// before disclosing any docstring, and docstrings before any body.
        members: Vec<InterfaceMember>,
        /// Impl blocks whose head names this interface.
        implementations: Vec<ImplRow>,
    },
    /// Any other top-level item (function, enum, type alias, client, …).
    Item {
        kind: ItemKind,
        /// See [`SymbolKind::canonical_fqn`].
        canonical_fqn: Option<String>,
    },
    /// A member of a containing item.
    Member {
        kind: MemberKind,
        /// The containing item, when it resolved.
        container: Option<DepRef>,
    },
    /// A local inside a function.
    Local { kind: LocalKind },
}

/// A top-level item kind with no describe-specific payload (classes and
/// interfaces have their own [`SymbolKind`] variants).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum ItemKind {
    Enum,
    TypeAlias,
    Function,
    TemplateString,
    Client,
    Test,
    RetryPolicy,
    Let,
}

/// An intra-item member kind — the [`DefinitionKind::is_member`] set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum MemberKind {
    Field,
    AssociatedType,
    Method,
    Variant,
}

/// A local's kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum LocalKind {
    Parameter,
    Binding,
}

impl SymbolKind {
    /// The compiler's flat kind, for display labels and kind-based coloring.
    pub fn definition_kind(&self) -> DefinitionKind {
        match self {
            SymbolKind::Class { .. } => DefinitionKind::Class,
            SymbolKind::Interface { .. } => DefinitionKind::Interface,
            SymbolKind::Item { kind, .. } => match kind {
                ItemKind::Enum => DefinitionKind::Enum,
                ItemKind::TypeAlias => DefinitionKind::TypeAlias,
                ItemKind::Function => DefinitionKind::Function,
                ItemKind::TemplateString => DefinitionKind::TemplateString,
                ItemKind::Client => DefinitionKind::Client,
                ItemKind::Test => DefinitionKind::Test,
                ItemKind::RetryPolicy => DefinitionKind::RetryPolicy,
                ItemKind::Let => DefinitionKind::Let,
            },
            SymbolKind::Member { kind, .. } => match kind {
                MemberKind::Field => DefinitionKind::Field,
                MemberKind::AssociatedType => DefinitionKind::AssociatedType,
                MemberKind::Method => DefinitionKind::Method,
                MemberKind::Variant => DefinitionKind::Variant,
            },
            SymbolKind::Local { kind } => match kind {
                LocalKind::Parameter => DefinitionKind::Parameter,
                LocalKind::Binding => DefinitionKind::Binding,
            },
        }
    }

    /// Canonical fully-qualified name to print in the header, `Some` only
    /// when it differs from the bare name (a builtin alias like `string`, or
    /// a namespaced/dependency type like `root.ns.Foo`) — a top-level user
    /// symbol at package root shows its bare name, which is already
    /// canonical. Members and locals never carry one: their headers show the
    /// bare name, and a member's owner is the container row.
    pub fn canonical_fqn(&self) -> Option<&str> {
        match self {
            SymbolKind::Class { canonical_fqn, .. }
            | SymbolKind::Interface { canonical_fqn, .. }
            | SymbolKind::Item { canonical_fqn, .. } => canonical_fqn.as_deref(),
            SymbolKind::Member { .. } | SymbolKind::Local { .. } => None,
        }
    }

    /// The containing item — `Some` only for a member that resolved one.
    pub fn container(&self) -> Option<&DepRef> {
        match self {
            SymbolKind::Member { container, .. } => container.as_ref(),
            SymbolKind::Class { .. }
            | SymbolKind::Interface { .. }
            | SymbolKind::Item { .. }
            | SymbolKind::Local { .. } => None,
        }
    }
}

/// The families a [`DefinitionKind`] partitions into for describe. THE one
/// exhaustive mapping: a new compiler kind fails compilation here, forcing a
/// decision about which payload its descriptions carry.
enum KindClass {
    Class,
    Interface,
    Item(ItemKind),
    Member(MemberKind),
    Local(LocalKind),
}

fn classify_definition_kind(kind: DefinitionKind) -> KindClass {
    match kind {
        DefinitionKind::Class => KindClass::Class,
        DefinitionKind::Interface => KindClass::Interface,
        DefinitionKind::Enum => KindClass::Item(ItemKind::Enum),
        DefinitionKind::TypeAlias => KindClass::Item(ItemKind::TypeAlias),
        DefinitionKind::Function => KindClass::Item(ItemKind::Function),
        DefinitionKind::TemplateString => KindClass::Item(ItemKind::TemplateString),
        DefinitionKind::Client => KindClass::Item(ItemKind::Client),
        DefinitionKind::Test => KindClass::Item(ItemKind::Test),
        DefinitionKind::RetryPolicy => KindClass::Item(ItemKind::RetryPolicy),
        DefinitionKind::Let => KindClass::Item(ItemKind::Let),
        DefinitionKind::Field => KindClass::Member(MemberKind::Field),
        DefinitionKind::AssociatedType => KindClass::Member(MemberKind::AssociatedType),
        DefinitionKind::Method => KindClass::Member(MemberKind::Method),
        DefinitionKind::Variant => KindClass::Member(MemberKind::Variant),
        DefinitionKind::Binding => KindClass::Local(LocalKind::Binding),
        DefinitionKind::Parameter => KindClass::Local(LocalKind::Parameter),
    }
}

/// A method of a class, surfaced in `describe` so methods are always
/// discoverable regardless of body truncation. Unlike [`DepRef`] (dependency
/// tracking) this carries the full canonical signature, first-line docstring,
/// and full definition range needed to render a method listing.
#[derive(Clone, Serialize)]
pub struct MethodRef {
    pub name: String,
    /// Canonical one-line signature, e.g. `function Greet(self) -> string`.
    pub signature: String,
    /// First line of the method's docstring, if any.
    pub docstring: Option<String>,
    #[serde(skip)]
    pub file: SourceFile,
    pub file_path: String,
    /// Byte range of the full method definition (1-based line range when rendered).
    #[serde(serialize_with = "serialize_range")]
    pub item_range: TextRange,
}

/// Which slot of an interface's declared surface a member occupies, in the
/// rendering priority order `baml describe` uses: associated types first,
/// then fields, required methods, defaulted methods.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InterfaceMemberCategory {
    AssociatedType,
    Field,
    RequiredMethod,
    DefaultMethod,
}

impl InterfaceMemberCategory {
    pub fn as_str(self) -> &'static str {
        match self {
            InterfaceMemberCategory::AssociatedType => "associated type",
            InterfaceMemberCategory::Field => "field",
            InterfaceMemberCategory::RequiredMethod => "required method",
            InterfaceMemberCategory::DefaultMethod => "default method",
        }
    }
}

/// One member of an interface's declared surface, with its facets separated
/// so a renderer can admit them independently: the one-line declaration is
/// the enumeration, the docstring and (for defaulted methods) the body are
/// progressively-disclosed detail behind it.
#[derive(Clone, Serialize)]
pub struct InterfaceMember {
    pub category: InterfaceMemberCategory,
    pub name: String,
    /// The declaration line exactly as the reconstructed interface block
    /// spells it: `type Item extends Bar;`, `name: string,`, a method
    /// signature — a defaulted method carries a `{ ... }` suffix marking its
    /// elided body.
    pub declaration: String,
    /// Full docstring (every line), if any.
    pub docstring: Option<String>,
    /// Defaulted methods only: the full definition source with leading doc
    /// comments stripped (the docstring facet carries those).
    pub body: Option<String>,
    #[serde(skip)]
    pub file: SourceFile,
    pub file_path: String,
    /// Byte range of the member's declaration (methods: the whole definition).
    #[serde(serialize_with = "serialize_range")]
    pub item_range: TextRange,
}

/// An impl block whose head names the described interface.
#[derive(Clone, Serialize)]
pub struct ImplRow {
    /// `implement <Head> for <Target>` in canonical spelling.
    pub display: String,
    #[serde(skip)]
    pub file: SourceFile,
    pub file_path: String,
    /// Byte range of the impl block.
    #[serde(serialize_with = "serialize_range")]
    pub span: TextRange,
    /// Byte range of the head's interface mention (`Named` in
    /// `implement Named for Robot`) — the site this row REPLACES in the
    /// reference list, so the two sections never double-report one mention.
    #[serde(serialize_with = "serialize_range")]
    pub head_span: TextRange,
}

/// A symbol referenced in the signature of another symbol.
#[derive(Clone, Serialize)]
pub struct DepRef {
    pub name: String,
    #[serde(serialize_with = "serialize_kind")]
    pub kind: DefinitionKind,
    #[serde(skip)]
    pub file: SourceFile,
    pub file_path: String,
    #[serde(serialize_with = "serialize_range")]
    pub name_span: TextRange,
}

/// A site where a symbol is used.
#[derive(Clone, Serialize)]
pub struct RefSite {
    #[serde(skip)]
    pub file: SourceFile,
    pub file_path: String,
    #[serde(serialize_with = "serialize_range")]
    pub range: TextRange,
    /// The full source line containing the reference.
    pub line_text: String,
    /// 1-based line number.
    pub line_number: usize,
}

// ── describe ─────────────────────────────────────────────────────────────────

/// Describe a symbol by exact name.
///
/// Searches all `files` for a symbol whose name matches `name` exactly
/// (case-sensitive). Returns descriptions for all matches (there may be
/// multiple if the name appears in different files, though BAML typically
/// has unique top-level names).
///
/// Returns an empty Vec if no symbol with that name exists.
pub fn describe(
    db: &dyn baml_compiler2_ppir::Db,
    files: &[SourceFile],
    name: &str,
) -> Vec<SymbolDescription> {
    // Find exact matches via search_symbols (which does case-insensitive
    // substring matching), then filter to exact case-sensitive matches.
    let candidates = search_symbols(db, files, name);

    // Prefer top-level symbols; fall back to members (fields, variants) if
    // no top-level match exists; finally try locals (parameters, let bindings).
    let top_level: Vec<&SymbolInfo> = candidates
        .iter()
        .filter(|s| s.name == name && s.container_name.is_none())
        .collect();

    if !top_level.is_empty() {
        return top_level
            .into_iter()
            .filter_map(|sym| describe_symbol(db, files, sym))
            .collect();
    }

    let members: Vec<&SymbolInfo> = candidates
        .iter()
        .filter(|s| s.name == name && s.container_name.is_some())
        .collect();

    if !members.is_empty() {
        return members
            .into_iter()
            .filter_map(|sym| describe_symbol(db, files, sym))
            .collect();
    }

    // No outline match — try locals (parameters and let bindings).
    describe_locals(db, files, name)
}

/// Describe a symbol given a known `Definition` (from `PackageItems` lookup).
///
/// Bypasses substring search — goes directly from `Definition` to `SymbolDescription`
/// using the same `describe_top_level()` internals.
pub fn describe_by_definition(
    db: &dyn baml_compiler2_ppir::Db,
    files: &[SourceFile],
    definition: Definition<'_>,
) -> Option<SymbolDescription> {
    if definition.is_language_internal(db) {
        return None;
    }
    let (file, name_span) = crate::syntax::definition_span(db, definition)?;

    // Extract the name text from the source.
    let name = {
        let text = file.text(db);
        let start: usize = name_span.start().into();
        let end: usize = name_span.end().into();
        text[start..end].to_string()
    };

    let sym = SymbolInfo {
        name,
        kind: definition.kind(),
        file,
        name_span,
        container_name: None,
    };

    describe_top_level(db, files, &sym)
}

/// Describe a member (field, variant) within a known parent item.
///
/// Searches the parent item's children in the file outline for a member
/// matching `member_name`, then delegates to `describe_member()`.
pub fn describe_item_member(
    db: &dyn baml_compiler2_ppir::Db,
    files: &[SourceFile],
    parent_def: Definition<'_>,
    member_name: &str,
) -> Option<SymbolDescription> {
    // A class method drills into the method itself — signature + body — which
    // the field/variant member path below cannot render.
    if let Definition::Class(class_loc) = parent_def {
        if let Some(desc) = describe_class_method(db, files, class_loc, member_name) {
            return Some(desc);
        }
    }

    // Interface members (methods, fields, associated types) are declared on
    // the interface's own item data — the outline walk below never descends
    // into interfaces, so this arm is their only road.
    if let Definition::Interface(iface_loc) = parent_def {
        if let Some(desc) = describe_interface_member(db, files, iface_loc, member_name) {
            return Some(desc);
        }
    }

    let (parent_file, parent_name_span) = crate::syntax::definition_span(db, parent_def)?;

    // Get the parent's name from the source text.
    let parent_name = {
        let text = parent_file.text(db);
        let start: usize = parent_name_span.start().into();
        let end: usize = parent_name_span.end().into();
        text[start..end].to_string()
    };

    // Search the file outline for the parent's children.
    let outline = crate::outline::file_outline(db, parent_file);
    for item in outline {
        if item.name == parent_name {
            for child in &item.children {
                if child.name == member_name {
                    let sym = SymbolInfo {
                        name: child.name.clone(),
                        kind: child.kind,
                        file: parent_file,
                        name_span: child.name_span,
                        container_name: Some(parent_name),
                    };
                    return describe_member(db, files, &sym);
                }
            }
        }
    }

    None
}

/// Build a full `SymbolDescription` for a single `SymbolInfo`.
fn describe_symbol(
    db: &dyn baml_compiler2_ppir::Db,
    files: &[SourceFile],
    sym: &SymbolInfo,
) -> Option<SymbolDescription> {
    if sym.kind.is_member() {
        describe_member(db, files, sym)
    } else {
        describe_top_level(db, files, sym)
    }
}

/// Describe a top-level symbol (class, function, enum, etc.).
fn describe_top_level(
    db: &dyn baml_compiler2_ppir::Db,
    files: &[SourceFile],
    sym: &SymbolInfo,
) -> Option<SymbolDescription> {
    let file = sym.file;

    // ── CST body extraction ──────────────────────────────────────────────────
    let item_range = find_item_range(db, file, sym.name_span, sym.kind)?;

    // Resolve the symbol's definition once; every downstream helper reuses it
    // instead of re-running name resolution.
    let definition = resolve_definition(db, file, sym);

    // ── Shape generation ─────────────────────────────────────────────────────
    let shape = build_shape(db, sym, definition);

    // ── Docstring extraction ─────────────────────────────────────────────────
    let docstring = extract_docstring(db, file, item_range);

    // ── Dependency discovery ─────────────────────────────────────────────────
    let dependencies = find_dependencies(db, files, file, sym, definition);

    // ── Resolved type ────────────────────────────────────────────────────────
    let resolved_type = resolve_type_for_item(db, definition);

    // ── Reference finding ────────────────────────────────────────────────────
    let mut references = find_references(db, files, file, sym.name_span, item_range);

    // ── Kind payload ─────────────────────────────────────────────────────────
    let kind = match classify_definition_kind(sym.kind) {
        KindClass::Class => {
            let (instance_methods, static_methods) = match definition {
                Some(Definition::Class(class_loc)) => collect_class_methods(db, class_loc),
                _ => (Vec::new(), Vec::new()),
            };
            SymbolKind::Class {
                canonical_fqn: canonical_fqn(db, sym, definition),
                instance_methods,
                static_methods,
            }
        }
        KindClass::Interface => {
            let (members, implementations) = match definition {
                Some(Definition::Interface(iface_loc)) => (
                    collect_interface_members(db, iface_loc),
                    collect_interface_impls(db, iface_loc),
                ),
                _ => (Vec::new(), Vec::new()),
            };
            SymbolKind::Interface {
                canonical_fqn: canonical_fqn(db, sym, definition),
                members,
                implementations,
            }
        }
        KindClass::Item(kind) => SymbolKind::Item {
            kind,
            canonical_fqn: canonical_fqn(db, sym, definition),
        },
        // `describe_symbol` routes members and locals to their own builders;
        // one reaching this path (a search hit with no member context)
        // describes with no container — honest absence.
        KindClass::Member(kind) => SymbolKind::Member {
            kind,
            container: None,
        },
        KindClass::Local(kind) => SymbolKind::Local { kind },
    };

    // An impl-head mention (`implement Named for …`, `implements Named {`)
    // is reported by the implementations section; keeping it in the
    // reference list too would double-count the same site.
    if let SymbolKind::Interface {
        implementations, ..
    } = &kind
    {
        references.retain(|reference| {
            !implementations.iter().any(|imp| {
                imp.file == reference.file && imp.head_span.contains_range(reference.range)
            })
        });
    }

    // Body block, with non-doc comments removed (CST-token based, so `//`
    // inside string/prompt literals is never touched):
    // - class: fields-only reconstruction (methods get their own sections),
    //   prefixed with the full `///` docstring;
    // - interface: the signature-only member surface (an interface is its
    //   contract; default bodies are implementation detail behind drill-in),
    //   prefixed with the full `///` docstring;
    // - builtin function: the signature only, never the native body block;
    // - everything else: the real source body.
    let full_body = if matches!(
        kind,
        SymbolKind::Class { .. } | SymbolKind::Interface { .. }
    ) {
        let mut body = docstring_lines(docstring.as_deref());
        body.push_str(&shape);
        body
    } else {
        let body_range =
            builtin_signature_range(db, file, sym, item_range, definition).unwrap_or(item_range);
        clean_body_source(db, file, body_range)
    };

    Some(SymbolDescription {
        name: sym.name.clone(),
        kind,
        file_path: file_path_string(db, file),
        file,
        name_span: sym.name_span,
        item_range,
        shape,
        full_body,
        docstring,
        resolved_type,
        dependencies,
        references,
    })
}

/// Describe a member symbol (field, variant, method).
///
/// Shows the member itself, the containing class/enum as a dependency,
/// and references to the member.
fn describe_member(
    db: &dyn baml_compiler2_ppir::Db,
    files: &[SourceFile],
    sym: &SymbolInfo,
) -> Option<SymbolDescription> {
    let file = sym.file;
    let KindClass::Member(member_kind) = classify_definition_kind(sym.kind) else {
        return None;
    };

    // Find the member's CST node (FIELD or ENUM_VARIANT).
    let member_range = find_member_range(db, file, sym.name_span, sym.kind)?;
    let full_body = clean_body_source(db, file, member_range);

    // Shape: "field_name type" or "VariantName" with container context.
    let shape = if let Some(ref container) = sym.container_name {
        format!("{}.{}", container, full_body.trim())
    } else {
        full_body.trim().to_string()
    };

    // Docstring from the member's CST node.
    let docstring = extract_docstring(db, file, member_range);

    // The containing class/enum is a dependency.
    let mut dependencies = Vec::new();
    if let Some(ref container_name) = sym.container_name {
        // Find the container symbol.
        let container_results = search_symbols(db, files, container_name);
        if let Some(container) = container_results
            .iter()
            .find(|s| s.name == *container_name && s.container_name.is_none())
        {
            dependencies.push(DepRef {
                name: container.name.clone(),
                kind: container.kind,
                file_path: file_path_string(db, container.file),
                file: container.file,
                name_span: container.name_span,
            });
        }
    }

    // Resolved type for members — look up via the parent class/enum rather than
    // `resolve_name_at`, which only handles top-level `Definition` items.
    let resolved_type = resolve_member_type(db, file, sym);

    // References to this field/variant.
    let references = find_references(db, files, file, sym.name_span, member_range);

    // Move the container dependency from dependencies to the container field.
    let container = if !dependencies.is_empty() {
        Some(dependencies.remove(0))
    } else {
        None
    };

    Some(SymbolDescription {
        name: sym.name.clone(),
        kind: SymbolKind::Member {
            kind: member_kind,
            container,
        },
        file_path: file_path_string(db, file),
        file,
        name_span: sym.name_span,
        item_range: member_range,
        shape,
        full_body,
        docstring,
        resolved_type,
        dependencies: Vec::new(),
        references,
    })
}

// ── Local variable lookup ────────────────────────────────────────────────────

/// Find and describe local variables (parameters and let bindings) by name.
///
/// Scans all functions in all files for parameters and let bindings matching
/// `name`. Returns a `SymbolDescription` for each match, with the containing
/// function as a dependency.
#[expect(
    deprecated,
    reason = "pre-D3: materializes interned inference state; moves to the finalized facts layer with the cutover"
)]
fn describe_locals(
    db: &dyn baml_compiler2_ppir::Db,
    files: &[SourceFile],
    name: &str,
) -> Vec<SymbolDescription> {
    let mut results = Vec::new();

    for &file in files {
        let index = baml_compiler2_hir::file_semantic_index(db, file);

        for &func_loc in baml_compiler2_ppir::item_data::file_functions(db, file) {
            let func_data = baml_compiler2_ppir::item_data::function_data(db, func_loc);
            let func_span = baml_compiler2_ppir::item_data::function_source_map(db, func_loc).span;
            // ── Check parameters ─────────────────────────────────────────
            for (param_idx, param) in func_data.params.iter().enumerate() {
                if param.name.as_str() != name {
                    continue;
                }

                let type_str = param
                    .type_ref
                    .map(|id| render::display_type_ref(&func_data.type_refs, id))
                    .unwrap_or_else(|| render::MISSING_RETURN.to_string());
                let optional = if param.has_default { "?" } else { "" };

                // Find the parameter's source span from the signature source map.
                let param_span =
                    baml_compiler2_hir::signature::function_signature_source_map(db, func_loc)
                        .param_spans
                        .get(param_idx)
                        .copied()
                        .unwrap_or_else(|| text_size::TextRange::empty(func_span.start()));

                let func_name = func_data.name.as_str().to_string();

                // Find usages of this parameter within the function.
                let param_refs = usages_at(db, file, param_span.start())
                    .into_iter()
                    .filter(|loc| !(loc.file == file && loc.range == param_span))
                    .map(|loc| {
                        let text = loc.file.text(db);
                        let (line_number, line_text) = line_at_offset(text, loc.range.start());
                        RefSite {
                            file_path: file_path_string(db, loc.file),
                            file: loc.file,
                            range: loc.range,
                            line_text,
                            line_number,
                        }
                    })
                    .collect();

                // Get the full function body for context display.
                let func_body = clean_body_source(db, file, func_span);

                results.push(SymbolDescription {
                    name: name.to_string(),
                    kind: SymbolKind::Local {
                        kind: LocalKind::Parameter,
                    },
                    file_path: file_path_string(db, file),
                    file,
                    name_span: param_span,
                    item_range: func_span,
                    shape: format!("{name}{optional}: {type_str}"),
                    full_body: func_body,
                    docstring: None,
                    resolved_type: Some(type_str),
                    dependencies: vec![make_function_dep(db, func_loc, &func_name)],
                    references: param_refs,
                });
            }

            // ── Check let bindings ───────────────────────────────────────
            // Find the function's scope in the semantic index.
            let func_scope_idx = index.scopes.iter().position(|s| {
                matches!(s.kind, baml_compiler2_hir::scope::ScopeKind::Function)
                    && s.name.as_ref() == Some(&func_data.name)
                    && s.range == func_span
            });

            let Some(func_scope_idx) = func_scope_idx else {
                continue;
            };

            // Collect bindings from the function scope AND all descendant
            // scopes (blocks, match arms, lambdas, etc.) so we don't miss
            // let bindings nested inside control flow.
            let func_scope = &index.scopes[func_scope_idx];
            let scope_indices = std::iter::once(func_scope_idx).chain(
                (func_scope.descendants.start.index()..func_scope.descendants.end.index())
                    .map(|i| i as usize),
            );

            for scope_idx in scope_indices {
                let bindings = &index.scope_bindings[scope_idx];

                for binding in &bindings.bindings {
                    if binding.name.as_str() != name {
                        continue;
                    }

                    // Use the body-owning scope for type inference and StmtId
                    // lookup. StmtId/PatId are local to an ExprBody, so
                    // lambda descendants must resolve against the lambda body,
                    // while ordinary block descendants resolve against the
                    // enclosing function body.
                    let def_site = binding.site;
                    let binding_span = binding.name_range;
                    let binding_scope =
                        FileScopeId::new(u32::try_from(scope_idx).expect("scope id overflow"));
                    let owner_scope = body_owner_scope(index, binding_scope);
                    let scope_id = index.scope_ids[owner_scope.index() as usize];
                    let inference = baml_compiler2_hir_ty::ide::infer_for_scope(db, scope_id);

                    // Parameters are already handled by the sig.params pass above;
                    // skip them here to avoid duplicate results.
                    if matches!(
                        def_site,
                        baml_compiler2_hir::semantic_index::DefinitionSite::Parameter(_)
                    ) {
                        continue;
                    }

                    let type_str = match def_site {
                        baml_compiler2_hir::semantic_index::DefinitionSite::Statement(stmt_id) => {
                            pattern_from_owner_body(db, func_loc, stmt_id)
                                .and_then(|pattern| {
                                    inference?
                                        .type_of_pat
                                        .get(&pattern)
                                        .map(baml_type::interned::Ty::to_plain)
                                })
                                .map(|ty| render::display_ty(&ty))
                                .unwrap_or_else(|| "unknown".to_string())
                        }
                        baml_compiler2_hir::semantic_index::DefinitionSite::PatternBinding(
                            pat_id,
                        )
                        | baml_compiler2_hir::semantic_index::DefinitionSite::CatchBinding(
                            pat_id,
                        ) => inference
                            .and_then(|inference| {
                                inference
                                    .type_of_pat
                                    .get(&pat_id)
                                    .map(baml_type::interned::Ty::to_plain)
                            })
                            .map(|ty| render::display_ty(&ty))
                            .unwrap_or_else(|| "unknown".to_string()),
                        baml_compiler2_hir::semantic_index::DefinitionSite::Parameter(_) => {
                            unreachable!("Parameters are skipped above")
                        }
                    };

                    let func_name = func_data.name.as_str().to_string();

                    let binding_refs = usages_at(db, file, binding_span.start())
                        .into_iter()
                        .filter(|loc| !(loc.file == file && loc.range == binding_span))
                        .map(|loc| {
                            let text = loc.file.text(db);
                            let (line_number, line_text) = line_at_offset(text, loc.range.start());
                            RefSite {
                                file_path: file_path_string(db, loc.file),
                                file: loc.file,
                                range: loc.range,
                                line_text,
                                line_number,
                            }
                        })
                        .collect();

                    let func_body = clean_body_source(db, file, func_span);

                    results.push(SymbolDescription {
                        name: name.to_string(),
                        kind: SymbolKind::Local {
                            kind: LocalKind::Binding,
                        },
                        file_path: file_path_string(db, file),
                        file,
                        name_span: binding_span,
                        item_range: func_span,
                        shape: format!("let {name}: {type_str}"),
                        full_body: func_body,
                        docstring: None,
                        resolved_type: Some(type_str),
                        dependencies: vec![make_function_dep(db, func_loc, &func_name)],
                        references: binding_refs,
                    });
                }
            }
        }
    }

    results
}

/// Return the nearest scope whose `ExprBody` owns statement/pattern arena IDs.
fn body_owner_scope(
    index: &baml_compiler2_hir::semantic_index::FileSemanticIndex<'_>,
    mut scope_id: FileScopeId,
) -> FileScopeId {
    loop {
        let scope = &index.scopes[scope_id.index() as usize];
        if matches!(
            scope.kind,
            ScopeKind::Function | ScopeKind::Let | ScopeKind::Lambda
        ) {
            return scope_id;
        }

        let Some(parent) = scope.parent else {
            return scope_id;
        };
        scope_id = parent;
    }
}

/// The binding pattern for `stmt_id`, resolved against the function's body.
///
/// Lambda bodies share that arena, so a `StmtId` resolves against it whatever
/// scope owns the statement.
fn pattern_from_owner_body(
    db: &dyn baml_compiler2_ppir::Db,
    func_loc: baml_compiler2_hir::loc::FunctionLoc<'_>,
    stmt_id: baml_compiler2_ast::StmtId,
) -> Option<baml_compiler2_ast::PatId> {
    let body = baml_compiler2_ppir::function_body(db, func_loc);
    let baml_compiler2_hir::body::FunctionBody::Expr(top_body) = body.as_ref() else {
        return None;
    };
    crate::syntax::extract_pat_from_stmt(top_body, stmt_id)
}

/// Extract the binding pattern from a let/for statement in a specific body.
/// Descend through nested lambda bodies using scope ranges as stable anchors.
/// Build a `DepRef` pointing to a function.
fn make_function_dep(
    db: &dyn baml_compiler2_ppir::Db,
    func_loc: baml_compiler2_hir::loc::FunctionLoc<'_>,
    func_name: &str,
) -> DepRef {
    let file = func_loc.file(db);
    let source_map = baml_compiler2_ppir::item_data::function_source_map(db, func_loc);
    // `function_source_map` fills a missing name span with `TextRange::default()`
    // (empty at offset 0); a real named function's name span is never that, so
    // fall back to the full declaration span in that (spanless) case.
    let name_span = if source_map.name_span == TextRange::default() {
        source_map.span
    } else {
        source_map.name_span
    };
    DepRef {
        name: func_name.to_string(),
        kind: DefinitionKind::Function,
        file_path: file_path_string(db, file),
        file,
        name_span,
    }
}

// ── CST body extraction ──────────────────────────────────────────────────────

/// Map from `DefinitionKind` to the set of `SyntaxKinds` that represent item-level
/// CST nodes for that kind.
fn is_item_node(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::CLASS_DEF
            | SyntaxKind::INTERFACE_DEF
            | SyntaxKind::FUNCTION_DEF
            | SyntaxKind::ENUM_DEF
            | SyntaxKind::CLIENT_DEF
            | SyntaxKind::TEST_DEF
            | SyntaxKind::TEST_EXPR_DEF
            | SyntaxKind::TESTSET_DEF
            | SyntaxKind::RETRY_POLICY_DEF
            | SyntaxKind::TEMPLATE_STRING_DEF
            | SyntaxKind::TYPE_ALIAS_DEF
    )
}

/// Find the text range of a member CST node (FIELD, `ENUM_VARIANT`) for a name span.
fn find_member_range(
    db: &dyn baml_compiler2_ppir::Db,
    file: SourceFile,
    name_span: TextRange,
    kind: DefinitionKind,
) -> Option<TextRange> {
    let target_syntax_kind = match kind {
        DefinitionKind::Field => SyntaxKind::FIELD,
        DefinitionKind::Variant => SyntaxKind::ENUM_VARIANT,
        _ => return None,
    };

    let tree = baml_compiler_parser::syntax_tree(db, file);

    let token = match tree.token_at_offset(name_span.start()) {
        rowan::TokenAtOffset::Single(t) => t,
        rowan::TokenAtOffset::Between(_, right) => right,
        rowan::TokenAtOffset::None => return None,
    };

    token
        .parent_ancestors()
        .find(|n| n.kind() == target_syntax_kind)
        .map(|n| n.text_range())
}

/// Find the text range of the enclosing item CST node for a name span.
fn find_item_range(
    db: &dyn baml_compiler2_ppir::Db,
    file: SourceFile,
    name_span: TextRange,
    _kind: DefinitionKind,
) -> Option<TextRange> {
    let tree = baml_compiler_parser::syntax_tree(db, file);

    let token = match tree.token_at_offset(name_span.start()) {
        rowan::TokenAtOffset::Single(t) => t,
        rowan::TokenAtOffset::Between(_, right) => right,
        rowan::TokenAtOffset::None => return None,
    };

    // Walk up to the enclosing item-level node.
    token
        .parent_ancestors()
        .find(|n| is_item_node(n.kind()))
        .map(|n| n.text_range())
}

// ── Shape generation ─────────────────────────────────────────────────────────

/// Resolve a symbol to its [`Definition`], once. Returns `None` for symbols
/// that don't resolve to a top-level item/builtin (e.g. locals). Threaded
/// through the describe helpers so name resolution runs a single time.
fn resolve_definition<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    file: SourceFile,
    sym: &SymbolInfo,
) -> Option<Definition<'db>> {
    let name = baml_base::Name::new(&sym.name);
    match baml_compiler2_ppir::resolve::resolve_name_at(db, file, sym.name_span.start(), &name) {
        baml_compiler2_ppir::resolve::ResolvedName::Item(def)
        | baml_compiler2_ppir::resolve::ResolvedName::Builtin(def) => Some(def),
        _ => None,
    }
}

/// Build a compact shape string for a symbol.
///
/// Uses `TypeInfo` from the existing `type_info` module for structured data,
/// then formats it without the markdown code fences.
fn build_shape<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    sym: &SymbolInfo,
    def: Option<Definition<'db>>,
) -> String {
    let Some(def) = def else {
        return format!("{} {}", sym.kind.as_str(), sym.name);
    };
    let type_info = type_info_for_definition(db, def);
    // The canonical block (fields-only for classes), without fences/docstring/hint.
    type_info.to_describe_block()
}

// ── Class methods ──────────────────────────────────────────────────────────────

/// A method gathered from a class, before splitting into instance/static and
/// projecting into the public [`MethodRef`] / `MethodSig` shapes.
struct CollectedMethod {
    name: String,
    signature: String,
    docstring: Option<String>,
    file: SourceFile,
    file_path: String,
    item_range: TextRange,
    is_instance: bool,
}

/// Collect a class's methods (resolved canonical signatures) in source order,
/// skipping auto-derived plumbing (`to_json`/`from_json`, …). Shared spine for
/// [`collect_class_methods`] (describe) and [`class_method_sigs`] (hover).
fn collect_class_methods_impl(
    db: &dyn baml_compiler2_ppir::Db,
    class_loc: baml_compiler2_hir::loc::ClassLoc<'_>,
) -> Vec<CollectedMethod> {
    use baml_compiler2_hir_ty::package_interface::ExportedType;

    let file = class_loc.file(db);
    let class_data = baml_compiler2_ppir::item_data::class_data(db, class_loc);

    // Resolved param/return/throws types come from the package interface, which
    // lowers class methods 1:1 with `class_data.methods` (same order, including
    // auto-derived entries), so positional indices line up.
    let pkg_info = baml_compiler2_hir::file_package::file_package(db, file);
    let pkg_id = baml_compiler2_hir::package::PackageId::new(db, pkg_info.package.clone());
    let iface = baml_compiler2_hir_ty::package_interface::package_interface(db, pkg_id);
    let exported = iface
        .lookup_type(&pkg_info.namespace_path, &class_data.name)
        .and_then(|t| match t {
            ExportedType::Class { methods, .. } => Some(methods),
            _ => None,
        });

    let file_path = file_path_string(db, file);
    let mut out = Vec::new();
    for (idx, &method_loc) in class_data.methods.iter().enumerate() {
        let m = baml_compiler2_ppir::item_data::function_data(db, method_loc);
        if m.metadata.is_language_internal {
            continue;
        }
        let is_instance = m.params.first().is_some_and(|p| p.name.as_str() == "self");
        let signature = crate::info::resolved_function_sig_parts(
            db,
            method_loc,
            exported.and_then(|ms| crate::info::exported_method(ms, idx, &m.name)),
        )
        .render(db, file, crate::info::method_sig_style());
        let docstring = m
            .docstring
            .as_ref()
            .map(|d| d.lines().next().unwrap_or("").to_string());
        out.push(CollectedMethod {
            name: m.name.as_str().to_string(),
            signature,
            docstring,
            file,
            file_path: file_path.clone(),
            item_range: baml_compiler2_ppir::item_data::function_source_map(db, method_loc).span,
            is_instance,
        });
    }
    out
}

/// Build `(instance_methods, static_methods)` for a class's describe output.
/// Instance methods have a `self` first parameter; the rest are static.
fn collect_class_methods(
    db: &dyn baml_compiler2_ppir::Db,
    class_loc: baml_compiler2_hir::loc::ClassLoc<'_>,
) -> (Vec<MethodRef>, Vec<MethodRef>) {
    let mut instance = Vec::new();
    let mut statics = Vec::new();
    for m in collect_class_methods_impl(db, class_loc) {
        let bucket = if m.is_instance {
            &mut instance
        } else {
            &mut statics
        };
        bucket.push(MethodRef {
            name: m.name,
            signature: m.signature,
            docstring: m.docstring,
            file: m.file,
            file_path: m.file_path,
            item_range: m.item_range,
        });
    }
    (instance, statics)
}

// ── Interface surface ────────────────────────────────────────────────────────

/// Collect an interface's declared member surface in rendering priority
/// order: associated types, fields, required methods, defaulted methods.
///
/// Declarations use the same renderers as the hover/shape block
/// ([`crate::info::render_associated_type`], the arena `TypeRef` renderer,
/// `resolved_function_sig_parts` + `method_sig_style`), so a member reads
/// identically in every surface; the facet split (docstring, body) is what
/// this adds over the flat block.
fn collect_interface_members(
    db: &dyn baml_compiler2_ppir::Db,
    iface_loc: baml_compiler2_hir::loc::InterfaceLoc<'_>,
) -> Vec<InterfaceMember> {
    let file = iface_loc.file(db);
    let file_path = file_path_string(db, file);
    let iface = baml_compiler2_ppir::item_data::interface_data(db, iface_loc);
    let source_map = baml_compiler2_ppir::item_data::interface_source_map(db, iface_loc);

    let mut out = Vec::new();

    // Associated types. `docstring: None` is honest absence: assoc-type doc
    // comments are not lowered into `AssociatedTypeData` today; the facet
    // fills in if they ever are.
    for (assoc, spans) in iface
        .associated_types
        .iter()
        .zip(&source_map.associated_type_spans)
    {
        out.push(InterfaceMember {
            category: InterfaceMemberCategory::AssociatedType,
            name: assoc.name.as_str().to_string(),
            declaration: format!("{};", crate::info::render_associated_type(iface, assoc)),
            docstring: None,
            body: None,
            file,
            file_path: file_path.clone(),
            item_range: spans.span,
        });
    }

    // Fields.
    for (field, name_span) in iface.fields.iter().zip(&source_map.field_name_spans) {
        let item_range =
            find_member_range(db, file, *name_span, DefinitionKind::Field).unwrap_or(*name_span);
        out.push(InterfaceMember {
            category: InterfaceMemberCategory::Field,
            name: field.name.as_str().to_string(),
            declaration: format!(
                "{}: {},",
                field.name.as_str(),
                render::display_type_ref(&iface.type_refs, field.type_ref)
            ),
            docstring: field.docstring.clone(),
            body: None,
            file,
            file_path: file_path.clone(),
            item_range,
        });
    }

    // Methods: required before defaulted, each group in source order.
    let mut required = Vec::new();
    let mut defaulted = Vec::new();
    for &method_loc in &iface.methods {
        let m = baml_compiler2_ppir::item_data::function_data(db, method_loc);
        if m.metadata.is_language_internal {
            continue;
        }
        let signature = crate::info::resolved_function_sig_parts(db, method_loc, None).render(
            db,
            file,
            crate::info::method_sig_style(),
        );
        let span = baml_compiler2_ppir::item_data::function_source_map(db, method_loc).span;
        let name = m.name.as_str().to_string();
        if baml_compiler2_ppir::item_data::function_has_body(db, method_loc) {
            defaulted.push(InterfaceMember {
                category: InterfaceMemberCategory::DefaultMethod,
                name,
                declaration: format!("{signature} {{ ... }}"),
                docstring: m.docstring.clone(),
                body: Some(strip_leading_doc_lines(&clean_body_source(db, file, span))),
                file,
                file_path: file_path.clone(),
                item_range: span,
            });
        } else {
            required.push(InterfaceMember {
                category: InterfaceMemberCategory::RequiredMethod,
                name,
                declaration: signature,
                docstring: m.docstring.clone(),
                body: None,
                file,
                file_path: file_path.clone(),
                item_range: span,
            });
        }
    }
    out.extend(required);
    out.extend(defaulted);
    out
}

/// Collect the impl blocks whose head names this interface, as renderable
/// rows. Enumeration is the compiler's
/// ([`baml_compiler2_hir_ty::impls::impls_naming_interface`]); this only
/// projects each block into its display spelling and location.
fn collect_interface_impls(
    db: &dyn baml_compiler2_ppir::Db,
    iface_loc: baml_compiler2_hir::loc::InterfaceLoc<'_>,
) -> Vec<ImplRow> {
    baml_compiler2_hir_ty::impls::impls_naming_interface(db, iface_loc)
        .iter()
        .filter_map(|&block| {
            let facts = baml_compiler2_hir_ty::impls::impl_facts(db, block).as_ref()?;
            let data = baml_compiler2_ppir::item_data::impl_block_data(db, block);
            let file = block.file(db);
            let source_map = baml_compiler2_ppir::item_data::impl_block_source_map(db, block);
            Some(ImplRow {
                display: render_impl_row(facts),
                file,
                file_path: file_path_string(db, file),
                span: source_map.span,
                head_span: source_map.type_refs.span(data.interface_target),
            })
        })
        .collect()
}

/// `implement <Head> for <Target>`: the head's short name plus any written
/// generic arguments and associated-type pins, the for-target in the
/// canonical owner spelling (`int`, `T[]`, `user.Foo`). The head keeps its
/// SHORT name deliberately — every row sits under the interface it names, so
/// repeating the full path would be noise; the variation a reader scans for
/// is the instantiation and the implementor.
#[expect(
    deprecated,
    reason = "pre-D3: materializes interned inference state; moves to the finalized facts layer with the cutover"
)]
fn render_impl_row(facts: &baml_compiler2_hir_ty::impls::ImplFacts<'_>) -> String {
    let iface = &facts.interface;
    let mut head = iface.name.name().as_str().to_string();
    let mut args: Vec<String> = iface
        .generics
        .iter()
        .map(|generic| render::display_addressable_ty(&generic.to_plain()))
        .collect();
    args.extend(iface.associated_types.iter().map(|(name, ty)| {
        format!(
            "{} = {}",
            name.as_str(),
            render::display_addressable_ty(&ty.to_plain())
        )
    }));
    if !args.is_empty() {
        head.push('<');
        head.push_str(&args.join(", "));
        head.push('>');
    }
    format!(
        "implement {head} for {}",
        render::display_addressable_ty(&facts.for_ty_pattern.to_plain())
    )
}

/// Describe one interface member (drill-in): a method (required or
/// defaulted), a field, or an associated type. This is the target the member
/// enumeration points at — a required method shows its docstring and
/// signature (it HAS no body), a defaulted method also shows its body.
/// Returns `None` if the interface declares no such member.
fn describe_interface_member(
    db: &dyn baml_compiler2_ppir::Db,
    files: &[SourceFile],
    iface_loc: baml_compiler2_hir::loc::InterfaceLoc<'_>,
    member_name: &str,
) -> Option<SymbolDescription> {
    let file = iface_loc.file(db);
    let iface = baml_compiler2_ppir::item_data::interface_data(db, iface_loc);
    let source_map = baml_compiler2_ppir::item_data::interface_source_map(db, iface_loc);

    // The owning interface is the container for every member form.
    let container = crate::syntax::definition_span(db, Definition::Interface(iface_loc)).map(
        |(cfile, cspan)| DepRef {
            name: iface.name.as_str().to_string(),
            kind: DefinitionKind::Interface,
            file_path: file_path_string(db, cfile),
            file: cfile,
            name_span: cspan,
        },
    );

    // Methods — required and defaulted alike are real function items.
    if let Some(&method_loc) = iface.methods.iter().find(|&&loc| {
        let m = baml_compiler2_ppir::item_data::function_data(db, loc);
        m.name.as_str() == member_name && !m.metadata.is_language_internal
    }) {
        let m = baml_compiler2_ppir::item_data::function_data(db, method_loc);
        let method_map = baml_compiler2_ppir::item_data::function_source_map(db, method_loc);
        let method_span = method_map.span;
        let signature = crate::info::resolved_function_sig_parts(db, method_loc, None).render(
            db,
            file,
            crate::info::method_sig_style(),
        );
        let full_body = if baml_compiler2_ppir::item_data::function_has_body(db, method_loc) {
            docstring_prefixed_body(
                m.docstring.as_deref(),
                &clean_body_source(db, file, method_span),
            )
        } else {
            let mut body = docstring_lines(m.docstring.as_deref());
            body.push_str(&signature);
            body
        };
        // A spanless name (`TextRange::default()` fill) falls back to the
        // declaration span, same as `make_function_dep`.
        let name_span = if method_map.name_span == TextRange::default() {
            TextRange::empty(method_span.start())
        } else {
            method_map.name_span
        };
        let references = find_references(db, files, file, name_span, method_span);
        return Some(SymbolDescription {
            name: m.name.as_str().to_string(),
            kind: SymbolKind::Member {
                kind: MemberKind::Method,
                container,
            },
            file_path: file_path_string(db, file),
            file,
            name_span,
            item_range: method_span,
            shape: signature.clone(),
            full_body,
            docstring: m.docstring.clone(),
            resolved_type: Some(signature),
            dependencies: Vec::new(),
            references,
        });
    }

    // Fields — the shared member path handles FIELD nodes; it only needs the
    // name span, which the interface's source map records.
    if let Some((idx, _)) = iface
        .fields
        .iter()
        .enumerate()
        .find(|(_, f)| f.name.as_str() == member_name)
    {
        let name_span = source_map.field_name_spans.get(idx).copied()?;
        let sym = SymbolInfo {
            name: member_name.to_string(),
            kind: DefinitionKind::Field,
            file,
            name_span,
            container_name: Some(iface.name.as_str().to_string()),
        };
        return describe_member(db, files, &sym);
    }

    // Associated types.
    if let Some((idx, assoc)) = iface
        .associated_types
        .iter()
        .enumerate()
        .find(|(_, a)| a.name.as_str() == member_name)
    {
        let spans = source_map.associated_type_spans.get(idx)?;
        let declaration = crate::info::render_associated_type(iface, assoc);
        let references = find_references(db, files, file, spans.name_span, spans.span);
        return Some(SymbolDescription {
            name: member_name.to_string(),
            kind: SymbolKind::Member {
                kind: MemberKind::AssociatedType,
                container,
            },
            file_path: file_path_string(db, file),
            file,
            name_span: spans.name_span,
            item_range: spans.span,
            shape: format!("{}.{declaration}", iface.name.as_str()),
            full_body: declaration.clone(),
            docstring: None,
            resolved_type: Some(declaration),
            dependencies: Vec::new(),
            references,
        });
    }

    None
}

/// Describe a single class method (drill-in): its canonical signature, source
/// body, docstring, and owning class. The body is shown for user methods; a
/// builtin/native body (`$rust_function`, …) is elided to just the signature.
/// Returns `None` if the class has no such (non-auto-derived) method.
fn describe_class_method(
    db: &dyn baml_compiler2_ppir::Db,
    files: &[SourceFile],
    class_loc: baml_compiler2_hir::loc::ClassLoc<'_>,
    member_name: &str,
) -> Option<SymbolDescription> {
    use baml_compiler2_hir_ty::package_interface::ExportedType;

    let file = class_loc.file(db);
    let class_data = baml_compiler2_ppir::item_data::class_data(db, class_loc);

    // Locate the (non-auto-derived) method by name.
    let (idx, &method_loc) = class_data.methods.iter().enumerate().find(|(_, mid)| {
        let m = baml_compiler2_ppir::item_data::function_data(db, **mid);
        m.name.as_str() == member_name && !m.metadata.is_language_internal
    })?;
    let m = baml_compiler2_ppir::item_data::function_data(db, method_loc);
    let method_span = baml_compiler2_ppir::item_data::function_source_map(db, method_loc).span;

    let pkg_info = baml_compiler2_hir::file_package::file_package(db, file);
    let pkg_id = baml_compiler2_hir::package::PackageId::new(db, pkg_info.package.clone());
    let iface = baml_compiler2_hir_ty::package_interface::package_interface(db, pkg_id);
    let ef = iface
        .lookup_type(&pkg_info.namespace_path, &class_data.name)
        .and_then(|t| match t {
            ExportedType::Class { methods, .. } => {
                crate::info::exported_method(methods, idx, &m.name)
            }
            _ => None,
        });
    let signature = crate::info::resolved_function_sig_parts(db, method_loc, ef).render(
        db,
        file,
        crate::info::method_sig_style(),
    );

    // Drill-in shows the body; a builtin native body is elided to the signature.
    let full_body = if matches!(
        baml_compiler2_ppir::function_body(db, method_loc).as_ref(),
        baml_compiler2_hir::body::FunctionBody::Builtin(_)
    ) {
        let mut body = docstring_lines(m.docstring.as_deref());
        body.push_str(&signature);
        body
    } else {
        docstring_prefixed_body(
            m.docstring.as_deref(),
            &clean_body_source(db, file, method_span),
        )
    };

    let name_span = function_def_name_span(db, file, method_span, member_name)
        .unwrap_or_else(|| TextRange::empty(method_span.start()));

    // The owning class is the container.
    let container =
        crate::syntax::definition_span(db, Definition::Class(class_loc)).map(|(cfile, cspan)| {
            DepRef {
                name: class_data.name.as_str().to_string(),
                kind: DefinitionKind::Class,
                file_path: file_path_string(db, cfile),
                file: cfile,
                name_span: cspan,
            }
        });

    let references = find_references(db, files, file, name_span, method_span);

    Some(SymbolDescription {
        name: m.name.as_str().to_string(),
        kind: SymbolKind::Member {
            kind: MemberKind::Method,
            container,
        },
        file_path: file_path_string(db, file),
        file,
        name_span,
        item_range: method_span,
        shape: signature.clone(),
        full_body,
        docstring: m.docstring.clone(),
        resolved_type: Some(signature),
        dependencies: Vec::new(),
        references,
    })
}

/// The byte range of a method's name token within its `FUNCTION_DEF` node.
/// `span` is the method's full source span. Used to anchor reference search at
/// the name (not the leading `function` keyword / trivia).
fn function_def_name_span(
    db: &dyn baml_compiler2_ppir::Db,
    file: SourceFile,
    span: TextRange,
    name: &str,
) -> Option<TextRange> {
    let tree = baml_compiler_parser::syntax_tree(db, file);
    let node = match tree.covering_element(span) {
        rowan::NodeOrToken::Node(n) => n,
        rowan::NodeOrToken::Token(t) => t.parent()?,
    };
    let func = if node.kind() == SyntaxKind::FUNCTION_DEF {
        node
    } else {
        node.ancestors()
            .find(|n| n.kind() == SyntaxKind::FUNCTION_DEF)?
    };
    func.children_with_tokens()
        .filter_map(rowan::NodeOrToken::into_token)
        .find(|t| t.kind() == SyntaxKind::WORD && t.text() == name)
        .map(|t| t.text_range())
}

/// Collect dependency types referenced in a class's method signatures
/// (parameter, return, and inferred-throws types), using the resolved
/// package-interface signatures. Auto-derived methods are skipped; builtin
/// types resolve to nothing in the user-file outline; the class's own type is
/// already in `seen`, so it is never listed as its own dependency.
fn collect_method_signature_deps(
    db: &dyn baml_compiler2_ppir::Db,
    files: &[SourceFile],
    class_loc: baml_compiler2_hir::loc::ClassLoc<'_>,
    deps: &mut Vec<DepRef>,
    seen: &mut std::collections::HashSet<String>,
) {
    use baml_compiler2_hir_ty::package_interface::ExportedType;

    let file = class_loc.file(db);
    let class = baml_compiler2_ppir::item_data::class_data(db, class_loc);

    let pkg_info = baml_compiler2_hir::file_package::file_package(db, file);
    let pkg_id = baml_compiler2_hir::package::PackageId::new(db, pkg_info.package.clone());
    let iface = baml_compiler2_hir_ty::package_interface::package_interface(db, pkg_id);
    let Some(methods) = iface
        .lookup_type(&pkg_info.namespace_path, &class.name)
        .and_then(|t| match t {
            ExportedType::Class { methods, .. } => Some(methods),
            _ => None,
        })
    else {
        return;
    };

    for (idx, method_loc) in class.methods.iter().enumerate() {
        let m = baml_compiler2_ppir::item_data::function_data(db, *method_loc);
        if m.metadata.is_language_internal {
            continue;
        }
        let Some(ef) = crate::info::exported_method(methods, idx, &m.name) else {
            continue;
        };
        for param in &ef.params {
            collect_ty_deps(db, files, &param.ty, deps, seen);
        }
        collect_ty_deps(db, files, &ef.return_type, deps, seen);
        collect_ty_deps(db, files, &ef.callable_throws, deps, seen);
    }
}

/// Resolve a top-level symbol's canonical FQN for its description header.
///
/// `Some` only when it differs from the bare `sym.name` — i.e. when the
/// header should show it in parentheses (a builtin alias like `string`, or a
/// namespaced/dependency type like `root.ns.Foo`). A user type at package
/// root (or an unresolved symbol) returns `None`.
fn canonical_fqn(
    db: &dyn baml_compiler2_ppir::Db,
    sym: &SymbolInfo,
    def: Option<Definition<'_>>,
) -> Option<String> {
    let def = def?;
    let name = baml_base::Name::new(&sym.name);
    let qtn = baml_compiler2_hir_ty::lower::qualify_def(db, def, &name);
    let fqn = qtn.render_addressable();
    (fqn != sym.name).then_some(fqn)
}

// ── Type resolution ──────────────────────────────────────────────────────────

/// Resolve the type of a field or variant by looking it up in the parent
/// class/enum's resolved fields, rather than going through `resolve_name_at`
/// (which only handles top-level `Definition` items, not members).
fn resolve_member_type(
    db: &dyn baml_compiler2_ppir::Db,
    file: SourceFile,
    sym: &SymbolInfo,
) -> Option<String> {
    let container_name = sym.container_name.as_ref()?;
    let container_baml_name = baml_base::Name::new(container_name);
    let resolved = baml_compiler2_ppir::resolve::resolve_name_at(
        db,
        file,
        sym.name_span.start(),
        &container_baml_name,
    );

    let (baml_compiler2_ppir::resolve::ResolvedName::Item(def)
    | baml_compiler2_ppir::resolve::ResolvedName::Builtin(def)) = resolved
    else {
        return None;
    };

    match (sym.kind, def) {
        (
            DefinitionKind::Field,
            baml_compiler2_hir::contributions::Definition::Class(class_loc),
        ) => {
            let resolved_fields = baml_compiler2_hir_ty::lower::resolve_class_fields(db, class_loc);
            resolved_fields
                .iter()
                .find(|(field_name, _, _)| field_name.as_str() == sym.name)
                .map(|(_, ty, _)| render::display_ty_for_file(db, file, ty))
        }
        (DefinitionKind::Variant, baml_compiler2_hir::contributions::Definition::Enum(_)) => {
            // Enum variants don't have a meaningful type beyond the enum itself.
            Some(container_name.clone())
        }
        _ => None,
    }
}

/// Resolve the type of a top-level symbol to a user-friendly string.
///
/// For functions: `(a: string, b: int) -> ReturnType`
/// For classes: field list with types
/// For enums: variant list
/// For type aliases: the expansion
/// For locals: the inferred type
fn resolve_type_for_item(
    db: &dyn baml_compiler2_ppir::Db,
    def: Option<Definition<'_>>,
) -> Option<String> {
    use crate::info::TypeInfo;

    let type_info = type_info_for_definition(db, def?);
    match type_info {
        TypeInfo::Function {
            params,
            return_type,
            ..
        } => {
            let param_strs: Vec<String> = params
                .iter()
                .map(crate::info::FunctionParamInfo::render)
                .collect();
            let ret = return_type.map(|r| format!(" -> {r}")).unwrap_or_default();
            Some(format!("({}){}", param_strs.join(", "), ret))
        }
        TypeInfo::Class { fields, .. } => {
            if fields.is_empty() {
                Some("{}".to_string())
            } else {
                let field_strs: Vec<String> =
                    fields.iter().map(|(n, t)| format!("{n}: {t}")).collect();
                Some(format!("{{ {} }}", field_strs.join(", ")))
            }
        }
        TypeInfo::Enum { variants, .. } => Some(format!("{{ {} }}", variants.join(", "))),
        // The one-line type column stays the header; the member surface is
        // the shape block's job.
        TypeInfo::Interface {
            name,
            generic_params,
            ..
        } => {
            let generics = if generic_params.is_empty() {
                String::new()
            } else {
                format!("<{}>", generic_params.join(", "))
            };
            Some(format!("interface {name}{generics}"))
        }
        TypeInfo::TypeAlias { expansion, .. } => Some(expansion),
        TypeInfo::TemplateString { .. } => Some("template_string".to_string()),
        TypeInfo::LocalVar { ty, .. } => Some(ty),
        TypeInfo::Symbol { declaration, .. } => Some(declaration),
        TypeInfo::Documentation { label, .. } => Some(label),
        TypeInfo::OtherItem { kind, .. } => Some(kind.to_string()),
    }
}

// ── Docstring extraction ─────────────────────────────────────────────────────

/// Extract leading `///` doc comments from the CST node preceding the item.
fn extract_docstring(
    db: &dyn baml_compiler2_ppir::Db,
    file: SourceFile,
    item_range: TextRange,
) -> Option<String> {
    let tree = baml_compiler_parser::syntax_tree(db, file);

    let token = match tree.token_at_offset(item_range.start()) {
        rowan::TokenAtOffset::Single(t) => t,
        rowan::TokenAtOffset::Between(_, right) => right,
        rowan::TokenAtOffset::None => return None,
    };

    let item_node = token.parent_ancestors().find(|n| is_item_node(n.kind()))?;
    baml_compiler2_ast::extract_docstring(&item_node)
}

// ── Dependency discovery ─────────────────────────────────────────────────────

/// Find signature-level dependencies for a symbol.
///
/// For functions: parameter types, return type.
/// For classes: field types that are user-defined.
/// For other kinds: empty (self-contained or not applicable).
fn find_dependencies(
    db: &dyn baml_compiler2_ppir::Db,
    files: &[SourceFile],
    file: SourceFile,
    sym: &SymbolInfo,
    def: Option<Definition<'_>>,
) -> Vec<DepRef> {
    let Some(def) = def else {
        return Vec::new();
    };

    let mut deps = Vec::new();
    let mut seen = std::collections::HashSet::new();
    seen.insert(sym.name.clone());

    match def {
        baml_compiler2_hir::contributions::Definition::Function(func_loc) => {
            let sig = baml_compiler2_hir::signature::function_signature(db, func_loc);
            // Collect type names from params and return type.
            for param in &sig.params {
                collect_type_expr_deps(db, file, &param.ty, &mut deps, &mut seen);
            }
            if let Some(ret) = &sig.return_type {
                collect_type_expr_deps(db, file, ret, &mut deps, &mut seen);
            }
        }
        baml_compiler2_hir::contributions::Definition::Class(class_loc) => {
            let resolved = baml_compiler2_hir_ty::lower::resolve_class_fields(db, class_loc);
            for (_field_name, ty, _attrs) in resolved {
                collect_ty_deps(db, files, ty, &mut deps, &mut seen);
            }
            // Types referenced in method signatures (params/return/throws) are
            // dependencies too — e.g. `WrapperMarker` in `-> T | WrapperMarker`.
            // The class's own name is in `seen`, so it is never its own dep.
            collect_method_signature_deps(db, files, class_loc, &mut deps, &mut seen);
        }
        baml_compiler2_hir::contributions::Definition::Enum(_) => {
            // Enums are self-contained, no type dependencies.
        }
        baml_compiler2_hir::contributions::Definition::Interface(iface_loc) => {
            let iface = baml_compiler2_ppir::item_data::interface_data(db, iface_loc);
            for field in &iface.fields {
                collect_type_ref_deps(
                    db,
                    file,
                    &iface.type_refs,
                    field.type_ref,
                    &mut deps,
                    &mut seen,
                );
            }
            for &parent in &iface.requires {
                collect_type_ref_deps(db, file, &iface.type_refs, parent, &mut deps, &mut seen);
            }
        }
        baml_compiler2_hir::contributions::Definition::TypeAlias(alias_loc) => {
            // Walk the alias's target type reference.
            let alias = baml_compiler2_ppir::item_data::type_alias_data(db, alias_loc);
            if let Some(id) = alias.value {
                collect_type_ref_deps(db, file, &alias.type_refs, id, &mut deps, &mut seen);
            }
        }
        baml_compiler2_hir::contributions::Definition::Client(client_loc) => {
            // Surface the retry policy reference if present.
            let client = baml_compiler2_ppir::item_data::client_data(db, client_loc);
            if let Some(ref policy_name) = client.retry_policy_name {
                let name_str = policy_name.as_str().to_string();
                if seen.insert(name_str.clone()) {
                    if let Some(dep) = resolve_dep_from_outline(db, files, &name_str) {
                        deps.push(dep);
                    }
                }
            }
        }
        baml_compiler2_hir::contributions::Definition::Test(test_loc) => {
            // Extract the test's function references.
            let test = baml_compiler2_ppir::item_data::test_data(db, test_loc);
            for func_name in &test.function_refs {
                let name_str = func_name.as_str().to_string();
                if seen.insert(name_str.clone()) {
                    if let Some(dep) = resolve_dep_from_outline(db, files, &name_str) {
                        deps.push(dep);
                    }
                }
            }
        }
        baml_compiler2_hir::contributions::Definition::TemplateString(_) => {
            // Template string params are plain names with no types; self-contained.
        }
        baml_compiler2_hir::contributions::Definition::RetryPolicy(_)
        | baml_compiler2_hir::contributions::Definition::Let(_) => {
            // These don't have meaningful type dependencies for display.
        }
    }

    deps
}

/// Walk a `TypeExpr` and collect user-defined type names as `DepRefs`.
fn collect_type_expr_deps(
    db: &dyn baml_compiler2_ppir::Db,
    file: SourceFile,
    te: &baml_compiler2_ast::TypeExpr,
    deps: &mut Vec<DepRef>,
    seen: &mut std::collections::HashSet<String>,
) {
    use baml_compiler2_ast::TypeExprKind;
    match &te.kind {
        TypeExprKind::Path {
            segments,
            generic_args,
            ..
        } => {
            if let Some(last) = segments.last() {
                let name_str = last.as_str().to_string();
                if seen.insert(name_str.clone()) {
                    // Try to resolve this name to find its definition.
                    if let Some(dep) = resolve_dep(db, file, &name_str) {
                        deps.push(dep);
                    }
                }
            }
            for ga in generic_args {
                collect_type_expr_deps(db, file, ga, deps, seen);
            }
        }
        TypeExprKind::Optional { inner, .. } | TypeExprKind::List { inner, .. } => {
            collect_type_expr_deps(db, file, inner, deps, seen);
        }
        TypeExprKind::Map { key, value, .. } => {
            collect_type_expr_deps(db, file, key, deps, seen);
            collect_type_expr_deps(db, file, value, deps, seen);
        }
        TypeExprKind::Union { variants, .. } => {
            for v in variants {
                collect_type_expr_deps(db, file, v, deps, seen);
            }
        }
        TypeExprKind::Function {
            params,
            ret,
            throws,
            ..
        } => {
            for p in params {
                collect_type_expr_deps(db, file, &p.ty, deps, seen);
            }
            collect_type_expr_deps(db, file, ret, deps, seen);
            if let Some(throws) = throws {
                collect_type_expr_deps(db, file, throws, deps, seen);
            }
        }
        // Primitives and literals have no user-defined deps.
        _ => {}
    }
}

/// Span-free twin of [`collect_type_expr_deps`], walking a `TypeRefStore` arena
/// (the firewall's item-data type-reference representation) instead of a spanned
/// `ast::TypeExpr`. Behavior is identical: only the last path segment is treated
/// as a candidate dependency name; associated-type projections and primitives
/// contribute none.
fn collect_type_ref_deps(
    db: &dyn baml_compiler2_ppir::Db,
    file: SourceFile,
    store: &baml_compiler2_hir::type_ref::TypeRefStore,
    id: baml_compiler2_hir::type_ref::TypeRefId,
    deps: &mut Vec<DepRef>,
    seen: &mut std::collections::HashSet<String>,
) {
    use baml_compiler2_hir::type_ref::TypeRefKind;
    match &store[id].kind {
        TypeRefKind::Path {
            segments,
            generic_args,
            ..
        } => {
            if let Some(last) = segments.last() {
                let name_str = last.as_str().to_string();
                if seen.insert(name_str.clone()) {
                    // Try to resolve this name to find its definition.
                    if let Some(dep) = resolve_dep(db, file, &name_str) {
                        deps.push(dep);
                    }
                }
            }
            for &ga in generic_args {
                collect_type_ref_deps(db, file, store, ga, deps, seen);
            }
        }
        TypeRefKind::Optional { inner } | TypeRefKind::List { inner } => {
            collect_type_ref_deps(db, file, store, *inner, deps, seen);
        }
        TypeRefKind::Map { key, value } => {
            collect_type_ref_deps(db, file, store, *key, deps, seen);
            collect_type_ref_deps(db, file, store, *value, deps, seen);
        }
        TypeRefKind::Union { variants } => {
            for &v in variants {
                collect_type_ref_deps(db, file, store, v, deps, seen);
            }
        }
        TypeRefKind::Function {
            params,
            ret,
            throws,
        } => {
            for p in params {
                collect_type_ref_deps(db, file, store, p.ty, deps, seen);
            }
            collect_type_ref_deps(db, file, store, *ret, deps, seen);
            if let Some(throws) = throws {
                collect_type_ref_deps(db, file, store, *throws, deps, seen);
            }
        }
        // Primitives, literals, and associated-type projections have no
        // user-defined deps (matching `collect_type_expr_deps`).
        _ => {}
    }
}

/// Walk a resolved Ty and collect user-defined type names as `DepRefs`.
/// Resolve a qualified type name to a dependency via outline search and push it
/// (deduped). Only **local** (user-package) types are surfaced: builtin and
/// dependency types (`baml.json.json`, `baml.errors.InvalidArgument`, …) are
/// well-known and would just be noise. Lookup and dedup key on the **short**
/// name, matching the flat outline index; the owning symbol's name is pre-seeded
/// into `seen`, so a type is never listed as a dependency of itself.
fn collect_qtn_dep(
    db: &dyn baml_compiler2_ppir::Db,
    files: &[SourceFile],
    qtn: &baml_type::QualifiedTypeName,
    deps: &mut Vec<DepRef>,
    seen: &mut std::collections::HashSet<String>,
) {
    if !qtn.is_local() {
        return;
    }
    let short = qtn.name().as_str().to_string();
    if seen.insert(short.clone()) {
        if let Some(dep) = resolve_dep_from_outline(db, files, &short) {
            deps.push(dep);
        }
    }
}

fn collect_ty_deps(
    db: &dyn baml_compiler2_ppir::Db,
    files: &[SourceFile],
    ty: &baml_type::Ty,
    deps: &mut Vec<DepRef>,
    seen: &mut std::collections::HashSet<String>,
) {
    use baml_type::Ty;
    match ty {
        Ty::Class(qtn, generics, _) => {
            collect_qtn_dep(db, files, qtn, deps, seen);
            for generic in generics {
                collect_ty_deps(db, files, generic, deps, seen);
            }
        }
        Ty::Enum(qtn, _) | Ty::TypeAlias(qtn, _) => {
            collect_qtn_dep(db, files, qtn, deps, seen);
        }
        Ty::List(inner, _) => {
            collect_ty_deps(db, files, inner, deps, seen);
        }
        Ty::Map {
            key: k, value: v, ..
        } => {
            collect_ty_deps(db, files, k, deps, seen);
            collect_ty_deps(db, files, v, deps, seen);
        }
        Ty::Union(members, _) => {
            for m in members {
                collect_ty_deps(db, files, m, deps, seen);
            }
        }
        Ty::Function {
            params,
            ret,
            throws,
            ..
        } => {
            for param in params {
                collect_ty_deps(db, files, &param.ty, deps, seen);
            }
            collect_ty_deps(db, files, ret, deps, seen);
            collect_ty_deps(db, files, throws, deps, seen);
        }
        _ => {}
    }
}

/// Try to resolve a type name to a `DepRef` by looking it up in the outline.
fn resolve_dep_from_outline(
    db: &dyn baml_compiler2_ppir::Db,
    files: &[SourceFile],
    name: &str,
) -> Option<DepRef> {
    // Search all files' outlines for a symbol matching `name`.
    for &file in files {
        let outline = crate::outline::file_outline(db, file);
        for item in outline {
            if item.name == name {
                return Some(DepRef {
                    name: item.name.clone(),
                    kind: item.kind,
                    file_path: file_path_string(db, file),
                    file,
                    name_span: item.name_span,
                });
            }
        }
    }
    None
}

/// Try to resolve a name to a `DepRef` using name resolution.
fn resolve_dep(
    db: &dyn baml_compiler2_ppir::Db,
    context_file: SourceFile,
    name: &str,
) -> Option<DepRef> {
    let baml_name = baml_base::Name::new(name);
    // Use offset 0 — we just need scope-level resolution for the file.
    let resolved = baml_compiler2_ppir::resolve::resolve_name_at(
        db,
        context_file,
        text_size::TextSize::from(0),
        &baml_name,
    );

    let (baml_compiler2_ppir::resolve::ResolvedName::Item(def)
    | baml_compiler2_ppir::resolve::ResolvedName::Builtin(def)) = resolved
    else {
        return None;
    };

    let (dep_file, name_span) = crate::syntax::definition_span(db, def)?;

    Some(DepRef {
        name: name.to_string(),
        kind: def.kind(),
        file_path: file_path_string(db, dep_file),
        file: dep_file,
        name_span,
    })
}

// ── Reference finding ────────────────────────────────────────────────────────

/// Find all reference sites for a symbol, excluding the definition itself.
fn find_references(
    db: &dyn baml_compiler2_ppir::Db,
    _files: &[SourceFile],
    file: SourceFile,
    name_span: TextRange,
    item_range: TextRange,
) -> Vec<RefSite> {
    let locations = usages_at(db, file, name_span.start());

    locations
        .into_iter()
        // Exclude references inside the symbol's own definition (the name token
        // and any self-references in its body — e.g. a class used in its own
        // method signatures/bodies). `references` lists *external* usages.
        .filter(|loc| !(loc.file == file && item_range.contains(loc.range.start())))
        .map(|loc| {
            let text = loc.file.text(db);
            let (line_number, line_text) = line_at_offset(text, loc.range.start());
            RefSite {
                file_path: file_path_string(db, loc.file),
                file: loc.file,
                range: loc.range,
                line_text,
                line_number,
            }
        })
        .collect()
}

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Get the file path as a displayable string.
fn file_path_string(db: &dyn baml_compiler2_ppir::Db, file: SourceFile) -> String {
    file.path(db).display().to_string()
}

/// Render a docstring back as `/// `-prefixed source lines (trailing
/// newline included), or an empty string for `None`.
fn docstring_lines(docstring: Option<&str>) -> String {
    let mut out = String::new();
    for line in docstring.unwrap_or_default().lines() {
        if line.is_empty() {
            out.push_str("///\n");
        } else {
            out.push_str("/// ");
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

/// Drop the leading `///` doc-comment lines from a cleaned member source.
/// Used where the docstring is its own facet, rendered (and budgeted)
/// separately from the definition text.
fn strip_leading_doc_lines(text: &str) -> String {
    text.lines()
        .skip_while(|line| line.trim_start().starts_with("///"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// A method drill-in body: the docstring as `///` lines (indented to match
/// the body's first line, so the slice reads as it does in the file)
/// followed by the definition source. Method source-map spans start at the
/// `function` keyword, so the docstring must be re-attached — and any doc
/// lines the slice does carry are stripped first, so no span shape can
/// double them.
fn docstring_prefixed_body(docstring: Option<&str>, body: &str) -> String {
    let body = strip_leading_doc_lines(body);
    let indent: String = body
        .lines()
        .next()
        .map(|line| line.chars().take_while(|c| c.is_whitespace()).collect())
        .unwrap_or_default();
    let mut out = String::new();
    for line in docstring.unwrap_or_default().lines() {
        out.push_str(&indent);
        if line.is_empty() {
            out.push_str("///\n");
        } else {
            out.push_str("/// ");
            out.push_str(line);
            out.push('\n');
        }
    }
    out.push_str(&body);
    out
}

/// Slice a text range from file text, trimming leading blank lines.
/// The source text over `range` with non-doc comments removed.
///
/// Comment removal is **CST-token based**: only `LINE_COMMENT` (excluding `///`
/// docs), `BLOCK_COMMENT`, and `HEADER_COMMENT` tokens are dropped. A `//`
/// sequence inside a string/prompt literal (`#"…"#`) is a string token, not a
/// comment token, so it is never touched. A whole-line comment is removed with
/// its indentation and trailing newline; a trailing inline comment is removed
/// in place, keeping the code before it. `///` doc comments are kept.
fn clean_body_source(
    db: &dyn baml_compiler2_ppir::Db,
    file: SourceFile,
    range: TextRange,
) -> String {
    let text = file.text(db);
    let end: usize = range.end().into();
    // Show the body from the start of its first line so the original
    // indentation is preserved. Callers pass either a trimmed item span
    // (starts at the first real token) or a raw CST node range (starts at
    // leading trivia), so first advance to the first non-whitespace char,
    // then back up to that line's start — anchoring on trivia would pull in
    // the previous line (e.g. the enclosing `class … {`).
    let raw_start: usize = range.start().into();
    let first_real = text[raw_start..end]
        .find(|c: char| !c.is_whitespace())
        .map_or(raw_start, |off| raw_start + off);
    let start = text[..first_real].rfind('\n').map_or(0, |i| i + 1);
    let Some(src) = text.get(start..end) else {
        return String::new();
    };

    // Deletion intervals, relative to `src`, for every non-doc comment token.
    let tree = baml_compiler_parser::syntax_tree(db, file);
    let mut deletions: Vec<(usize, usize)> = Vec::new();
    for token in tree
        .descendants_with_tokens()
        .filter_map(baml_compiler_syntax::NodeOrToken::into_token)
    {
        if !token.kind().is_comment() {
            continue;
        }
        if token.kind() == SyntaxKind::LINE_COMMENT && token.text().starts_with("///") {
            continue; // keep doc comments
        }
        let tr = token.text_range();
        let (ts, te): (usize, usize) = (tr.start().into(), tr.end().into());
        if ts < start || te > end {
            continue;
        }
        let (mut del_start, mut del_end) = (ts - start, te - start);
        // Whole-line comment → also drop its indentation and trailing newline.
        let line_start = src[..del_start].rfind('\n').map_or(0, |i| i + 1);
        if src[line_start..del_start].trim().is_empty() {
            del_start = line_start;
            if let Some(rest) = src.get(del_end..) {
                if rest.starts_with("\r\n") {
                    del_end += 2;
                } else if rest.starts_with('\n') {
                    del_end += 1;
                }
            }
        }
        deletions.push((del_start, del_end));
    }
    deletions.sort_unstable();

    let mut out = String::with_capacity(src.len());
    let mut cursor = 0;
    for (del_start, del_end) in deletions {
        if del_start >= cursor {
            out.push_str(&src[cursor..del_start]);
            cursor = del_end;
        } else {
            cursor = cursor.max(del_end);
        }
    }
    out.push_str(&src[cursor..]);

    out.trim_start_matches('\n').trim_end().to_string()
}

/// If `sym` is a function with a builtin (native) body — `$rust_function`,
/// `$rust_io_function`, `$compiler_intrinsic` — return the range of its
/// signature (everything before the body block), so the body and its
/// implementation marker are never shown. The body block start comes from the
/// CST node boundary, so this is robust to any body formatting.
///
/// Returns `None` for user functions (whose body is meaningful and shown) and
/// non-function symbols.
fn builtin_signature_range(
    db: &dyn baml_compiler2_ppir::Db,
    file: SourceFile,
    sym: &SymbolInfo,
    item_range: TextRange,
    def: Option<Definition<'_>>,
) -> Option<TextRange> {
    if sym.kind != DefinitionKind::Function {
        return None;
    }

    // Confirm the body is builtin (not a user expression) via the HIR.
    let Definition::Function(func_loc) = def? else {
        return None;
    };
    if !matches!(
        baml_compiler2_ppir::function_body(db, func_loc).as_ref(),
        baml_compiler2_hir::body::FunctionBody::Builtin(_)
    ) {
        return None;
    }

    // Find the body block node and cut the range just before it.
    let tree = baml_compiler_parser::syntax_tree(db, file);
    let token = match tree.token_at_offset(sym.name_span.start()) {
        rowan::TokenAtOffset::Single(t) => t,
        rowan::TokenAtOffset::Between(_, right) => right,
        rowan::TokenAtOffset::None => return None,
    };
    let func_node = token
        .parent_ancestors()
        .find(|n| n.kind() == SyntaxKind::FUNCTION_DEF)?;
    let body_node = func_node.descendants().find(|n| {
        matches!(
            n.kind(),
            SyntaxKind::EXPR_FUNCTION_BODY
                | SyntaxKind::LLM_FUNCTION_BODY
                | SyntaxKind::FUNCTION_BODY
        )
    })?;

    Some(TextRange::new(
        item_range.start(),
        body_node.text_range().start(),
    ))
}

/// Find the 1-based line number and full line text at a byte offset.
fn line_at_offset(text: &str, offset: text_size::TextSize) -> (usize, String) {
    let offset: usize = offset.into();
    let offset = offset.min(text.len());

    let line_number = text[..offset].chars().filter(|&c| c == '\n').count() + 1;

    let line_start = text[..offset].rfind('\n').map(|p| p + 1).unwrap_or(0);
    let line_end = text[offset..]
        .find('\n')
        .map(|p| offset + p)
        .unwrap_or(text.len());

    (
        line_number,
        text[line_start..line_end].trim_end().to_string(),
    )
}

// ── Serde helpers ────────────────────────────────────────────────────────────

// serde's `serialize_with` contract requires `&T` — suppress the copy-by-ref lint.
#[allow(clippy::trivially_copy_pass_by_ref)]
fn serialize_kind<S: serde::Serializer>(kind: &DefinitionKind, s: S) -> Result<S::Ok, S::Error> {
    s.serialize_str(kind.as_str())
}

#[allow(clippy::trivially_copy_pass_by_ref)]
fn serialize_range<S: serde::Serializer>(range: &TextRange, s: S) -> Result<S::Ok, S::Error> {
    use serde::ser::SerializeStruct;
    let mut state = s.serialize_struct("Range", 2)?;
    state.serialize_field("start", &u32::from(range.start()))?;
    state.serialize_field("end", &u32::from(range.end()))?;
    state.end()
}

#[cfg(test)]
mod tests {
    use std::fmt::Write as _;

    use baml_compiler2_hir::package::sole_workspace_package;

    use super::SymbolDescription;
    use crate::test_support::{ProjectTest, offset_to_line_col};

    /// Feature-side conveniences over the shared project harness.
    trait DescribeExt {
        fn describe(&self, name: &str) -> Vec<SymbolDescription>;
        /// `describe()` over the compiler2-visible file set (stdlib included).
        fn describe_compiler2_visible(&self, name: &str) -> Vec<SymbolDescription>;
        /// Stable plain-text form of a description for snapshots.
        fn format_description(&self, desc: &SymbolDescription) -> String;
    }

    impl DescribeExt for ProjectTest {
        fn describe(&self, name: &str) -> Vec<SymbolDescription> {
            super::describe(&self.db, &self.files, name)
        }

        fn describe_compiler2_visible(&self, name: &str) -> Vec<SymbolDescription> {
            let files = baml_compiler2_hir::compiler2_all_files(&self.db);
            super::describe(&self.db, &files, name)
        }

        fn format_description(&self, desc: &SymbolDescription) -> String {
            let mut out = String::new();
            let filename = desc
                .file
                .path(&self.db)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown")
                .to_string();
            let text = desc.file.text(&self.db);
            let offset: usize = desc.name_span.start().into();
            let (line, _col) = offset_to_line_col(text, offset);

            let fqn = desc
                .kind
                .canonical_fqn()
                .map(|f| format!("  ({f})"))
                .unwrap_or_default();
            writeln!(
                out,
                "{} {}{}  {}:{}",
                desc.kind.definition_kind(),
                desc.name,
                fqn,
                filename,
                line
            )
            .unwrap();
            if let Some(ref doc) = desc.docstring {
                for line in doc.lines() {
                    if line.is_empty() {
                        writeln!(out, "///").unwrap();
                    } else {
                        writeln!(out, "/// {line}").unwrap();
                    }
                }
            }
            writeln!(out, "shape: {}", desc.shape).unwrap();
            if let super::SymbolKind::Interface {
                members,
                implementations,
                ..
            } = &desc.kind
            {
                if !members.is_empty() {
                    writeln!(out, "members:").unwrap();
                    for m in members {
                        writeln!(out, "  [{}] {}", m.category.as_str(), m.declaration).unwrap();
                        if let Some(doc) = &m.docstring {
                            for line in doc.lines() {
                                if line.is_empty() {
                                    writeln!(out, "    ///").unwrap();
                                } else {
                                    writeln!(out, "    /// {line}").unwrap();
                                }
                            }
                        }
                        if let Some(body) = &m.body {
                            writeln!(out, "    (body: {} lines)", body.lines().count()).unwrap();
                        }
                    }
                }
                if !implementations.is_empty() {
                    writeln!(out, "implementations:").unwrap();
                    for imp in implementations {
                        writeln!(out, "  {}", imp.display).unwrap();
                    }
                }
            }
            if let super::SymbolKind::Class {
                instance_methods,
                static_methods,
                ..
            } = &desc.kind
            {
                if !instance_methods.is_empty() {
                    writeln!(out, "methods:").unwrap();
                    for m in instance_methods {
                        if let Some(doc) = &m.docstring {
                            writeln!(out, "  /// {doc}").unwrap();
                        }
                        writeln!(out, "  {}", m.signature).unwrap();
                    }
                }
                if !static_methods.is_empty() {
                    writeln!(out, "static_methods:").unwrap();
                    for m in static_methods {
                        if let Some(doc) = &m.docstring {
                            writeln!(out, "  /// {doc}").unwrap();
                        }
                        writeln!(out, "  {}", m.signature).unwrap();
                    }
                }
            }
            if let Some(c) = desc.kind.container() {
                writeln!(out, "container: {}", c.name).unwrap();
            }
            if !desc.dependencies.is_empty() {
                out.push_str("deps:");
                for dep in &desc.dependencies {
                    write!(out, " {}", dep.name).unwrap();
                }
                out.push('\n');
            }
            if !desc.references.is_empty() {
                writeln!(out, "refs: {}", desc.references.len()).unwrap();
                for r in &desc.references {
                    let rfile = r
                        .file
                        .path(&self.db)
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("unknown")
                        .to_string();
                    writeln!(out, "  {}:{}  {}", rfile, r.line_number, r.line_text.trim()).unwrap();
                }
            }
            out
        }
    }

    fn make_multi_ns_project() -> ProjectTest {
        let mut builder = ProjectTest::builder();
        builder.source(
            "types.baml",
            r#"
class Point {
    x int
    y int
}
"#,
        );
        builder.source(
            "ns_llm/models.baml",
            r#"
class Config {
    model string
    temperature float
}
"#,
        );
        builder.build()
    }

    #[test]
    fn describe_by_definition_class_in_namespace() {
        let project = make_multi_ns_project();
        let pkg_id = sole_workspace_package(&project.db);
        let pkg = baml_compiler2_hir::package::package_items(&project.db, pkg_id);

        let ns_path = vec![baml_base::Name::new("llm")];
        let item_name = baml_base::Name::new("Config");
        let def = pkg.lookup_type(&ns_path, &item_name).unwrap();

        let files = baml_compiler2_hir::compiler2_all_files(&project.db);
        let desc = super::describe_by_definition(&project.db, &files, def).unwrap();
        assert_eq!(desc.name, "Config");
        assert_eq!(desc.kind.definition_kind(), crate::DefinitionKind::Class);
    }

    #[test]
    fn describe_item_member_field() {
        let project = make_multi_ns_project();
        let pkg_id = sole_workspace_package(&project.db);
        let pkg = baml_compiler2_hir::package::package_items(&project.db, pkg_id);

        let root_ns: Vec<baml_base::Name> = vec![];
        let item_name = baml_base::Name::new("Point");
        let def = pkg.lookup_type(&root_ns, &item_name).unwrap();

        let files = baml_compiler2_hir::compiler2_all_files(&project.db);
        let desc = super::describe_item_member(&project.db, &files, def, "x").unwrap();
        assert_eq!(desc.name, "x");
        assert_eq!(desc.kind.definition_kind(), crate::DefinitionKind::Field);
    }

    #[test]
    fn describe_item_member_nonexistent() {
        let project = make_multi_ns_project();
        let pkg_id = sole_workspace_package(&project.db);
        let pkg = baml_compiler2_hir::package::package_items(&project.db, pkg_id);

        let root_ns: Vec<baml_base::Name> = vec![];
        let item_name = baml_base::Name::new("Point");
        let def = pkg.lookup_type(&root_ns, &item_name).unwrap();

        let files = baml_compiler2_hir::compiler2_all_files(&project.db);
        assert!(super::describe_item_member(&project.db, &files, def, "nonexistent").is_none());
    }

    fn make_project() -> ProjectTest {
        let mut builder = ProjectTest::builder();
        builder.source(
            "types.baml",
            r#"
class Point {
    x int
    y int
}

class Person {
    name string
    age int
}

enum Color {
    Red,
    Green,
    Blue,
}
"#,
        );
        builder.source(
            "funcs.baml",
            r#"
/// Extract a point from text.
function extract_point(text: string) -> Point {
    let result = Point { x: 0, y: 0 };
    return result;
}

function make_person(n: string, a: int) -> Person {
    return Person { name: n, age: a };
}

function use_color(c: Color) -> string {
    match (c) {
        Red => "red"
        Green => "green"
        Blue => "blue"
    }
}
"#,
        );
        builder.build()
    }

    #[test]
    fn describe_class() {
        let project = make_project();
        let descs = project.describe("Point");
        assert_eq!(descs.len(), 1);
        insta::assert_snapshot!(project.format_description(&descs[0]));
    }

    #[test]
    fn describe_class_with_refs() {
        let project = make_project();
        let descs = project.describe("Person");
        assert_eq!(descs.len(), 1);
        insta::assert_snapshot!(project.format_description(&descs[0]));
    }

    #[test]
    fn describe_enum() {
        let project = make_project();
        let descs = project.describe("Color");
        assert_eq!(descs.len(), 1);
        insta::assert_snapshot!(project.format_description(&descs[0]));
    }

    #[test]
    fn describe_interface() {
        let mut builder = ProjectTest::builder();
        builder.source(
            "interfaces.baml",
            r#"
interface Named {
    name: string
    function label(self) -> string throws never
}

class Person {
    name: string
    implements Named {
        function label(self) -> string {
            return self.name
        }
    }
}
"#,
        );
        let project = builder.build();

        let descs = project.describe("Named");
        assert_eq!(descs.len(), 1);
        insta::assert_snapshot!(project.format_description(&descs[0]));
    }

    /// One fixture exercising the whole interface surface: an associated
    /// type, a field, a required method, a defaulted method (docstring +
    /// body), an in-body implements block, a free implement block, and an
    /// unrelated interface whose impl must NOT be listed.
    fn make_interface_surface_project() -> ProjectTest {
        let mut builder = ProjectTest::builder();
        builder.source(
            "interfaces.baml",
            r#"
/// A thing with a name.
interface Named {
    type Output

    name: string,

    /// The display label.
    function label(self) -> string throws never

    /// Greets by label.
    ///
    /// Meant for demos.
    function greet(self) -> string throws never {
        let base = self.label();
        base
    }
}

interface Other {
    function other_thing(self) -> int throws never
}

class Person {
    name: string,
    implements Named {
        type Output = int
        function label(self) -> string {
            self.name
        }
    }
}

class Robot {
    name: string,
}

implement Named for Robot {
    type Output = string
    function label(self) -> string {
        "robot"
    }
}

implement Other for Robot {
    function other_thing(self) -> int {
        1
    }
}
"#,
        );
        builder.build()
    }

    fn named_interface_def(
        project: &ProjectTest,
    ) -> baml_compiler2_hir::contributions::Definition<'_> {
        let pkg_id = sole_workspace_package(&project.db);
        let pkg = baml_compiler2_hir::package::package_items(&project.db, pkg_id);
        pkg.lookup_type(&[], &baml_base::Name::new("Named"))
            .unwrap()
    }

    #[test]
    fn describe_interface_full_surface() {
        let project = make_interface_surface_project();
        let descs = project.describe("Named");
        assert_eq!(descs.len(), 1);
        insta::assert_snapshot!(project.format_description(&descs[0]));
    }

    #[test]
    fn interface_members_come_in_category_order_with_facets() {
        let project = make_interface_surface_project();
        let desc = project.describe("Named").remove(0);

        let super::SymbolKind::Interface { members, .. } = &desc.kind else {
            panic!("an interface description carries the Interface kind");
        };
        let categories: Vec<_> = members.iter().map(|m| m.category).collect();
        assert_eq!(
            categories,
            [
                super::InterfaceMemberCategory::AssociatedType,
                super::InterfaceMemberCategory::Field,
                super::InterfaceMemberCategory::RequiredMethod,
                super::InterfaceMemberCategory::DefaultMethod,
            ]
        );

        let greet = &members[3];
        assert!(greet.declaration.ends_with("{ ... }"));
        let doc = greet.docstring.as_deref().unwrap();
        assert!(doc.contains("Meant for demos."));
        let body = greet.body.as_deref().unwrap();
        assert!(body.contains("self.label()"));
        assert!(!body.contains("///"), "docstring is its own facet: {body}");

        let label = &members[2];
        assert!(label.body.is_none(), "a required method has no body facet");
    }

    #[test]
    fn interface_implementations_list_only_impls_naming_it() {
        let project = make_interface_surface_project();
        let desc = project.describe("Named").remove(0);

        let super::SymbolKind::Interface {
            implementations, ..
        } = &desc.kind
        else {
            panic!("an interface description carries the Interface kind");
        };
        let rows: Vec<&str> = implementations
            .iter()
            .map(|imp| imp.display.as_str())
            .collect();
        assert_eq!(
            rows,
            ["implement Named for Person", "implement Named for Robot"]
        );

        // The interface's own body is a reconstruction: no default-method
        // bodies leak into it.
        assert!(!desc.full_body.contains("self.label()"));
        assert!(desc.full_body.contains("/// A thing with a name."));
    }

    #[test]
    fn describe_interface_member_required_method() {
        let project = make_interface_surface_project();
        let def = named_interface_def(&project);
        let files = baml_compiler2_hir::compiler2_all_files(&project.db);

        let desc = super::describe_item_member(&project.db, &files, def, "label").unwrap();
        assert_eq!(desc.kind.definition_kind(), crate::DefinitionKind::Method);
        assert_eq!(desc.kind.container().unwrap().name, "Named");
        // A required method HAS no body: the description is docstring +
        // signature, never someone else's lines.
        assert!(desc.full_body.contains("/// The display label."));
        assert!(
            desc.full_body
                .contains("function label(self) -> string throws never")
        );
        assert!(!desc.full_body.contains('{'));
    }

    #[test]
    fn describe_interface_member_default_method() {
        let project = make_interface_surface_project();
        let def = named_interface_def(&project);
        let files = baml_compiler2_hir::compiler2_all_files(&project.db);

        let desc = super::describe_item_member(&project.db, &files, def, "greet").unwrap();
        assert_eq!(desc.kind.definition_kind(), crate::DefinitionKind::Method);
        assert!(desc.full_body.contains("self.label()"));
        assert_eq!(
            desc.docstring.as_deref(),
            Some("Greets by label.\n\nMeant for demos.")
        );
    }

    #[test]
    fn describe_interface_member_field_and_associated_type() {
        let project = make_interface_surface_project();
        let def = named_interface_def(&project);
        let files = baml_compiler2_hir::compiler2_all_files(&project.db);

        let field = super::describe_item_member(&project.db, &files, def, "name").unwrap();
        assert_eq!(field.kind.definition_kind(), crate::DefinitionKind::Field);
        assert_eq!(field.kind.container().unwrap().name, "Named");

        let assoc = super::describe_item_member(&project.db, &files, def, "Output").unwrap();
        assert_eq!(
            assoc.kind.definition_kind(),
            crate::DefinitionKind::AssociatedType
        );
        assert_eq!(assoc.full_body, "type Output");
        assert_eq!(assoc.shape, "Named.type Output");
    }

    #[test]
    fn describe_function() {
        let project = make_project();
        let descs = project.describe("extract_point");
        assert_eq!(descs.len(), 1);
        insta::assert_snapshot!(project.format_description(&descs[0]));
    }

    #[test]
    fn describe_function_with_enum_param() {
        let project = make_project();
        let descs = project.describe("use_color");
        assert_eq!(descs.len(), 1);
        insta::assert_snapshot!(project.format_description(&descs[0]));
    }

    #[test]
    fn describe_nonexistent() {
        let project = make_project();
        assert!(project.describe("DoesNotExist").is_empty());
    }

    #[test]
    fn describe_builtin_string_with_compiler2_visible_files() {
        let project = make_project();
        let descs = project.describe_compiler2_visible("String");

        assert_eq!(descs.len(), 1);
        assert_eq!(descs[0].name, "String");
        insta::assert_snapshot!(project.format_description(&descs[0]));
    }

    #[test]
    fn describe_builtin_deep_copy_with_compiler2_visible_files() {
        let project = make_project();
        let descs = project.describe_compiler2_visible("deep_copy");

        assert_eq!(descs.len(), 1);
        assert_eq!(descs[0].name, "deep_copy");
        insta::assert_snapshot!(project.format_description(&descs[0]));
    }

    #[test]
    fn user_only_describe_still_does_not_search_builtins() {
        let project = make_project();
        assert!(project.describe("String").is_empty());
    }

    #[test]
    fn describe_is_case_sensitive() {
        let project = make_project();
        assert!(project.describe("point").is_empty());
    }

    #[test]
    fn describe_lambda_local_binding_uses_lambda_body() {
        let mut builder = ProjectTest::builder();
        builder.source(
            "lambda.baml",
            r#"
function lambda_local_describe() -> string {
    let f = () -> string {
        let ignored = 1
        let target = "lambda"
        target
    }
    f()
}
"#,
        );
        let project = builder.build();

        let descs = project.describe("target");

        assert_eq!(descs.len(), 1);
        assert_eq!(descs[0].shape, "let target: string");
        assert_eq!(descs[0].resolved_type.as_deref(), Some("string"));
    }
}

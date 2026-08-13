//! `type_at` — structured type/signature info at a cursor position.
//!
//! This is a regular function (not a Salsa query). It uses the Rowan CST to
//! find the token under the cursor, resolves the name via `resolve_name_at`,
//! and builds a `TypeInfo` value describing what the name refers to.
//!
//! ## Resolution cases
//!
//! - `ResolvedName::Item(Definition::Function(_))` — builds `TypeInfo::Function`
//!   with params and return type from `function_signature`.
//!
//! - `ResolvedName::Item(Definition::Class(_))` — builds `TypeInfo::Class`
//!   with field names and types from `resolve_class_fields`.
//!
//! - `ResolvedName::Item(Definition::Enum(_))` — builds `TypeInfo::Enum`
//!   with variant names from `enum_data`.
//!
//! - `ResolvedName::Item(Definition::TypeAlias(_))` — builds `TypeInfo::TypeAlias`
//!   with the expansion type from `resolve_type_alias`.
//!
//! - `ResolvedName::Item(Definition::TemplateString(_))` — builds
//!   `TypeInfo::TemplateString` (no further info available).
//!
//! - `ResolvedName::Item(Definition::Client(_) | Generator(_) | ...)` — builds
//!   `TypeInfo::OtherItem` with the kind label.
//!
//! - `ResolvedName::Local { definition_site: Some(Parameter(idx)) }` — builds
//!   `TypeInfo::LocalVar` with the parameter type from `function_signature`.
//!
//! - `ResolvedName::Local { definition_site: Some(Statement(stmt_id)) }` — builds
//!   `TypeInfo::LocalVar` with the binding type from `infer_scope_types`.
//!
//! - `ResolvedName::Builtin(def)` — same as the matching `Item` case above.
//!
//! - `ResolvedName::Unknown` or cursor not on a WORD token — returns `None`.

use baml_base::{Name, SourceFile};
use baml_compiler_syntax::{SyntaxKind, SyntaxToken};
use baml_compiler2_hir::{
    contributions::Definition, scope::ScopeKind, semantic_index::DefinitionSite,
};
use baml_type::BuiltinTypeName;
use text_size::{TextRange, TextSize};

use crate::{Db, utils};

// ── TypeInfo ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionParamInfo {
    pub name: String,
    pub ty: String,
    pub optional: bool,
}

impl FunctionParamInfo {
    pub fn render(&self) -> String {
        let optional = if self.optional { "?" } else { "" };
        format!("{}{}: {}", self.name, optional, self.ty)
    }
}

/// A class method, captured for the hover "has methods?" check and the
/// describe/test renderers. The full describe listing (with docstring and line
/// range) is carried separately by `describe::MethodRef`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MethodSig {
    pub name: String,
    /// Canonical one-line signature, e.g. `function Greet(self) -> string`.
    pub signature: String,
    /// `true` when the first parameter is named `self`.
    pub is_instance: bool,
}

/// Structured type/signature info at a cursor position.
///
/// Returned by `type_at`. The LSP layer (`request.rs`) formats this into hover
/// markdown. Keeping it as a structured type makes it easy to format for
/// different output contexts (markdown, plain text, etc.).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeInfo {
    /// A function definition: name, parameters, return type.
    Function {
        name: String,
        params: Vec<FunctionParamInfo>,
        return_type: Option<String>,
        throws: Option<String>,
        note: Option<String>,
    },
    /// A class definition: name, fields (name + type string), implemented interfaces.
    Class {
        name: String,
        /// Generic type parameter names (e.g. `["T"]`). Rendered as `<T>` after
        /// the class name in the body block; empty for non-generic classes.
        generic_params: Vec<String>,
        fields: Vec<(String, String)>,
        implements: Vec<String>,
        /// Instance + static methods (signatures only). Drives the hover hint
        /// and feeds the test/describe renderers; not shown inline in hover.
        methods: Vec<MethodSig>,
        /// The class's full `///` docstring (all lines), if any.
        docstring: Option<String>,
        /// Canonical FQN for the hover "Run `baml describe …`" hint (`string`,
        /// `Foo`, `root.ns.Foo`, `baml.json.JsonObject`).
        canonical_fqn: String,
    },
    /// An enum definition: name, variants.
    Enum { name: String, variants: Vec<String> },
    /// A type alias: name + the expansion type string.
    TypeAlias { name: String, expansion: String },
    /// A template string: name only (no further type info).
    TemplateString { name: String },
    /// A local variable (let binding or parameter): name + inferred/declared type.
    LocalVar { name: String, ty: String },
    /// A declaration or resolved expression already rendered as canonical BAML.
    Symbol { declaration: String },
    /// Concise language documentation for a primitive, literal, or keyword.
    Documentation { label: String, detail: String },
    /// A non-structural top-level item (client, generator, test, `retry_policy`).
    OtherItem { name: String, kind: &'static str },
}

impl TypeInfo {
    /// The canonical BAML block for this item, without code fences, docstring,
    /// or any trailing hint/note. For a class this is the **fields-only** body
    /// (`class Foo {\n    bar: int,\n}`): methods are surfaced separately by
    /// `describe`, never inside the body block. Shared by
    /// [`Self::to_hover_markdown`] (which wraps it) and `describe::build_shape`.
    pub fn to_describe_block(&self) -> String {
        match self {
            TypeInfo::Function {
                name,
                params,
                return_type,
                throws,
                ..
            } => {
                let param_strs: Vec<String> =
                    params.iter().map(FunctionParamInfo::render).collect();
                let ret = return_type
                    .as_deref()
                    .map(|r| format!(" -> {r}"))
                    .unwrap_or_default();
                let throws = throws
                    .as_deref()
                    .map(|t| format!(" throws {t}"))
                    .unwrap_or_default();
                format!(
                    "function {}({}){}{throws}",
                    name,
                    param_strs.join(", "),
                    ret
                )
            }
            TypeInfo::Class {
                name,
                generic_params,
                fields,
                implements,
                ..
            } => {
                // Fields-only canonical body: `name: type,` with trailing comma,
                // 4-space indent. Methods are never rendered here.
                let generics = if generic_params.is_empty() {
                    String::new()
                } else {
                    format!("<{}>", generic_params.join(", "))
                };
                let mut member_strs: Vec<String> = fields
                    .iter()
                    .map(|(n, t)| format!("    {n}: {t},"))
                    .collect();
                for implements_block in implements {
                    member_strs.extend(implements_block.lines().map(|line| format!("    {line}")));
                }

                if member_strs.is_empty() {
                    format!("class {name}{generics} {{}}")
                } else {
                    format!("class {name}{generics} {{\n{}\n}}", member_strs.join("\n"))
                }
            }
            TypeInfo::Enum { name, variants } => {
                let variant_strs: Vec<String> =
                    variants.iter().map(|v| format!("    {v}")).collect();
                if variant_strs.is_empty() {
                    format!("enum {name} {{}}")
                } else {
                    format!("enum {name} {{\n{}\n}}", variant_strs.join("\n"))
                }
            }
            TypeInfo::TypeAlias { name, expansion } => format!("type {name} = {expansion}"),
            TypeInfo::TemplateString { name } => format!("template_string {name}"),
            TypeInfo::LocalVar { name, ty } => format!("{name}: {ty}"),
            TypeInfo::Symbol { declaration } => declaration.clone(),
            TypeInfo::Documentation { label, .. } => label.clone(),
            TypeInfo::OtherItem { name, kind } => format!("{kind} {name}"),
        }
    }

    /// Format this `TypeInfo` as hover markdown.
    ///
    /// The caller (request.rs) wraps the result in an LSP `MarkupContent`.
    pub fn to_hover_markdown(&self) -> String {
        match self {
            TypeInfo::Class {
                docstring,
                methods,
                canonical_fqn,
                ..
            } => {
                // Docstring lines live inside the fenced block, above the class.
                let mut inner = String::new();
                if let Some(doc) = docstring {
                    for line in doc.lines() {
                        inner.push_str("/// ");
                        inner.push_str(line);
                        inner.push('\n');
                    }
                }
                inner.push_str(&self.to_describe_block());
                let mut out = format!("```baml\n{inner}\n```");
                // Only a class with methods points the user at `baml describe`.
                if !methods.is_empty() {
                    out.push_str("\n\nRun `baml describe ");
                    out.push_str(canonical_fqn);
                    out.push_str("` for methods and details.");
                }
                out
            }
            TypeInfo::Function { note, .. } => {
                let mut out = format!("```baml\n{}\n```", self.to_describe_block());
                if let Some(note) = note {
                    out.push_str("\n\n");
                    out.push_str(note);
                }
                out
            }
            TypeInfo::Documentation { detail, .. } => {
                format!("```baml\n{}\n```\n\n{detail}", self.to_describe_block())
            }
            _ => format!("```baml\n{}\n```", self.to_describe_block()),
        }
    }
}

// ── type_at ───────────────────────────────────────────────────────────────────

/// Find structured type/signature info for the symbol at `offset` in `file`.
///
/// Regular function (not cached). The expensive work (`file_semantic_index`,
/// `function_signature`, `resolve_class_fields`, `infer_scope_types`) is
/// internally Salsa-cached.
///
/// Returns `None` if the cursor is not on an identifier, or if the name
/// cannot be resolved.
pub fn type_at(db: &dyn Db, file: SourceFile, offset: TextSize) -> Option<TypeInfo> {
    // ── Step 1: find the token at the cursor ─────────────────────────────────
    let token = utils::find_token_at_offset(db, file, offset)?;

    if let Some(info) = literal_type_info(&token) {
        return Some(info);
    }

    if let Some(info) = builtin_type_info(db, &token) {
        return Some(info);
    }

    if let Some(info) = client_config_key_type_info(&token) {
        return Some(info);
    }

    if let Some(info) = attribute_type_info(&token) {
        return Some(info);
    }

    if token.kind().is_keyword() {
        return keyword_type_info(token.text());
    }

    // Only WORD tokens can be names.
    if token.kind() != SyntaxKind::WORD {
        return None;
    }

    let name_text = token.text();
    let name = Name::new(name_text);

    if let Some(info) = generic_type_parameter_info_at(db, file, offset, &name) {
        return Some(info);
    }

    // Declaration tokens for fields, methods, enum variants, and interface
    // signatures are not package-level names. Resolve them through their item
    // source maps before attempting ordinary scope resolution.
    if let Some(info) = declaration_type_info_at(db, file, offset, &name) {
        return Some(info);
    }

    // Function parameters need a direct signature fallback. In particular,
    // declarative LLM functions have synthesized expression bodies whose scope
    // range does not always map back through the ordinary local-binding path.
    if let Some(info) = function_parameter_type_info_at(db, file, offset, &name) {
        return Some(info);
    }

    // Fields, methods, enum variants, and qualified builtin functions are
    // resolved by expression inference rather than bare-name lookup. Use the
    // inferred per-segment type so generic receiver arguments are realized.
    if let Some(info) = member_type_info_at(db, file, offset, name_text) {
        return Some(info);
    }

    // A let/for binding is not visible to expression resolution until after its
    // declaration statement. Hovers on the declaration token itself still need
    // to describe that binding.
    if let Some((site, lookup_offset)) = declaration_site_at(db, file, offset, &name) {
        return local_type_info(db, file, lookup_offset, &name, site);
    }

    // ── Step 2: resolve the name in scope ─────────────────────────────────────
    let resolved = baml_compiler2_ppir::resolve::resolve_name_at(db, file, offset, &name);

    // ── Step 3: build TypeInfo based on the resolution ────────────────────────
    match resolved {
        baml_compiler2_ppir::resolve::ResolvedName::Item(def)
        | baml_compiler2_ppir::resolve::ResolvedName::Builtin(def) => {
            Some(type_info_for_definition(db, def))
        }

        baml_compiler2_ppir::resolve::ResolvedName::Local {
            name: local_name,
            definition_site: Some(site),
        } => local_type_info(db, file, offset, &local_name, site),

        baml_compiler2_ppir::resolve::ResolvedName::Local {
            definition_site: None,
            ..
        }
        | baml_compiler2_ppir::resolve::ResolvedName::Unknown => None,
    }
}

fn client_config_key_type_info(token: &SyntaxToken) -> Option<TypeInfo> {
    let config_item = token.parent()?;
    if config_item.kind() != SyntaxKind::CONFIG_ITEM
        || !config_item
            .ancestors()
            .any(|ancestor| ancestor.kind() == SyntaxKind::CLIENT_DEF)
    {
        return None;
    }

    let key = config_item
        .children_with_tokens()
        .filter_map(rowan::NodeOrToken::into_token)
        .find(|candidate| {
            matches!(
                candidate.kind(),
                SyntaxKind::WORD | SyntaxKind::KW_RETRY_POLICY
            )
        })?;
    if key.text_range() != token.text_range() {
        return None;
    }

    let spec = baml_base::client_config_key_spec(token.text());
    let label = spec.map_or(token.text(), |spec| spec.signature);
    let detail = spec
        .and_then(|spec| baml_builtins2::language_topic(spec.name))
        .or_else(|| baml_builtins2::language_topic("client_option"))
        .map(|topic| topic.summary.as_str())?;

    Some(TypeInfo::Documentation {
        label: label.to_string(),
        detail: detail.to_string(),
    })
}

fn attribute_type_info(token: &SyntaxToken) -> Option<TypeInfo> {
    let attribute = token.parent()?;
    let prefix = match attribute.kind() {
        SyntaxKind::ATTRIBUTE => "@",
        SyntaxKind::BLOCK_ATTRIBUTE => "@@",
        _ => return None,
    };

    let name = attribute
        .children_with_tokens()
        .filter_map(rowan::NodeOrToken::into_token)
        .find(|candidate| candidate.kind() == SyntaxKind::WORD)?;
    if name.text_range() != token.text_range() {
        return None;
    }

    let spec = baml_base::schema_attribute_spec(token.text())?;
    let detail = baml_builtins2::language_topic(spec.name)?.summary.clone();

    Some(TypeInfo::Documentation {
        label: format!("{prefix}{}", spec.signature()),
        detail,
    })
}

fn literal_type_info(token: &SyntaxToken) -> Option<TypeInfo> {
    let (label, ty, detail) = match (token.kind(), token.text()) {
        (_, "true" | "false") => (token.text().to_string(), "bool", "A boolean literal type."),
        (_, "null") => (token.text().to_string(), "null", "The `null` literal type."),
        (SyntaxKind::INTEGER_LITERAL, _) => (
            token.text().to_string(),
            "int",
            "An exact `int` literal type.",
        ),
        (SyntaxKind::BIGINT_LITERAL, _) => (
            token.text().to_string(),
            "bigint",
            "An exact arbitrary-precision `bigint` literal type.",
        ),
        (SyntaxKind::FLOAT_LITERAL, _) => (
            token.text().to_string(),
            "float",
            "An exact `float` literal type.",
        ),
        _ => {
            if token
                .parent_ancestors()
                .any(|node| matches!(node.kind(), SyntaxKind::BACKTICK_INTERPOLATION))
            {
                return None;
            }
            let string = token.parent_ancestors().find(|node| {
                matches!(
                    node.kind(),
                    SyntaxKind::STRING_LITERAL
                        | SyntaxKind::RAW_STRING_LITERAL
                        | SyntaxKind::BYTE_STRING_LITERAL
                )
            })?;
            let label = string.text().to_string();
            let (ty, detail) = if string.kind() == SyntaxKind::BYTE_STRING_LITERAL {
                ("uint8array", "A byte-string literal type.")
            } else {
                ("string", "An exact `string` literal type.")
            };
            return Some(TypeInfo::Documentation {
                label,
                detail: format!("{detail} Its base type is `{ty}`."),
            });
        }
    };

    Some(TypeInfo::Documentation {
        label,
        detail: format!("{detail} Its base type is `{ty}`."),
    })
}

fn builtin_type_info(db: &dyn Db, token: &SyntaxToken) -> Option<TypeInfo> {
    let builtin = BuiltinTypeName::from_alias(token.text())?;
    let detail = match builtin {
        BuiltinTypeName::Primitive(_) => {
            let crate::ResolvedTarget::Item(Definition::Class(class_loc)) =
                crate::resolve_builtin_type_target(db, builtin.alias())?
            else {
                return None;
            };
            let data = baml_compiler2_ppir::item_data::class_data(db, class_loc);
            first_docstring_paragraph(data.docstring.as_deref()?)?
        }
        BuiltinTypeName::Void | BuiltinTypeName::Never | BuiltinTypeName::Unknown => {
            baml_builtins2::language_topic(builtin.alias())?
                .summary
                .clone()
        }
        // `json` is a real type alias and follows ordinary semantic name
        // resolution below rather than being intercepted as an intrinsic.
        BuiltinTypeName::Json => return None,
    };
    Some(TypeInfo::Documentation {
        label: builtin.alias().to_string(),
        detail,
    })
}

fn keyword_type_info(keyword: &str) -> Option<TypeInfo> {
    let detail = baml_builtins2::language_topic(keyword)
        .map(|topic| topic.summary.clone())
        .or_else(|| {
            baml_builtins2::typescript_crosswalk_topic(keyword).map(|topic| topic.message.clone())
        })?;

    Some(TypeInfo::Documentation {
        label: keyword.to_string(),
        detail,
    })
}

fn first_docstring_paragraph(docstring: &str) -> Option<String> {
    let paragraph = docstring.split("\n\n").next()?.trim();
    (!paragraph.is_empty()).then(|| {
        paragraph
            .lines()
            .map(str::trim)
            .collect::<Vec<_>>()
            .join(" ")
    })
}

fn range_contains(range: TextRange, offset: TextSize) -> bool {
    range.contains(offset) || range.end() == offset
}

fn generic_type_parameter_info_at(
    db: &dyn Db,
    file: SourceFile,
    offset: TextSize,
    name: &Name,
) -> Option<TypeInfo> {
    use baml_compiler2_ppir::item_data;

    let mut candidates: Vec<(TextSize, String, Option<String>)> = Vec::new();

    for func_loc in item_data::file_functions(db, file) {
        let source_map = item_data::function_source_map(db, *func_loc);
        if !range_contains(source_map.span, offset) {
            continue;
        }
        let data = item_data::function_data(db, *func_loc);
        if let Some(declared) = data.generic_params.iter().find(|param| &param.name == name) {
            let origin = item_data::file_classes(db, file)
                .iter()
                .find_map(|class_loc| {
                    let class = item_data::class_data(db, *class_loc);
                    class
                        .methods
                        .contains(func_loc)
                        .then(|| format!("method {}.{}", class.name.as_str(), data.name.as_str()))
                })
                .or_else(|| {
                    item_data::file_interfaces(db, file)
                        .iter()
                        .find_map(|iface_loc| {
                            let iface = item_data::interface_data(db, *iface_loc);
                            iface.default_methods.contains(func_loc).then(|| {
                                format!("method {}.{}", iface.name.as_str(), data.name.as_str())
                            })
                        })
                })
                .unwrap_or_else(|| format!("function {}", data.name.as_str()));
            let bound = render_generic_bounds(declared, &data.type_refs);
            candidates.push((source_map.span.len(), origin, bound));
        }
    }

    for class_loc in item_data::file_classes(db, file) {
        let source_map = item_data::class_source_map(db, *class_loc);
        if !range_contains(source_map.span, offset) {
            continue;
        }
        let data = item_data::class_data(db, *class_loc);
        if let Some(declared) = data.generic_params.iter().find(|param| &param.name == name) {
            candidates.push((
                source_map.span.len(),
                format!("class {}", data.name.as_str()),
                render_generic_bounds(declared, &data.type_refs),
            ));
        }
    }

    for iface_loc in item_data::file_interfaces(db, file) {
        let source_map = item_data::interface_source_map(db, *iface_loc);
        if !range_contains(source_map.span, offset) {
            continue;
        }
        let data = item_data::interface_data(db, *iface_loc);
        for (method_idx, method) in data.required_methods.iter().enumerate() {
            let Some(method_source_map) = source_map.required_method_spans.get(method_idx) else {
                continue;
            };
            if !range_contains(method_source_map.span, offset) {
                continue;
            }
            if let Some(declared) = method
                .generic_params
                .iter()
                .find(|param| &param.name == name)
            {
                candidates.push((
                    method_source_map.span.len(),
                    format!("method {}.{}", data.name.as_str(), method.name.as_str()),
                    render_generic_bounds(declared, &data.type_refs),
                ));
            }
        }
        if let Some(declared) = data.generic_params.iter().find(|param| &param.name == name) {
            candidates.push((
                source_map.span.len(),
                format!("interface {}", data.name.as_str()),
                render_generic_bounds(declared, &data.type_refs),
            ));
        }
    }

    candidates.sort_by_key(|(span_len, _, _)| *span_len);
    let (_, origin, bound) = candidates.into_iter().next()?;
    let bound = bound
        .map(|bound| format!(" extends {bound}"))
        .unwrap_or_default();
    Some(TypeInfo::Documentation {
        label: format!("type parameter {}{bound}", name.as_str()),
        detail: format!("Declared by `{origin}`."),
    })
}

/// A parameter's declared bounds rendered as source (`A & B`), or `None` when it
/// is unbounded.
fn render_generic_bounds(
    param: &baml_compiler2_ppir::item_data::GenericParamData,
    store: &baml_compiler2_hir::type_ref::TypeRefStore,
) -> Option<String> {
    if param.bounds.is_empty() {
        return None;
    }
    Some(
        param
            .bounds
            .iter()
            .map(|&id| store.display(id).to_string())
            .collect::<Vec<_>>()
            .join(" & "),
    )
}

fn render_generic_params(
    params: &[baml_compiler2_ppir::item_data::GenericParamData],
    store: &baml_compiler2_hir::type_ref::TypeRefStore,
) -> Vec<String> {
    params
        .iter()
        .map(|param| match render_generic_bounds(param, store) {
            Some(bounds) => format!("{} extends {bounds}", param.name.as_str()),
            None => param.name.as_str().to_string(),
        })
        .collect()
}

fn declaration_type_info_at(
    db: &dyn Db,
    file: SourceFile,
    offset: TextSize,
    name: &Name,
) -> Option<TypeInfo> {
    use baml_compiler2_ppir::item_data;

    let index = baml_compiler2_hir::file_semantic_index(db, file);
    let scope_id = index.scope_at_offset(offset, None);
    let scope = &index.scopes[scope_id.index() as usize];
    let preferred_class = if matches!(scope.kind, ScopeKind::Class) {
        scope.name.as_ref().and_then(|class_name| {
            match baml_compiler2_ppir::resolve::resolve_name_at(db, file, offset, class_name) {
                baml_compiler2_ppir::resolve::ResolvedName::Item(Definition::Class(loc))
                | baml_compiler2_ppir::resolve::ResolvedName::Builtin(Definition::Class(loc)) => {
                    Some(loc)
                }
                _ => None,
            }
        })
    } else {
        None
    };
    let class_locs = preferred_class
        .into_iter()
        .chain(
            item_data::file_classes(db, file)
                .iter()
                .copied()
                .filter(|loc| Some(*loc) != preferred_class),
        )
        .collect::<Vec<_>>();

    for class_loc in class_locs {
        let class = item_data::class_data(db, class_loc);
        let source_map = item_data::class_source_map(db, class_loc);
        for (field_idx, field) in class.fields.iter().enumerate() {
            let Some(name_span) = source_map.field_name_spans.get(field_idx) else {
                continue;
            };
            if field.name == *name && range_contains(*name_span, offset) {
                let ty = utils::display_type_ref(&class.type_refs, field.type_ref);
                return Some(TypeInfo::LocalVar {
                    name: name.as_str().to_string(),
                    ty,
                });
            }
        }
    }

    for enum_loc in item_data::file_enums(db, file) {
        let enum_data = item_data::enum_data(db, *enum_loc);
        let source_map = item_data::enum_source_map(db, *enum_loc);
        for (variant_idx, variant) in enum_data.variants.iter().enumerate() {
            let Some(name_span) = source_map.variant_name_spans.get(variant_idx) else {
                continue;
            };
            if variant.name == *name && range_contains(*name_span, offset) {
                return Some(TypeInfo::Symbol {
                    declaration: format!(
                        "{}.{}: {}",
                        enum_data.name.as_str(),
                        variant.name.as_str(),
                        enum_data.name.as_str()
                    ),
                });
            }
        }
    }

    for iface_loc in item_data::file_interfaces(db, file) {
        let iface = item_data::interface_data(db, *iface_loc);
        let source_map = item_data::interface_source_map(db, *iface_loc);

        for (field_idx, field) in iface.fields.iter().enumerate() {
            let Some(name_span) = source_map.field_name_spans.get(field_idx) else {
                continue;
            };
            if field.name == *name && range_contains(*name_span, offset) {
                let ty = utils::display_type_ref(&iface.type_refs, field.type_ref);
                return Some(TypeInfo::LocalVar {
                    name: name.as_str().to_string(),
                    ty,
                });
            }
        }

        for (method_idx, method) in iface.required_methods.iter().enumerate() {
            let Some(method_spans) = source_map.required_method_spans.get(method_idx) else {
                continue;
            };
            if method.name == *name && range_contains(method_spans.name_span, offset) {
                return Some(TypeInfo::Symbol {
                    declaration: render_interface_method_signature(iface, method),
                });
            }

            for (param_idx, param) in method.params.iter().enumerate() {
                let Some(param_span) = method_spans.param_spans.get(param_idx) else {
                    continue;
                };
                if param.name == *name && range_contains(*param_span, offset) {
                    let ty = param
                        .type_ref
                        .map(|id| utils::display_type_ref(&iface.type_refs, id))
                        .unwrap_or_else(|| {
                            if param.name.as_str() == "self" {
                                "Self".to_string()
                            } else {
                                "unknown".to_string()
                            }
                        });
                    return Some(TypeInfo::LocalVar {
                        name: name.as_str().to_string(),
                        ty,
                    });
                }
            }
        }
    }

    // Class/default-interface method declarations are ordinary FunctionLocs,
    // but they are intentionally absent from package contributions.
    for func_loc in item_data::file_functions(db, file) {
        let source_map = item_data::function_source_map(db, *func_loc);
        if range_contains(source_map.name_span, offset)
            && item_data::function_data(db, *func_loc).name == *name
        {
            if matches!(
                baml_compiler2_ppir::resolve::resolve_name_at(db, file, offset, name),
                baml_compiler2_ppir::resolve::ResolvedName::Item(Definition::Function(_))
                    | baml_compiler2_ppir::resolve::ResolvedName::Builtin(Definition::Function(_))
            ) {
                continue;
            }
            return Some(TypeInfo::Symbol {
                declaration: render_function_data_signature(item_data::function_data(
                    db, *func_loc,
                )),
            });
        }
    }

    None
}

fn render_function_data_signature(
    function: &baml_compiler2_ppir::item_data::FunctionData,
) -> String {
    let generic_params = render_generic_params(&function.generic_params, &function.type_refs);
    let generics = if generic_params.is_empty() {
        String::new()
    } else {
        format!("<{}>", generic_params.join(", "))
    };
    let params = function
        .params
        .iter()
        .map(|param| {
            let optional = if param.has_default { "?" } else { "" };
            match param.type_ref {
                Some(id) => format!(
                    "{}{}: {}",
                    param.name.as_str(),
                    optional,
                    function.type_refs.display(id)
                ),
                None => param.name.as_str().to_string(),
            }
        })
        .collect::<Vec<_>>()
        .join(", ");
    let ret = function
        .return_type
        .map(|id| format!(" -> {}", function.type_refs.display(id)))
        .unwrap_or_default();
    let throws = function
        .throws
        .map(|id| format!(" throws {}", function.type_refs.display(id)))
        .unwrap_or_else(|| " throws never".to_string());
    format!(
        "function {}{generics}({params}){ret}{throws}",
        function.name.as_str()
    )
}

fn render_interface_method_signature(
    iface: &baml_compiler2_ppir::item_data::InterfaceData<'_>,
    method: &baml_compiler2_ppir::item_data::InterfaceMethodSigData,
) -> String {
    let generic_params = render_generic_params(&method.generic_params, &iface.type_refs);
    let generics = if generic_params.is_empty() {
        String::new()
    } else {
        format!("<{}>", generic_params.join(", "))
    };
    let params = method
        .params
        .iter()
        .map(|param| {
            let optional = if param.has_default { "?" } else { "" };
            match param.type_ref {
                Some(id) => format!(
                    "{}{}: {}",
                    param.name.as_str(),
                    optional,
                    iface.type_refs.display(id)
                ),
                None => param.name.as_str().to_string(),
            }
        })
        .collect::<Vec<_>>()
        .join(", ");
    let ret = method
        .return_type
        .map(|id| format!(" -> {}", iface.type_refs.display(id)))
        .unwrap_or_default();
    let throws = method
        .throws
        .map(|id| format!(" throws {}", iface.type_refs.display(id)))
        .unwrap_or_default();
    format!(
        "function {}{generics}({params}){ret}{throws}",
        method.name.as_str()
    )
}

fn function_parameter_type_info_at(
    db: &dyn Db,
    file: SourceFile,
    offset: TextSize,
    name: &Name,
) -> Option<TypeInfo> {
    use baml_compiler2_ast::ast::FunctionOrigin;
    use baml_compiler2_ppir::item_data;

    let mut candidates = item_data::file_functions(db, file)
        .iter()
        .copied()
        .filter(|&loc| {
            let source_map = item_data::function_source_map(db, loc);
            let data = item_data::function_data(db, loc);
            let matching_param = data
                .params
                .iter()
                .enumerate()
                .find(|(_, param)| param.name == *name);
            let Some((param_idx, _)) = matching_param else {
                return false;
            };
            let on_declaration = source_map
                .param_spans
                .get(param_idx)
                .is_some_and(|span| range_contains(*span, offset));
            let in_llm_body = item_data::function_llm_meta(db, loc).is_some()
                && range_contains(source_map.span, offset);
            on_declaration || in_llm_body
        })
        .collect::<Vec<_>>();
    candidates.sort_by_key(|&loc| {
        let data = item_data::function_data(db, loc);
        let source_map = item_data::function_source_map(db, loc);
        (
            source_map.span.len(),
            match data.metadata.origin {
                FunctionOrigin::UserDefined => 0u8,
                FunctionOrigin::Companion => 1,
                FunctionOrigin::Internal => 2,
                FunctionOrigin::AutoDerive => 3,
            },
        )
    });

    let func_loc = candidates.into_iter().next()?;
    let data = item_data::function_data(db, func_loc);
    let param = data.params.iter().find(|param| param.name == *name)?;
    Some(TypeInfo::LocalVar {
        name: name.as_str().to_string(),
        ty: param
            .type_ref
            .map(|id| data.type_refs.display(id).to_string())
            .unwrap_or_else(|| {
                if param.name.as_str() == "self" {
                    "Self".to_string()
                } else {
                    "unknown".to_string()
                }
            }),
    })
}

fn member_type_info_at(
    db: &dyn Db,
    file: SourceFile,
    offset: TextSize,
    token_text: &str,
) -> Option<TypeInfo> {
    use baml_compiler2_ast::Expr;
    use baml_compiler2_hir::body::FunctionBody;
    use baml_compiler2_hir_ty::infer::MemberResolution;

    let index = baml_compiler2_hir::file_semantic_index(db, file);
    let scope_id = index.scope_at_offset(offset, None);
    let enclosing_func_scope = index
        .ancestor_scopes(scope_id)
        .into_iter()
        .find(|ancestor_id| {
            matches!(
                index.scopes[ancestor_id.index() as usize].kind,
                ScopeKind::Function
            )
        })?;
    let func_scope_range = index.scopes[enclosing_func_scope.index() as usize].range;
    let func_loc = utils::function_at_scope_range(db, file, func_scope_range)?;
    let function_body = baml_compiler2_hir::body::function_body(db, func_loc);
    let FunctionBody::Expr(expr_body) = function_body.as_ref() else {
        return None;
    };
    let source_map = baml_compiler2_hir::body::function_body_source_map(db, func_loc)?;
    let inference = baml_compiler2_hir_ty::ide::infer_for_scope(
        db,
        index.scope_ids[enclosing_func_scope.index() as usize],
    )?;
    let bare_name_is_local = matches!(
        baml_compiler2_ppir::resolve::resolve_name_at(db, file, offset, &Name::new(token_text),),
        baml_compiler2_ppir::resolve::ResolvedName::Local { .. }
    );

    let mut best: Option<(baml_compiler2_ast::ExprId, TextRange, Option<usize>)> = None;
    for (expr_id, expr) in expr_body.exprs.iter() {
        match expr {
            Expr::MemberAccess { member, .. } if member.as_str() == token_text => {
                let span = source_map.expr_span(expr_id);
                if range_contains(span, offset)
                    && best.is_none_or(|(_, previous, _)| span.len() < previous.len())
                {
                    best = Some((expr_id, span, None));
                }
            }
            Expr::Path(segments) if segments.len() >= 2 => {
                let segment_idx = segments[1..]
                    .iter()
                    .enumerate()
                    .find(|(idx, segment)| {
                        segment.as_str() == token_text
                            && range_contains(
                                source_map.path_segment_span(expr_id, *idx + 1),
                                offset,
                            )
                    })
                    .map(|(idx, _)| idx + 1);
                if let Some(segment_idx) = segment_idx {
                    let span = source_map.expr_span(expr_id);
                    if best.is_none_or(|(_, previous, _)| span.len() < previous.len()) {
                        best = Some((expr_id, span, Some(segment_idx)));
                    }
                }
            }
            Expr::Path(segments)
                if bare_name_is_local
                    && segments.len() == 1
                    && segments[0].as_str() == token_text =>
            {
                let span = source_map.expr_span(expr_id);
                if range_contains(span, offset)
                    && best.is_none_or(|(_, previous, _)| span.len() < previous.len())
                {
                    best = Some((expr_id, span, None));
                }
            }
            _ => {}
        }
    }

    let (expr_id, _, path_segment_idx) = best?;
    let resolution = match path_segment_idx {
        Some(segment_idx) => inference
            .path_resolutions
            .get(&expr_id)
            .and_then(|path| path.segments.get(segment_idx))
            .and_then(|step| step.resolution.as_ref())
            .or_else(|| inference.member_resolutions.get(&expr_id)),
        None => inference.member_resolutions.get(&expr_id),
    };

    if let Some(MemberResolution::Free { func: func_loc }) = resolution {
        return Some(type_info_for_definition(
            db,
            Definition::Function(*func_loc),
        ));
    }

    if let Some(MemberResolution::Variant {
        enum_loc,
        variant: variant_name,
    }) = resolution
    {
        let enum_data = baml_compiler2_ppir::item_data::enum_data(db, *enum_loc);
        return Some(TypeInfo::Symbol {
            declaration: format!(
                "{}.{}: {}",
                enum_data.name.as_str(),
                variant_name.as_str(),
                enum_data.name.as_str()
            ),
        });
    }

    let ty = path_segment_idx
        .and_then(|segment_idx| {
            inference
                .path_resolutions
                .get(&expr_id)
                .and_then(|path| path.segments.get(segment_idx))
                .map(|step| step.ty.to_plain())
        })
        .or_else(|| {
            inference
                .type_of_expr
                .get(&expr_id)
                .map(baml_type::interned::Ty::to_plain)
        })?;
    Some(TypeInfo::LocalVar {
        name: token_text.to_string(),
        ty: utils::display_ty_for_file(db, file, &ty),
    })
}

fn declaration_site_at(
    db: &dyn Db,
    file: SourceFile,
    offset: TextSize,
    name: &Name,
) -> Option<(DefinitionSite, TextSize)> {
    let index = baml_compiler2_hir::file_semantic_index(db, file);
    let scope_id = index.scope_at_offset(offset, None);

    for ancestor_id in index.ancestor_scopes(scope_id) {
        let bindings = &index.scope_bindings[ancestor_id.index() as usize];
        for binding in bindings.bindings.iter().rev() {
            if &binding.name == name
                && (binding.name_range.contains(offset) || binding.name_range.end() == offset)
            {
                return Some((binding.site, binding.name_range.end()));
            }
        }
    }

    None
}

// ── type_info_for_definition ──────────────────────────────────────────────────

/// Build `TypeInfo` for a top-level item definition.
pub fn type_info_for_definition(db: &dyn Db, def: Definition<'_>) -> TypeInfo {
    match def {
        Definition::Function(func_loc) => {
            let file = func_loc.file(db);
            let pkg_info = baml_compiler2_hir::file_package::file_package(db, file);
            let pkg_id = baml_compiler2_hir::package::PackageId::new(db, pkg_info.package.clone());
            let iface = baml_compiler2_hir_ty::package_interface::package_interface(db, pkg_id);
            let sig = baml_compiler2_hir::signature::function_signature(db, func_loc);
            let function_name = sig.name.clone();

            let fallback = || {
                let params = sig
                    .params
                    .iter()
                    .map(|param| FunctionParamInfo {
                        name: param.name.as_str().to_string(),
                        ty: utils::display_type_expr(&param.ty),
                        optional: param.has_default,
                    })
                    .collect();
                let return_type = sig.return_type.as_ref().map(utils::display_type_expr);
                TypeInfo::Function {
                    name: sig.name.as_str().to_string(),
                    params,
                    return_type,
                    throws: sig.throws.as_ref().map(utils::display_type_expr),
                    note: None,
                }
            };

            let Some(exported) = iface.lookup_function(&pkg_info.namespace_path, &function_name)
            else {
                return fallback();
            };

            let params = exported
                .params
                .iter()
                .map(|param| FunctionParamInfo {
                    name: param
                        .name
                        .as_ref()
                        .map(|name| name.as_str().to_string())
                        .unwrap_or_else(|| "_".to_string()),
                    ty: display_surface_ty(db, file, &param.ty),
                    optional: param.is_optional(),
                })
                .collect();
            let return_type = Some(display_surface_ty(db, file, &exported.return_type));
            let throws = if exported.declared_throws.is_some()
                || !matches!(exported.callable_throws, baml_type::Ty::Never { .. })
            {
                Some(display_surface_ty(db, file, &exported.callable_throws))
            } else {
                None
            };
            TypeInfo::Function {
                name: exported.name.as_str().to_string(),
                params,
                return_type,
                throws,
                note: callback_forwarding_note(exported),
            }
        }

        Definition::Class(class_loc) => {
            let class_data = baml_compiler2_ppir::item_data::class_data(db, class_loc);
            let class_name = class_data.name.as_str().to_string();

            // Use resolved field types (Salsa-cached), rendered canonically so
            // builtin companion classes collapse to their alias (`string`).
            let resolved = baml_compiler2_hir_ty::lower::resolve_class_fields(db, class_loc);
            let fields = resolved
                .iter()
                .map(|(field_name, ty, _attrs)| {
                    (
                        field_name.as_str().to_string(),
                        utils::display_ty_canonical_for_file(db, class_loc.file(db), ty),
                    )
                })
                .collect();
            // `implements` targets and associated-type bindings are unresolved
            // type references rendered via the arena-backed `TypeRef` renderer
            // (byte-identical to `ast::TypeExpr`'s `Display`).
            let implements = class_data
                .implements
                .iter()
                .map(|block| render_implements_block(block, &class_data.type_refs))
                .collect();

            let qtn = baml_compiler2_hir_ty::lower::qualify_def(db, def, &class_data.name);
            let canonical_fqn = utils::canonical_fqn_string(&qtn);
            let methods = crate::describe::class_method_sigs(db, class_loc);

            let generic_params =
                render_generic_params(&class_data.generic_params, &class_data.type_refs);

            TypeInfo::Class {
                name: class_name,
                generic_params,
                fields,
                implements,
                methods,
                docstring: class_data.docstring.clone(),
                canonical_fqn,
            }
        }

        Definition::Enum(enum_loc) => {
            let enum_data = baml_compiler2_ppir::item_data::enum_data(db, enum_loc);
            let variants = enum_data
                .variants
                .iter()
                .map(|v| v.name.as_str().to_string())
                .collect();
            TypeInfo::Enum {
                name: enum_data.name.as_str().to_string(),
                variants,
            }
        }

        Definition::Interface(iface_loc) => {
            let iface = baml_compiler2_ppir::item_data::interface_data(db, iface_loc);
            let generic_params = render_generic_params(&iface.generic_params, &iface.type_refs);
            let generics = if generic_params.is_empty() {
                String::new()
            } else {
                format!("<{}>", generic_params.join(", "))
            };
            TypeInfo::Symbol {
                declaration: format!("interface {}{generics}", iface.name.as_str()),
            }
        }

        Definition::TypeAlias(alias_loc) => {
            let alias_data = baml_compiler2_ppir::item_data::type_alias_data(db, alias_loc);
            let alias_name = alias_data.name.as_str().to_string();

            // Use the resolved (lowered) type for display.
            let resolved = baml_compiler2_hir_ty::lower::type_alias_value(db, alias_loc).to_plain();
            let expansion = utils::display_ty_for_file(db, alias_loc.file(db), &resolved);

            TypeInfo::TypeAlias {
                name: alias_name,
                expansion,
            }
        }

        Definition::TemplateString(ts_loc) => {
            let ts_data = baml_compiler2_ppir::item_data::template_string_data(db, ts_loc);
            TypeInfo::TemplateString {
                name: ts_data.name.as_str().to_string(),
            }
        }

        Definition::Client(loc) => {
            let data = baml_compiler2_ppir::item_data::client_data(db, loc);
            TypeInfo::OtherItem {
                name: data.name.as_str().to_string(),
                kind: "client",
            }
        }

        Definition::Test(loc) => {
            let data = baml_compiler2_ppir::item_data::test_data(db, loc);
            TypeInfo::OtherItem {
                name: data.name.as_str().to_string(),
                kind: "test",
            }
        }

        Definition::RetryPolicy(loc) => {
            let data = baml_compiler2_ppir::item_data::retry_policy_data(db, loc);
            TypeInfo::OtherItem {
                name: data.name.as_str().to_string(),
                kind: "retry_policy",
            }
        }

        Definition::Let(loc) => {
            let data = baml_compiler2_ppir::item_data::let_data(db, loc);
            let kind = match data.origin {
                baml_compiler2_ast::ast::LetOrigin::Client => "client",
                baml_compiler2_ast::ast::LetOrigin::RetryPolicy => "retry_policy",
                baml_compiler2_ast::ast::LetOrigin::Source => "let",
            };
            TypeInfo::OtherItem {
                name: data.name.as_str().to_string(),
                kind,
            }
        }
    }
}

fn render_implements_block(
    block: &baml_compiler2_ppir::item_data::ImplementsData,
    store: &baml_compiler2_hir::type_ref::TypeRefStore,
) -> String {
    let mut members = Vec::new();

    members.extend(block.field_links.iter().map(|link| {
        format!(
            "{} as {}",
            link.interface_field.as_str(),
            link.class_field.as_str()
        )
    }));
    members.extend(block.associated_type_bindings.iter().map(|binding| {
        let ty = binding
            .type_ref
            .map(|id| store.display(id).to_string())
            .unwrap_or_else(|| "unknown".to_string());
        format!("type {} = {}", binding.name.as_str(), ty)
    }));

    let target = store.display(block.target);
    if members.is_empty() {
        format!("implements {target} {{}}")
    } else {
        let members = members
            .into_iter()
            .map(|member| format!("    {member}"))
            .collect::<Vec<_>>()
            .join("\n");
        format!("implements {target} {{\n{members}\n}}")
    }
}

fn display_surface_ty(db: &dyn Db, file: SourceFile, ty: &baml_type::Ty) -> String {
    utils::display_ty_for_file(db, file, ty)
}

fn display_local_binding_ty(db: &dyn Db, file: SourceFile, ty: &baml_type::Ty) -> String {
    utils::display_ty_for_file(db, file, ty)
}

fn function_param_matches_effect_slot(
    ty: &baml_type::Ty,
    effect_param: &baml_type::ParamTy,
) -> bool {
    use baml_type::Ty;

    match ty {
        Ty::Function { throws, .. } => matches!(
            throws.as_ref(),
            Ty::TypeVar(param, _) if param == effect_param
        ),
        Ty::Union(members, _) => {
            let mut matched = false;
            for member in members {
                if matches!(member, Ty::Null { .. }) {
                    continue;
                }
                if !function_param_matches_effect_slot(member, effect_param) {
                    return false;
                }
                matched = true;
            }
            matched
        }
        _ => false,
    }
}

fn callback_forwarding_note(
    exported: &baml_compiler2_hir_ty::package_interface::ExportedFunction,
) -> Option<String> {
    use baml_type::Ty;

    let throws_facts =
        baml_compiler2_hir_ty::package_interface::flatten_ty_to_facts(&exported.callable_throws);
    let throw_fact_refs = throws_facts.iter().collect::<Vec<_>>();
    let [only_fact] = throw_fact_refs.as_slice() else {
        return None;
    };
    let Ty::TypeVar(effect_name, _) = only_fact else {
        return None;
    };
    if !baml_type::is_synthetic_effect_param(effect_name.name()) {
        return None;
    }

    let mut matching_params = exported
        .params
        .iter()
        .filter(|param| function_param_matches_effect_slot(&param.ty, effect_name))
        .filter_map(|param| param.name.as_ref())
        .collect::<Vec<_>>();

    if matching_params.len() == 1 {
        let callback_name = matching_params.pop().expect("len checked");
        Some(format!(
            "Forwards whatever callback `{callback_name}` throws."
        ))
    } else {
        None
    }
}

// ── local_type_info ───────────────────────────────────────────────────────────

/// Build `TypeInfo::LocalVar` for a local variable (let binding or parameter).
fn local_type_info(
    db: &dyn Db,
    file: SourceFile,
    at_offset: TextSize,
    name: &Name,
    site: DefinitionSite,
) -> Option<TypeInfo> {
    let index = baml_compiler2_hir::file_semantic_index(db, file);

    // Find the enclosing Function scope.
    let scope_id = index.scope_at_offset(at_offset, None);
    let enclosing_func_scope = index
        .ancestor_scopes(scope_id)
        .into_iter()
        .find(|ancestor_id| {
            matches!(
                index.scopes[ancestor_id.index() as usize].kind,
                ScopeKind::Function
            )
        })?;

    let func_scope_range = index.scopes[enclosing_func_scope.index() as usize].range;

    // Match scope range to a function via the firewall enumeration.
    let func_loc = crate::utils::function_at_scope_range(db, file, func_scope_range)?;

    match site {
        DefinitionSite::Parameter(param_idx) => {
            // Render from the arena-backed signature data so generic arguments
            // are preserved (`Lorem<int>`, not the legacy AST display `Lorem`).
            let data = baml_compiler2_ppir::item_data::function_data(db, func_loc);
            let ty_str = data
                .params
                .get(param_idx)
                .and_then(|param| param.type_ref)
                .map(|id| data.type_refs.display(id).to_string())
                .unwrap_or_else(|| "unknown".to_string());
            Some(TypeInfo::LocalVar {
                name: name.as_str().to_string(),
                ty: ty_str,
            })
        }

        DefinitionSite::Statement(stmt_id) => {
            // Look up the binding type from inferred scope types.
            // We need to go from `StmtId` → `PatId` → binding type.
            //
            // Use the function body to find the statement and extract the pat id,
            // then look up the type from infer_scope_types for the enclosing scope.
            let body = baml_compiler2_hir::body::function_body(db, func_loc);
            let pat_id = body_stmt_to_pat_id(&body, stmt_id)?;

            // Get the scope containing at_offset (may be a nested block scope).
            // infer_scope_types is keyed by ScopeId. We need the function scope's
            // ScopeId to get the binding type, since bindings are stored per scope.
            let func_scope_id = index.scope_ids[enclosing_func_scope.index() as usize];
            let inference = baml_compiler2_hir_ty::ide::infer_for_scope(db, func_scope_id)?;
            let ty_str = inference
                .type_of_pat
                .get(&pat_id)
                .map(|ty| display_local_binding_ty(db, file, &ty.to_plain()))
                .unwrap_or_else(|| {
                    // Try the use-site's ancestor scope chain — restricts the
                    // lookup to inferences for bodies that share the
                    // use-site's pattern arena. Iterating *every* scope in
                    // the file would, under PatId collisions across nested
                    // ExprBodies (e.g. two lambdas with the same arena
                    // index), surface the wrong type for hover/inlay hints.
                    find_binding_ty_in_scopes(db, index, scope_id, pat_id)
                        .map(|ty| display_local_binding_ty(db, file, &ty))
                        .unwrap_or_else(|| "unknown".to_string())
                });

            Some(TypeInfo::LocalVar {
                name: name.as_str().to_string(),
                ty: ty_str,
            })
        }

        DefinitionSite::PatternBinding(pat_id) | DefinitionSite::CatchBinding(pat_id) => {
            // Resolve the binding type from inference (matching completions) so
            // hover agrees with completion type info instead of reporting
            // "unknown". `PatId` is body-local, so walk the use-site's scope
            // chain (collision-safe) rather than probing the enclosing function
            // body first, which could match a same-index binding in another body
            // for pattern/catch bindings inside closures or nested blocks.
            let ty_str = find_binding_ty_in_scopes(db, index, scope_id, pat_id)
                .map(|ty| display_local_binding_ty(db, file, &ty))
                .unwrap_or_else(|| "unknown".to_string());
            Some(TypeInfo::LocalVar {
                name: name.as_str().to_string(),
                ty: ty_str,
            })
        }
    }
}

/// Extract the `PatId` for the binding introduced by `stmt_id`.
///
/// For local declaration statements, returns the pattern ID.
/// Returns `None` for other statement kinds.
fn body_stmt_to_pat_id(
    body: &baml_compiler2_hir::body::FunctionBody,
    stmt_id: baml_compiler2_ast::StmtId,
) -> Option<baml_compiler2_ast::PatId> {
    use baml_compiler2_hir::body::FunctionBody;
    let FunctionBody::Expr(expr_body) = body else {
        return None;
    };

    // `stmt_id` is arena-local to the body it was created in. When the use-site's
    // definition resolves into a *different* ExprBody than `body` (e.g. a binding
    // declared inside a nested testset / lambda), this body's `stmts` arena may
    // not contain that index — indexing it would panic, and a panic aborts the
    // whole wasm runtime. Bounds-check and bail instead.
    let raw = stmt_id.into_raw().into_u32() as usize;
    if raw >= expr_body.stmts.len() {
        return None;
    }
    let stmt = &expr_body.stmts[stmt_id];
    match stmt {
        baml_compiler2_ast::Stmt::Let { pattern, .. }
        | baml_compiler2_ast::Stmt::For {
            binding: pattern, ..
        } => Some(*pattern),
        _ => None,
    }
}

/// Search the use-site's ancestor-scope chain for the binding type of
/// `pat_id`.
///
/// `PatId`s are arena-local to a function/lambda body, so iterating *all*
/// scopes in the file can surface a wrong-arena hit if two bodies happen to
/// allocate the same `PatId` index. Walking ancestors only — Function or
/// Lambda scopes that enclose `from_scope` — restricts the lookup to
/// inferences whose binding maps were populated from the use-site's own
/// arena. Mirrors the structure already used by
/// `completions.rs::find_binding_ty_for_local`.
fn find_binding_ty_in_scopes(
    db: &dyn Db,
    index: &baml_compiler2_hir::semantic_index::FileSemanticIndex<'_>,
    from_scope: baml_compiler2_hir::scope::FileScopeId,
    pat_id: baml_compiler2_ast::PatId,
) -> Option<baml_type::Ty> {
    for ancestor_id in index.ancestor_scopes(from_scope) {
        let scope_id = index.scope_ids[ancestor_id.index() as usize];
        let Some(inference) = baml_compiler2_hir_ty::ide::infer_for_scope(db, scope_id) else {
            continue;
        };
        if let Some(ty) = inference.type_of_pat.get(&pat_id) {
            return Some(ty.to_plain());
        }
    }
    None
}

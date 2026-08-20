//! Structured symbol information: `type_at` (hover) and
//! `type_info_for_definition`, the extraction core shared with `describe`.
//!
//! Regular functions (not Salsa queries) over Salsa-cached compiler data. A
//! cursor position or a resolved definition becomes a serializable
//! [`TypeInfo`]; presentation stays with the consumers (markdown in the LSP
//! protocol layer, ANSI/plain text in the CLI, `--json` via serde).
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
use baml_compiler2_hir::{contributions::Definition, loc::FunctionLoc};
use baml_compiler2_hir_ty::package_interface::ExportedFunction;
use baml_compiler2_ppir::item_data;
use baml_type::BuiltinTypeName;
use serde::Serialize;
use text_size::{TextRange, TextSize};

use crate::render::{self, FnSigParts, SigSlot, SigStyle, TypeForm};

// ── TypeInfo ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MethodSig {
    pub name: String,
    /// Canonical one-line signature, e.g. `function Greet(self) -> string`.
    pub signature: String,
    /// `true` when the first parameter is named `self`.
    pub is_instance: bool,
}

/// Structured type/signature info at a cursor position.
///
/// Returned by `type_at` and `type_info_for_definition`. Plain serializable
/// data: hover *markdown* is the LSP protocol layer's rendering of this
/// struct, never produced here; [`Self::to_describe_block`] is the shared
/// plain-text form. Types are carried as canonical strings, which is also
/// the machine (`--json`) contract.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum TypeInfo {
    /// A function definition: name, parameters, return type.
    Function {
        name: String,
        params: Vec<FunctionParamInfo>,
        return_type: Option<String>,
        throws: Option<String>,
        note: Option<String>,
        /// The owning namespace path (`baml.http`, `root.util`, `root`).
        owner: Option<String>,
        /// The function's full `///` docstring, if any.
        docstring: Option<String>,
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
        /// The owning container path (`user`, `baml.http`).
        owner: Option<String>,
        /// Canonical FQN for the hover "Run `baml describe …`" hint (`string`,
        /// `Foo`, `root.ns.Foo`, `baml.json.JsonObject`).
        canonical_fqn: String,
    },
    /// An enum definition: name, variants.
    Enum {
        name: String,
        variants: Vec<String>,
        owner: Option<String>,
    },
    /// A type alias: name + the expansion type string.
    TypeAlias {
        name: String,
        expansion: String,
        owner: Option<String>,
    },
    /// A template string: name only (no further type info).
    TemplateString { name: String },
    /// A named binding or member slot: a local, a parameter, or a field.
    LocalVar {
        name: String,
        ty: String,
        /// Introduced by `let`/`for`/pattern/catch — rendered with the `let`
        /// keyword, the way rust-analyzer spells binding provenance.
        /// Parameters and fields are spelled bare.
        is_let: bool,
        /// For members: the owning item's path (`baml.errors.Io`); `None`
        /// for locals and parameters.
        owner: Option<String>,
    },
    /// A declaration or resolved expression already rendered as canonical BAML.
    Symbol {
        declaration: String,
        /// The owning container: a method's receiver subject spelled the way
        /// the reader writes the call (`T[]`, `map<K, V>`,
        /// `user.util.Widget<T>`), an interface member's interface path, an
        /// item's namespace path.
        owner: Option<String>,
        /// The declaration's full `///` docstring, if any.
        docstring: Option<String>,
    },
    /// Concise language documentation for a primitive, literal, or keyword.
    Documentation { label: String, detail: String },
    /// A non-structural top-level item (client, generator, test, `retry_policy`).
    OtherItem { name: String, kind: &'static str },
}

impl TypeInfo {
    /// The owning path shown above the declaration (rust-analyzer's
    /// provenance header), when one is known.
    pub fn owner_path(&self) -> Option<&str> {
        match self {
            TypeInfo::Function { owner, .. }
            | TypeInfo::LocalVar { owner, .. }
            | TypeInfo::Class { owner, .. }
            | TypeInfo::Enum { owner, .. }
            | TypeInfo::TypeAlias { owner, .. }
            | TypeInfo::Symbol { owner, .. } => owner.as_deref(),
            TypeInfo::TemplateString { .. }
            | TypeInfo::Documentation { .. }
            | TypeInfo::OtherItem { .. } => None,
        }
    }

    /// The docstring shown below the declaration, when one is carried.
    pub fn docs(&self) -> Option<&str> {
        match self {
            TypeInfo::Class { docstring, .. }
            | TypeInfo::Function { docstring, .. }
            | TypeInfo::Symbol { docstring, .. } => docstring.as_deref(),
            TypeInfo::Enum { .. }
            | TypeInfo::TypeAlias { .. }
            | TypeInfo::TemplateString { .. }
            | TypeInfo::LocalVar { .. }
            | TypeInfo::Documentation { .. }
            | TypeInfo::OtherItem { .. } => None,
        }
    }

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
                // Both clauses always render: a declaration missing its
                // mandatory return type is an error state, and an absent
                // throws string means the contract is inferred from a body
                // this data could not see.
                let ret = format!(
                    " -> {}",
                    return_type.as_deref().unwrap_or(render::MISSING_RETURN)
                );
                let throws = format!(
                    " throws {}",
                    throws.as_deref().unwrap_or(render::PENDING_INFERENCE)
                );
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
            TypeInfo::Enum { name, variants, .. } => {
                let variant_strs: Vec<String> =
                    variants.iter().map(|v| format!("    {v}")).collect();
                if variant_strs.is_empty() {
                    format!("enum {name} {{}}")
                } else {
                    format!("enum {name} {{\n{}\n}}", variant_strs.join("\n"))
                }
            }
            TypeInfo::TypeAlias {
                name, expansion, ..
            } => format!("type {name} = {expansion}"),
            TypeInfo::TemplateString { name } => format!("template_string {name}"),
            TypeInfo::LocalVar {
                name, ty, is_let, ..
            } => {
                if *is_let {
                    format!("let {name}: {ty}")
                } else {
                    format!("{name}: {ty}")
                }
            }
            TypeInfo::Symbol { declaration, .. } => declaration.clone(),
            TypeInfo::Documentation { label, .. } => label.clone(),
            TypeInfo::OtherItem { name, kind } => format!("{kind} {name}"),
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
pub fn type_at(
    db: &dyn baml_compiler2_ppir::Db,
    file: SourceFile,
    offset: TextSize,
) -> Option<TypeInfo> {
    // ── Step 1: find the token at the cursor ─────────────────────────────────
    let token = crate::syntax::find_token_at_offset(db, file, offset)?;

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

    // Everything name-like goes through the one addressing layer, so hover
    // can never disagree with go-to-definition about what the cursor is on.
    let target = crate::resolve::symbol_at(db, file, offset)?;
    target_type_info(db, target)
}

/// Build `TypeInfo` for a resolved [`SymbolTarget`]. Every arm reads
/// recorded compiler data (firewall items, source maps, inference records) —
/// no span-equality matching, no name heuristics.
fn target_type_info(
    db: &dyn baml_compiler2_ppir::Db,
    target: crate::resolve::SymbolTarget<'_>,
) -> Option<TypeInfo> {
    use crate::resolve::SymbolTarget;

    match target {
        SymbolTarget::Item(def) => Some(type_info_for_definition(db, def)),
        SymbolTarget::Local {
            func,
            func_scope,
            binding,
        } => local_target_type_info(db, func, func_scope, binding),
        SymbolTarget::Field { class, field_index } => {
            let class_data = item_data::class_data(db, class);
            let field = class_data.fields.get(field_index)?;
            // Resolved field types (Salsa-cached), canonical — matching the
            // class hover body; unresolved firewall spelling as fallback.
            let ty = baml_compiler2_hir_ty::lower::resolve_class_fields(db, class)
                .iter()
                .find(|(name, _, _)| *name == field.name)
                .map(|(_, ty, _)| render::display_ty_canonical_for_file(db, class.file(db), ty))
                .unwrap_or_else(|| render::display_type_ref(&class_data.type_refs, field.type_ref));
            // The owner is the class's own self type — the same subject-type
            // rule as methods, so generic owners anchor their params
            // (`user.Box<T>` above `item: T`).
            let self_ty = baml_compiler2_hir_ty::lower::class_self_ty(db, class).to_plain();
            Some(TypeInfo::LocalVar {
                name: field.name.as_str().to_string(),
                ty,
                is_let: false,
                owner: Some(render::display_owner_ty(&self_ty)),
            })
        }
        SymbolTarget::Variant {
            enum_loc,
            variant_index,
        } => {
            let enum_data = item_data::enum_data(db, enum_loc);
            let variant = enum_data.variants.get(variant_index)?;
            // The member shape: `Active: Status` under a `user.Status` owner
            // fence, parallel to a field's `name: string` under its class.
            let qtn = baml_compiler2_hir_ty::lower::qualify_def(
                db,
                Definition::Enum(enum_loc),
                &enum_data.name,
            );
            Some(TypeInfo::Symbol {
                declaration: format!("{}: {}", variant.name.as_str(), enum_data.name.as_str()),
                owner: Some(qtn.to_string()),
                docstring: variant.docstring.clone(),
            })
        }
        SymbolTarget::Method { func } => Some(TypeInfo::Symbol {
            declaration: resolved_function_sig_parts(db, func, None).render(
                db,
                func.file(db),
                hover_sig_style(),
            ),
            owner: method_owner_path(db, func),
            docstring: item_data::function_data(db, func).docstring.clone(),
        }),
        SymbolTarget::InterfaceRequiredMethod {
            iface,
            method_index,
        } => {
            let iface_data = item_data::interface_data(db, iface);
            let method = iface_data.required_methods.get(method_index)?;
            let qtn = baml_compiler2_hir_ty::lower::qualify_def(
                db,
                Definition::Interface(iface),
                &iface_data.name,
            );
            Some(TypeInfo::Symbol {
                declaration: FnSigParts::of_interface_method(iface_data, method).render(
                    db,
                    iface.file(db),
                    hover_sig_style(),
                ),
                owner: Some(qtn.to_string()),
                docstring: method.docstring.clone(),
            })
        }
        SymbolTarget::InterfaceField { iface, field_index } => {
            let iface_data = item_data::interface_data(db, iface);
            let field = iface_data.fields.get(field_index)?;
            let qtn = baml_compiler2_hir_ty::lower::qualify_def(
                db,
                Definition::Interface(iface),
                &iface_data.name,
            );
            Some(TypeInfo::LocalVar {
                name: field.name.as_str().to_string(),
                ty: render::display_type_ref(&iface_data.type_refs, field.type_ref),
                is_let: false,
                owner: Some(qtn.to_string()),
            })
        }
    }
}

/// `TypeInfo::LocalVar` for a local binding, from inference records.
fn local_target_type_info(
    db: &dyn baml_compiler2_ppir::Db,
    func: baml_compiler2_hir::loc::FunctionLoc<'_>,
    func_scope: baml_compiler2_hir::scope::FileScopeId,
    binding: baml_compiler2_hir::semantic_index::BindingId,
) -> Option<TypeInfo> {
    use baml_compiler2_hir::semantic_index::BindingKind;

    let file = func.file(db);
    let index = baml_compiler2_hir::file_semantic_index(db, file);
    match binding.kind {
        BindingKind::Parameter(idx) => {
            // The declared type from the arena-backed signature data, so
            // generic arguments are preserved (`Lorem<int>`). A *lambda*
            // parameter's scope is not the function's own scope and its type
            // is not declared anywhere the firewall records — pending
            // inference support, it renders as the inference hole.
            if binding.scope != func_scope {
                let bindings = &index.scope_bindings[binding.scope.index() as usize];
                let (name, _) = bindings
                    .params
                    .iter()
                    .find(|(_, param_idx)| *param_idx == idx)?;
                return Some(TypeInfo::LocalVar {
                    name: name.as_str().to_string(),
                    ty: render::PENDING_INFERENCE.to_string(),
                    is_let: false,
                    owner: None,
                });
            }
            let data = item_data::function_data(db, func);
            let param = data.params.get(idx)?;
            let ty = param
                .type_ref
                .map(|id| data.type_refs.display(id).to_string())
                .unwrap_or_else(|| {
                    if param.name.as_str() == "self" {
                        "Self".to_string()
                    } else {
                        render::MISSING_RETURN.to_string()
                    }
                });
            Some(TypeInfo::LocalVar {
                name: param.name.as_str().to_string(),
                ty,
                is_let: false,
                owner: None,
            })
        }
        BindingKind::Local(idx) => {
            let local = index.scope_bindings[binding.scope.index() as usize]
                .bindings
                .get(idx as usize)?;
            // `bind_pattern` is the per-name identity inference keys binding
            // types on; the ancestor-scope walk keeps `PatId` lookups inside
            // the use-site's own arena.
            let ty = find_binding_ty_in_scopes(db, index, binding.scope, local.bind_pattern)
                .map(|ty| render::display_ty_for_file(db, file, &ty))
                .unwrap_or_else(|| render::PENDING_INFERENCE.to_string());
            Some(TypeInfo::LocalVar {
                name: local.name.as_str().to_string(),
                ty,
                is_let: true,
                owner: None,
            })
        }
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

fn builtin_type_info(db: &dyn baml_compiler2_ppir::Db, token: &SyntaxToken) -> Option<TypeInfo> {
    let builtin = BuiltinTypeName::from_alias(token.text())?;
    let detail = match builtin {
        BuiltinTypeName::Primitive(_) => {
            let crate::listing::ResolvedTarget::Item(Definition::Class(class_loc)) =
                crate::listing::resolve_builtin_type_target(db, builtin.alias())?
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

/// The owning container of `file`'s items: the compiler's package name
/// plus the namespace path (`user`, `user.util`, `baml.http`). Assembled
/// from `file_package`'s components with no eliding and no special cases —
/// the spelling changes automatically when workspace packages gain real
/// names.
fn owning_path(db: &dyn baml_compiler2_ppir::Db, file: SourceFile) -> String {
    let pkg = baml_compiler2_hir::file_package::file_package(db, file);
    let mut path = pkg.package.as_str().to_string();
    for segment in &pkg.namespace_path {
        path.push('.');
        path.push_str(segment.as_str());
    }
    path
}

/// The owner line for a method: the subject type of its declaring container,
/// spelled the way the reader writes the receiver. The compiler's
/// [`item_data::method_owner`] names the container; `class_self_ty`'s builtin
/// bridging spells the companion classes structurally (`T[]`, `map<K, V>`,
/// `string`) with no per-class special case, an out-of-body `implements`
/// block spells its resolved for-target, and an interface default method
/// spells the interface's path. Full canonical paths throughout — member
/// owners never elide.
fn method_owner_path(
    db: &dyn baml_compiler2_ppir::Db,
    func: baml_compiler2_hir::loc::FunctionLoc<'_>,
) -> Option<String> {
    use baml_compiler2_ppir::item_data::MethodOwner;

    match item_data::method_owner(db, func)? {
        MethodOwner::Class(class) => {
            let self_ty = baml_compiler2_hir_ty::lower::class_self_ty(db, class).to_plain();
            Some(render::display_owner_ty(&self_ty))
        }
        MethodOwner::Interface(iface) => {
            let name = &item_data::interface_data(db, iface).name;
            let qtn =
                baml_compiler2_hir_ty::lower::qualify_def(db, Definition::Interface(iface), name);
            Some(qtn.to_string())
        }
        MethodOwner::FreeImpl(block) => {
            // `impl_facts` is `None` when the block's header does not resolve
            // to an interface — honest absence beats a wrong owner.
            let facts = baml_compiler2_hir_ty::impls::impl_facts(db, block).as_ref()?;
            Some(render::display_owner_ty(&facts.for_ty_pattern.to_plain()))
        }
    }
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
    db: &dyn baml_compiler2_ppir::Db,
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
            // The compiler's method→owner record, not a membership scan —
            // the same source the hover owner line reads.
            let origin = match item_data::method_owner(db, *func_loc) {
                Some(item_data::MethodOwner::Class(class)) => format!(
                    "method {}.{}",
                    item_data::class_data(db, class).name.as_str(),
                    data.name.as_str()
                ),
                Some(item_data::MethodOwner::Interface(iface)) => format!(
                    "method {}.{}",
                    item_data::interface_data(db, iface).name.as_str(),
                    data.name.as_str()
                ),
                Some(item_data::MethodOwner::FreeImpl(block)) => {
                    let subject = baml_compiler2_hir_ty::impls::impl_facts(db, block)
                        .as_ref()
                        .map(|facts| render::display_owner_ty(&facts.for_ty_pattern.to_plain()));
                    match subject {
                        Some(subject) => format!("method {}.{}", subject, data.name.as_str()),
                        None => format!("function {}", data.name.as_str()),
                    }
                }
                None => format!("function {}", data.name.as_str()),
            };
            let bound = render::render_generic_bounds(declared, &data.type_refs);
            candidates.push((source_map.span.len(), origin, bound));
        }
    }

    // Free `implements … for …` blocks declare their own generic frame
    // (`implement<T extends Compare> Sortable for T[]`); the header's params
    // are hoverable like any class/interface param.
    for impl_loc in item_data::file_impls(db, file) {
        let source_map = item_data::impl_block_source_map(db, *impl_loc);
        if !range_contains(source_map.span, offset) {
            continue;
        }
        let data = item_data::impl_block_data(db, *impl_loc);
        let item_data::ImplSubjectData::Free { generics, .. } = &data.subject else {
            continue;
        };
        if let Some(declared) = generics.iter().find(|param| &param.name == name) {
            let subject = baml_compiler2_hir_ty::impls::impl_facts(db, *impl_loc)
                .as_ref()
                .map_or_else(
                    || "implements".to_string(),
                    |facts| {
                        format!(
                            "implements for {}",
                            render::display_owner_ty(&facts.for_ty_pattern.to_plain())
                        )
                    },
                );
            candidates.push((
                source_map.span.len(),
                subject,
                render::render_generic_bounds(declared, &data.type_refs),
            ));
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
                render::render_generic_bounds(declared, &data.type_refs),
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
                    render::render_generic_bounds(declared, &data.type_refs),
                ));
            }
        }
        if let Some(declared) = data.generic_params.iter().find(|param| &param.name == name) {
            candidates.push((
                source_map.span.len(),
                format!("interface {}", data.name.as_str()),
                render::render_generic_bounds(declared, &data.type_refs),
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

// ── type_info_for_definition ──────────────────────────────────────────────────

/// Build `TypeInfo` for a top-level item definition.
pub fn type_info_for_definition(db: &dyn baml_compiler2_ppir::Db, def: Definition<'_>) -> TypeInfo {
    match def {
        Definition::Function(func_loc) => {
            let file = func_loc.file(db);
            let pkg_info = baml_compiler2_hir::file_package::file_package(db, file);
            let pkg_id = baml_compiler2_hir::package::PackageId::new(db, pkg_info.package.clone());
            let iface = baml_compiler2_hir_ty::package_interface::package_interface(db, pkg_id);
            let data = item_data::function_data(db, func_loc);

            let Some(exported) = iface.lookup_function(&pkg_info.namespace_path, &data.name) else {
                // Unresolved fallback, from the firewall signature. A missing
                // return annotation stays `None` (rendered `!error`); an
                // omitted `throws` stays `None` (the contract is inferred
                // from the body, which this data cannot see — rendered `_`).
                let params = data
                    .params
                    .iter()
                    .map(|param| FunctionParamInfo {
                        name: param.name.as_str().to_string(),
                        ty: param
                            .type_ref
                            .map(|id| render::display_type_ref(&data.type_refs, id))
                            .unwrap_or_else(|| render::MISSING_RETURN.to_string()),
                        optional: param.has_default,
                    })
                    .collect();
                return TypeInfo::Function {
                    name: data.name.as_str().to_string(),
                    params,
                    return_type: data
                        .return_type
                        .map(|id| render::display_type_ref(&data.type_refs, id)),
                    throws: data
                        .throws
                        .map(|id| render::display_type_ref(&data.type_refs, id)),
                    note: None,
                    owner: Some(owning_path(db, file)),
                    docstring: data.docstring.clone(),
                };
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
            // The resolved contract always renders, `never` included: hover
            // states what the compiler checked, not what the source spelled.
            let throws = Some(display_surface_ty(db, file, &exported.callable_throws));
            TypeInfo::Function {
                name: exported.name.as_str().to_string(),
                params,
                return_type,
                throws,
                note: callback_forwarding_note(exported),
                owner: Some(owning_path(db, file)),
                docstring: data.docstring.clone(),
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
                        render::display_ty_canonical_for_file(db, class_loc.file(db), ty),
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
            let canonical_fqn = render::canonical_fqn_string(&qtn);
            let methods = class_method_sigs(db, class_loc);

            let generic_params =
                render::render_generic_params(&class_data.generic_params, &class_data.type_refs);

            TypeInfo::Class {
                name: class_name,
                generic_params,
                fields,
                implements,
                methods,
                docstring: class_data.docstring.clone(),
                canonical_fqn,
                owner: Some(owning_path(db, class_loc.file(db))),
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
                owner: Some(owning_path(db, enum_loc.file(db))),
            }
        }

        Definition::Interface(iface_loc) => {
            let iface = baml_compiler2_ppir::item_data::interface_data(db, iface_loc);
            let generic_params =
                render::render_generic_params(&iface.generic_params, &iface.type_refs);
            let generics = if generic_params.is_empty() {
                String::new()
            } else {
                format!("<{}>", generic_params.join(", "))
            };
            TypeInfo::Symbol {
                declaration: format!("interface {}{generics}", iface.name.as_str()),
                owner: Some(owning_path(db, iface_loc.file(db))),
                docstring: iface.docstring.clone(),
            }
        }

        Definition::TypeAlias(alias_loc) => {
            let alias_data = baml_compiler2_ppir::item_data::type_alias_data(db, alias_loc);
            let alias_name = alias_data.name.as_str().to_string();

            // Use the resolved (lowered) type for display.
            let resolved = baml_compiler2_hir_ty::lower::type_alias_value(db, alias_loc).to_plain();
            let expansion = render::display_ty_for_file(db, alias_loc.file(db), &resolved);

            TypeInfo::TypeAlias {
                name: alias_name,
                expansion,
                owner: Some(owning_path(db, alias_loc.file(db))),
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

fn display_surface_ty(
    db: &dyn baml_compiler2_ppir::Db,
    file: SourceFile,
    ty: &baml_type::Ty,
) -> String {
    render::display_ty_for_file(db, file, ty)
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
        let callback_name = matching_params
            .pop()
            .unwrap_or_else(|| unreachable!("length checked to be exactly one above"));
        Some(format!(
            "Forwards whatever callback `{callback_name}` throws."
        ))
    } else {
        None
    }
}

// ── local_type_info ───────────────────────────────────────────────────────────

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
    db: &dyn baml_compiler2_ppir::Db,
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

// ── Signature construction ────────────────────────────────────────────────────

/// The hover declaration style: keyword + name + generics, full
/// type-reference spellings, builtin companions collapsed to their aliases.
fn hover_sig_style() -> SigStyle {
    SigStyle {
        keyword_and_name: true,
        hide_self_receiver: false,
        type_form: TypeForm::Full,
        canonical_resolved: true,
    }
}

/// The method-listing style (hover hint + describe): like hover, but
/// unresolved fallbacks use the brief type spelling — method lists are dense
/// and the resolved types carry the precision.
pub(crate) fn method_sig_style() -> SigStyle {
    SigStyle {
        keyword_and_name: true,
        hide_self_receiver: false,
        type_form: TypeForm::Brief,
        canonical_resolved: true,
    }
}

/// Signature parts for a function with each slot preferring the *resolved*
/// exported signature (params by position, return type, checked throws) and
/// falling back to the firewall type references. The one builder behind
/// hover and describe method signatures.
///
/// A `self` receiver stays bare (as written): the exported signature carries
/// its resolved type, but the reader spelled none.
pub fn resolved_function_sig_parts<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    func_loc: FunctionLoc<'db>,
    exported: Option<&'db ExportedFunction>,
) -> FnSigParts<'db> {
    let data = item_data::function_data(db, func_loc);
    let mut parts = FnSigParts::of_function_data(data);
    let Some(exported) = exported else {
        return parts;
    };
    for (idx, param) in parts.params.iter_mut().enumerate() {
        if idx == 0 && param.name == "self" {
            continue;
        }
        if let Some(resolved) = exported.params.get(idx) {
            param.ty = Some(SigSlot::Resolved(&resolved.ty));
        }
    }
    parts.ret = SigSlot::Resolved(&exported.return_type);
    parts.throws = SigSlot::Resolved(&exported.callable_throws);
    parts
}

/// Instance + static method signatures for a class — the hover "has
/// methods?" hint and the describe renderers share this one enumeration.
///
/// Resolved param/return/throws types come from the package interface, which
/// lowers class methods 1:1 with `class_data.methods` (same order), so
/// positional indices line up.
pub(crate) fn class_method_sigs(
    db: &dyn baml_compiler2_ppir::Db,
    class_loc: baml_compiler2_hir::loc::ClassLoc<'_>,
) -> Vec<MethodSig> {
    use baml_compiler2_hir_ty::package_interface::ExportedType;

    let file = class_loc.file(db);
    let class_data = item_data::class_data(db, class_loc);

    let pkg_info = baml_compiler2_hir::file_package::file_package(db, file);
    let pkg_id = baml_compiler2_hir::package::PackageId::new(db, pkg_info.package.clone());
    let iface = baml_compiler2_hir_ty::package_interface::package_interface(db, pkg_id);
    let exported = iface
        .lookup_type(&pkg_info.namespace_path, &class_data.name)
        .and_then(|t| match t {
            ExportedType::Class { methods, .. } => Some(methods),
            ExportedType::Enum { .. }
            | ExportedType::Interface { .. }
            | ExportedType::TypeAlias { .. } => None,
        });

    let mut out = Vec::new();
    for (idx, &method_loc) in class_data.methods.iter().enumerate() {
        let method = item_data::function_data(db, method_loc);
        if method.metadata.is_language_internal {
            continue;
        }
        let is_instance = method
            .params
            .first()
            .is_some_and(|p| p.name.as_str() == "self");
        let signature = resolved_function_sig_parts(
            db,
            method_loc,
            exported.and_then(|methods| exported_method(methods, idx, &method.name)),
        )
        .render(db, file, method_sig_style());
        out.push(MethodSig {
            name: method.name.as_str().to_string(),
            signature,
            is_instance,
        });
    }
    out
}

/// The exported signature lowered from the method at `idx`, when the
/// positional row matches by name (mid-edit the two lists can skew).
pub(crate) fn exported_method<'db>(
    methods: &'db [ExportedFunction],
    idx: usize,
    name: &Name,
) -> Option<&'db ExportedFunction> {
    methods
        .get(idx)
        .filter(|ef| ef.name.as_str() == name.as_str())
}

#[cfg(test)]
mod tests {
    use super::{TypeInfo, type_at};
    use crate::test_support::CursorTest;

    /// The `TypeInfo` at the fixture cursor, or a panic with context.
    fn info_at(test: &CursorTest) -> TypeInfo {
        type_at(&test.db, test.cursor.file, test.cursor.offset)
            .unwrap_or_else(|| panic!("expected type info at the fixture cursor"))
    }

    #[test]
    fn client_config_hover_only_documents_the_key_token() {
        let key = CursorTest::new(
            r#"client<llm> Example {
  prov<[CURSOR]ider openai
}"#,
        );
        let TypeInfo::Documentation { label, .. } = info_at(&key) else {
            panic!("config key hovers as documentation");
        };
        assert!(label.contains("provider <name>"));

        let value = CursorTest::new(
            r#"client<llm> Example {
  provider open<[CURSOR]ai
}"#,
        );
        let value_info = type_at(&value.db, value.cursor.file, value.cursor.offset);
        assert!(
            !matches!(
                &value_info,
                Some(TypeInfo::Documentation { label, .. }) if label.contains("provider <name>")
            ),
            "the value token must not document the key, got: {value_info:?}"
        );
    }

    #[test]
    fn schema_attribute_hover_only_documents_the_attribute_name() {
        let name = CursorTest::new(
            r#"class Example {
  value string @descrip<[CURSOR]tion("Displayed value")
}"#,
        );
        let TypeInfo::Documentation { label, .. } = info_at(&name) else {
            panic!("attribute name hovers as documentation");
        };
        assert!(label.contains(r#"@description("text")"#));

        let argument = CursorTest::new(
            r#"class Example {
  value string @description("Displayed <[CURSOR]value")
}"#,
        );
        let block = info_at(&argument).to_describe_block();
        assert!(!block.contains(r#"@description("text")"#));
    }

    #[test]
    fn builtin_type_hover_uses_stdlib_docstring() {
        let test = CursorTest::new(
            r#"class Example {
  value i<[CURSOR]nt
}"#,
        );
        let TypeInfo::Documentation { detail, .. } = info_at(&test) else {
            panic!("builtin type hovers as documentation");
        };
        assert!(detail.contains("A 63-bit signed integer. Range: -2^62 to 2^62-1"));
        assert!(!detail.contains("with checked arithmetic"));
    }

    #[test]
    fn intrinsic_type_hover_uses_language_topic() {
        let test = CursorTest::new(
            r#"function stop() -> ne<[CURSOR]ver {
  throw "stop"
}"#,
        );
        let TypeInfo::Documentation { detail, .. } = info_at(&test) else {
            panic!("intrinsic type hovers as documentation");
        };
        assert!(detail.contains("The bottom type of an expression that never returns normally."));
    }

    #[test]
    fn function_info_uses_resolved_callback_surface() {
        let test = CursorTest::new(
            r#"function <[CURSOR]forward(cb: (x: int) -> int) -> int {
  return cb(1)
}"#,
        );

        let info = info_at(&test);
        let block = info.to_describe_block();
        assert!(
            block.contains(
                "function forward(cb: (x: int) -> int throws callback) -> int throws callback"
            ),
            "expected resolved callback throws surface, got:\n{block}"
        );
        let TypeInfo::Function { note, .. } = info else {
            panic!("function info expected");
        };
        assert_eq!(
            note.as_deref(),
            Some("Forwards whatever callback `cb` throws.")
        );
    }

    #[test]
    fn function_info_states_resolved_never_throws_explicitly() {
        // The old renderer omitted `throws never`; the resolved contract now
        // always renders — hover states what the compiler checked.
        let test = CursorTest::new(
            r#"function <[CURSOR]plain(x: int) -> int {
  return x + 1
}"#,
        );

        let block = info_at(&test).to_describe_block();
        assert_eq!(block, "function plain(x: int) -> int throws never");
    }

    #[test]
    fn function_info_shows_inferred_throws_not_never() {
        // An omitted `throws` clause is an inferred contract: a body that
        // throws must surface the thrown type, never `throws never`.
        let test = CursorTest::new(
            r#"function <[CURSOR]risky() -> int {
  throw "boom"
}"#,
        );

        // The inferred contract is exact: the literal type `"boom"`, not a
        // widened `string`.
        let block = info_at(&test).to_describe_block();
        assert!(
            block.contains(r#"throws "boom""#),
            "inferred throw contract must render, got:\n{block}"
        );
        assert!(
            !block.contains("throws never"),
            "a throwing body can never surface `throws never`, got:\n{block}"
        );
    }

    #[test]
    fn function_info_shows_explicit_throws_surface() {
        let test = CursorTest::new(
            r#"function <[CURSOR]risky() -> int throws string {
  throw "boom"
}"#,
        );

        let block = info_at(&test).to_describe_block();
        assert!(
            block.contains("function risky() -> int throws string"),
            "expected explicit throws surface, got:\n{block}"
        );
    }

    #[test]
    fn function_info_shows_defaulted_params_as_optional() {
        let test = CursorTest::new(
            r#"function <[CURSOR]search(query: string, max_results: int = 10, filter: string? = null) -> int {
  return max_results
}"#,
        );

        let block = info_at(&test).to_describe_block();
        assert!(
            block.contains(
                "function search(query: string, max_results?: int, filter?: string | null) -> int"
            ),
            "expected defaulted params to render with optional markers, got:\n{block}"
        );
    }

    #[test]
    fn local_function_type_info_preserves_optional_param_markers() {
        let test = CursorTest::new(
            r#"function combine(x: int, a: int = 10, b: int = 100) -> int {
  return x + a + b
}

function main() -> int {
  let <[CURSOR]f: (x: int, b?: int) -> int = combine
  return f(1, b = 5)
}"#,
        );

        assert_eq!(
            info_at(&test).to_describe_block(),
            "let f: (x: int, b?: int) -> int throws never"
        );
    }

    #[test]
    fn local_var_info_for_for_loop_binding_uses_iterable_item_type() {
        let test = CursorTest::new(
            r#"function sum() -> int {
  let total = 0
  for (let <[CURSOR]x in [1, 2]) {
    total += x
  }
  return total
}"#,
        );

        assert_eq!(info_at(&test).to_describe_block(), "let x: int");
    }

    #[test]
    fn class_info_lists_out_of_body_implements() {
        let test = CursorTest::new(
            r#"
interface Animal {
  function speak(self) -> string throws never
}

class Dog<[CURSOR] {
  name: string
}

implements Animal for Dog {
  function speak(self) -> string { return self.name }
}
"#,
        );

        let block = info_at(&test).to_describe_block();
        assert!(
            block.contains("implements Animal {}"),
            "expected class info to surface out-of-body implements, got:\n{block}"
        );
    }

    #[test]
    fn class_info_shows_associated_type_bindings_in_implements() {
        let test = CursorTest::new(
            r#"
interface Decoder<Input> {
  type Output
  function decode(self, raw: Input) -> Self.Output throws never
}

class IntDecoder<[CURSOR] {
  implements Decoder<string> {
    type Output = int
    function decode(self, raw: string) -> Self.Output { return 1 }
  }
}
"#,
        );

        let block = info_at(&test).to_describe_block();
        assert!(
            block.contains("type Output = int"),
            "expected class info to include associated type bindings, got:\n{block}"
        );
    }

    #[test]
    fn class_info_carries_docstring_methods_and_fqn_but_a_fields_only_block() {
        let test = CursorTest::new(
            r#"/// Does foo things.
class Foo<[CURSOR] {
    bar int

    function greet(self) -> string {
        "hi"
    }
}"#,
        );

        let info = info_at(&test);
        let TypeInfo::Class {
            docstring,
            methods,
            canonical_fqn,
            ..
        } = &info
        else {
            panic!("class info expected");
        };
        // The LSP layer renders the describe hint and docstring fencing from
        // these fields; the data must be here, the presentation must not.
        assert_eq!(docstring.as_deref(), Some("Does foo things."));
        assert_eq!(canonical_fqn, "Foo");
        assert!(
            methods.iter().any(|m| m.name == "greet" && m.is_instance),
            "methods carry the describe hint, got: {methods:?}"
        );

        let block = info.to_describe_block();
        assert!(
            block.contains("bar: int,"),
            "expected field shape, got:\n{block}"
        );
        assert!(
            !block.contains("function greet"),
            "the body block is fields-only, got:\n{block}"
        );
    }

    #[test]
    fn class_info_without_methods_has_empty_method_list() {
        let test = CursorTest::new(
            r#"class Point<[CURSOR] {
    x int
    y int
}"#,
        );

        let info = info_at(&test);
        let TypeInfo::Class { methods, .. } = &info else {
            panic!("class info expected");
        };
        assert!(methods.is_empty());
        let block = info.to_describe_block();
        assert!(block.contains("x: int,") && block.contains("y: int,"));
    }

    /// Destructure a `TypeInfo::Symbol` or panic with context.
    fn symbol_parts(info: TypeInfo) -> (String, Option<String>, Option<String>) {
        let TypeInfo::Symbol {
            declaration,
            owner,
            docstring,
        } = info
        else {
            panic!("expected a Symbol hover, got: {info:?}");
        };
        (declaration, owner, docstring)
    }

    #[test]
    fn array_method_owner_is_the_companion_subject_type() {
        let test = CursorTest::new(
            r#"function example() -> int {
  let xs: int[] = [1, 2, 3];
  xs.a<[CURSOR]t(0);
  0
}"#,
        );
        let (declaration, owner, docstring) = symbol_parts(info_at(&test));
        assert_eq!(
            declaration,
            "function at(self, index: int) -> T? throws never"
        );
        assert_eq!(owner.as_deref(), Some("T[]"));
        assert!(
            docstring
                .as_deref()
                .is_some_and(|d| d.contains("Negative indices")),
            "stdlib method docstring rides along, got: {docstring:?}"
        );
    }

    #[test]
    fn map_method_owner_is_the_companion_subject_type() {
        let test = CursorTest::new(
            r#"function example() -> int {
  let scores: map<string, int> = { "a": 1 };
  scores.ke<[CURSOR]ys();
  0
}"#,
        );
        let (declaration, owner, _) = symbol_parts(info_at(&test));
        assert_eq!(declaration, "function keys(self) -> K[] throws never");
        assert_eq!(owner.as_deref(), Some("map<K, V>"));
    }

    #[test]
    fn free_impl_method_owner_is_the_for_target() {
        // `sum` on `int[]` lives in stdlib's `implements Summable for int[]`:
        // the owner is the impl header's for-target, not a companion path.
        let test = CursorTest::new(
            r#"function example() -> int {
  let xs: int[] = [1, 2, 3];
  xs.su<[CURSOR]m()
}"#,
        );
        // (`sum`'s docstring sits on the `implements` block, not the
        // method — block docs are not folded onto members.)
        let (_, owner, _) = symbol_parts(info_at(&test));
        assert_eq!(owner.as_deref(), Some("int[]"));
    }

    #[test]
    fn blanket_impl_method_owner_keeps_the_impl_generics() {
        // `sort` comes from the blanket `implements Sortable for T[]`.
        let test = CursorTest::new(
            r#"function example() -> int[] {
  let xs: int[] = [3, 1, 2];
  xs.so<[CURSOR]rt()
}"#,
        );
        let (_, owner, _) = symbol_parts(info_at(&test));
        assert_eq!(owner.as_deref(), Some("T[]"));
    }

    #[test]
    fn class_method_owner_is_the_full_self_type_path() {
        let test = CursorTest::new(
            r#"class Widget {
    label string

    function describe_self(self) -> string {
        self.label
    }
}

function example(w: Widget) -> string {
    w.describe<[CURSOR]_self()
}"#,
        );
        let (_, owner, _) = symbol_parts(info_at(&test));
        assert_eq!(owner.as_deref(), Some("user.Widget"));
    }

    #[test]
    fn interface_default_method_owner_is_the_interface_path() {
        let test = CursorTest::new(
            r#"interface Greeter {
    /// Say hello.
    function greet(self) -> string throws never {
        "hi"
    }
}

class Widget {
    implements Greeter {}
}

function example(w: Widget) -> string {
    w.gre<[CURSOR]et()
}"#,
        );
        let (_, owner, docstring) = symbol_parts(info_at(&test));
        assert_eq!(owner.as_deref(), Some("user.Greeter"));
        assert_eq!(docstring.as_deref(), Some("Say hello."));
    }

    #[test]
    fn variant_hover_is_a_member_under_its_enum_owner() {
        let test = CursorTest::new(
            r#"enum Status {
    Active
    Inactive
}

function example() -> Status {
    Status.Act<[CURSOR]ive
}"#,
        );
        let (declaration, owner, _) = symbol_parts(info_at(&test));
        assert_eq!(declaration, "Active: Status");
        assert_eq!(owner.as_deref(), Some("user.Status"));
    }

    #[test]
    fn free_impl_header_generic_param_hovers_with_its_subject() {
        let test = CursorTest::new(
            r#"interface Wrap {
    function describe_wrapped(self) -> string throws never
}

implements<El<[CURSOR]em extends Wrap> Wrap for Elem[] {
    function describe_wrapped(self) -> string throws never {
        "many"
    }
}"#,
        );
        let TypeInfo::Documentation { label, detail } = info_at(&test) else {
            panic!("generic params hover as documentation");
        };
        assert_eq!(label, "type parameter Elem extends Wrap");
        assert_eq!(detail, "Declared by `implements for Elem[]`.");
    }

    #[test]
    fn item_owners_are_the_compiler_package_path() {
        // Class/enum owners come from `file_package`, never from string
        // surgery on the canonical fqn (which spelled `root`).
        let class = CursorTest::new(
            r#"class Poi<[CURSOR]nt {
    x int
}"#,
        );
        assert_eq!(info_at(&class).owner_path(), Some("user"));

        let en = CursorTest::new(
            r#"enum Sta<[CURSOR]tus {
    Active
}"#,
        );
        assert_eq!(info_at(&en).owner_path(), Some("user"));
    }
}

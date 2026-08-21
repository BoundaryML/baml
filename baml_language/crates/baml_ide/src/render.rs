//! Rendering: the one place IDE output turns types and signatures into
//! strings.
//!
//! Three layers, one policy:
//!
//! - **Resolved types** ([`display_ty_for_file`], [`display_ty_canonical_for_file`],
//!   [`display_ty`]): render a [`Ty`] via [`TyRenderStrategy`], so the
//!   structural walk lives once in `baml_type` and this module only supplies
//!   naming policy (shortest unambiguous spelling for the file's
//!   package/namespace, `callback` for synthetic effect params, the
//!   streaming-only `(evolving)` annotation hidden).
//! - **Unresolved types** ([`display_type_ref`]): the brief source-level form
//!   of a firewall type reference (last path segment only, generics
//!   dropped). Features render unresolved types from firewall item data
//!   only — never from AST `TypeExpr`s — so the spelling cannot fork between
//!   consumers.
//! - **Signatures** ([`FnSigParts`]): one layout engine for every function
//!   signature the IDE prints (hover, describe, completion details). Callers
//!   choose the *content* — which type source fills each slot — and a
//!   [`SigStyle`]; the layout itself is written once.

use baml_base::{Name, SourceFile};
use baml_compiler2_hir::{
    package::PackageItems,
    type_ref::{TypeRefId, TypeRefStore},
};
use baml_compiler2_ppir::item_data::{
    FunctionData, GenericParamData, InterfaceData, InterfaceMethodSigData,
};
use baml_type::{QualifiedTypeName, Ty, TyRenderStrategy, user_facing::humanize_type_string};

// ── Resolved-type rendering ───────────────────────────────────────────────────

/// Context for hover/completion type rendering: knows the file's current
/// package + namespace so qualified names collapse to the shortest
/// unambiguous form (bare when in scope, `root.path` when not, the dependency
/// package prefix for cross-package types). Implements [`TyRenderStrategy`]
/// so the structural walk lives once in `baml_type`.
struct TyDisplayContext<'db> {
    current_package: Name,
    current_namespace: Vec<Name>,
    package_items: &'db PackageItems<'db>,
    /// When set, collapse builtin companion classes to their lowercase
    /// primitive/keyword alias (`baml.String` → `string`, `baml.json.json` →
    /// `json`). Only the describe + hover + signature paths opt in (via
    /// [`display_ty_canonical_for_file`]); diagnostics/completions/inlay
    /// hints keep the un-collapsed spelling.
    collapse_aliases: bool,
}

impl TyDisplayContext<'_> {
    fn display_qtn(&self, qtn: &QualifiedTypeName) -> String {
        if self.collapse_aliases
            && let Some(alias) = qtn.builtin_alias()
        {
            return alias.to_string();
        }

        if qtn.package() == &self.current_package && self.can_use_bare_name(qtn) {
            return qtn.name().to_string();
        }

        // Everything non-bare spells the full canonical path — real package
        // names, never the `root.` source shorthand (correct only inside the
        // defining package, and signatures are read from outside it).
        //
        // Through `source_spelling`, not `Display`: a runtime-minted
        // declaration carries a hidden `$dyn.<mint>` discriminator that keys
        // its identity in the VM, and no user can write it. Below the
        // discriminator the name is the one the source wrote, which is what
        // every reader of this path — hover, describe, completions, and the
        // diagnostics a runtime compile hands back — must be shown.
        qtn.source_spelling().to_string()
    }

    fn can_use_bare_name(&self, qtn: &QualifiedTypeName) -> bool {
        // Namespace comparisons read `source_namespace` for the same reason
        // the spelling does: the discriminator is identity, not a namespace
        // the current file could ever sit in.
        if qtn.source_namespace() == self.current_namespace {
            return true;
        }

        if qtn.source_namespace().is_empty() {
            return self
                .package_items
                .lookup_type(&self.current_namespace, qtn.name())
                .is_none();
        }

        false
    }
}

impl TyRenderStrategy for TyDisplayContext<'_> {
    fn qtn(&self, qtn: &QualifiedTypeName) -> String {
        self.display_qtn(qtn)
    }

    fn type_var(&self, name: &Name) -> String {
        if baml_type::is_synthetic_effect_param(name) {
            "callback".to_string()
        } else {
            name.to_string()
        }
    }

    // Hover/completion hide the streaming-only `(evolving)` annotation.
    fn show_evolving(&self) -> bool {
        false
    }
}

/// Context-free strategy: full canonical paths (real package names,
/// including the implicit `user` package), hides `(evolving)`, and shows
/// synthetic effect params as `callback`. Used by [`display_ty`] where no
/// current-package context is available.
struct PlainTyRender;

impl TyRenderStrategy for PlainTyRender {
    fn qtn(&self, qtn: &QualifiedTypeName) -> String {
        qtn.to_string()
    }

    fn type_var(&self, name: &Name) -> String {
        if baml_type::is_synthetic_effect_param(name) {
            "callback".to_string()
        } else {
            name.to_string()
        }
    }

    fn show_evolving(&self) -> bool {
        false
    }
}

/// Render `ty` in `file`'s package/namespace context — the hover/completion
/// form.
pub fn display_ty_for_file(db: &dyn baml_compiler2_ppir::Db, file: SourceFile, ty: &Ty) -> String {
    display_ty_for_file_impl(db, file, ty, false)
}

/// Like [`display_ty_for_file`], but collapses builtin companion classes to
/// their lowercase primitive/keyword alias (`baml.String` → `string`,
/// `baml.media.Image` → `image`, `baml.json.json` → `json`). This is the
/// canonical type printer used by the describe + hover + signature paths;
/// other call sites (diagnostics, completions, inlay hints) keep the
/// un-collapsed [`display_ty_for_file`].
pub fn display_ty_canonical_for_file(
    db: &dyn baml_compiler2_ppir::Db,
    file: SourceFile,
    ty: &Ty,
) -> String {
    display_ty_for_file_impl(db, file, ty, true)
}

fn display_ty_for_file_impl(
    db: &dyn baml_compiler2_ppir::Db,
    file: SourceFile,
    ty: &Ty,
    collapse_aliases: bool,
) -> String {
    let pkg_info = baml_compiler2_hir::file_package::file_package(db, file);
    let pkg_id = baml_compiler2_hir::package::PackageId::new(db, pkg_info.package.clone());
    let package_items = baml_compiler2_ppir::package_items(db, pkg_id);
    let ctx = TyDisplayContext {
        current_package: pkg_info.package,
        current_namespace: pkg_info.namespace_path,
        package_items,
        collapse_aliases,
    };
    ty.render_with(&ctx)
}

/// Format a resolved [`Ty`] as a user-friendly string without file context.
///
/// Full canonical paths so same-short-name types stay distinguishable;
/// synthetic effect params show as `callback`. With file context available,
/// prefer [`display_ty_for_file`].
pub fn display_ty(ty: &Ty) -> String {
    ty.render_with(&PlainTyRender)
}

/// Render `ty` for a hover owner line: full canonical paths — member owners
/// never elide, the reader may be hovering from any package — with builtin
/// companion classes collapsed to their reader-facing alias (`baml.String`
/// → `string`). Combined with `class_self_ty`'s builtin bridging this
/// spells a method's container the way the reader writes the receiver:
/// `T[]`, `map<K, V>`, `string`, `user.util.Widget<T>`.
pub fn display_owner_ty(ty: &Ty) -> String {
    ty.render_with(&OwnerTyRender)
}

/// Strategy for [`display_owner_ty`]: [`PlainTyRender`] plus the companion
/// alias collapse of [`display_ty_canonical_for_file`].
struct OwnerTyRender;

impl TyRenderStrategy for OwnerTyRender {
    fn qtn(&self, qtn: &QualifiedTypeName) -> String {
        qtn.builtin_alias()
            .map_or_else(|| qtn.to_string(), str::to_string)
    }

    fn type_var(&self, name: &Name) -> String {
        if baml_type::is_synthetic_effect_param(name) {
            "callback".to_string()
        } else {
            name.to_string()
        }
    }

    fn show_evolving(&self) -> bool {
        false
    }
}

// ── Unresolved-type rendering (firewall type refs) ────────────────────────────

fn type_ref_needs_postfix_parens(store: &TypeRefStore, id: TypeRefId) -> bool {
    use baml_compiler2_hir::type_ref::TypeRefKind;
    matches!(
        store[id].kind,
        TypeRefKind::Union { .. } | TypeRefKind::Function { .. }
    )
}

fn display_type_ref_as_postfix_base(store: &TypeRefStore, id: TypeRefId) -> String {
    let rendered = display_type_ref(store, id);
    if type_ref_needs_postfix_parens(store, id) {
        format!("({rendered})")
    } else {
        rendered
    }
}

fn display_type_ref_as_function_result(store: &TypeRefStore, id: TypeRefId) -> String {
    use baml_compiler2_hir::type_ref::TypeRefKind;
    let rendered = display_type_ref(store, id);
    if matches!(store[id].kind, TypeRefKind::Function { .. }) {
        format!("({rendered})")
    } else {
        rendered
    }
}

/// Format a firewall type reference as a brief source-level type string
/// (last path segment only, generics dropped). For callers holding firewall
/// item data (`FunctionData::type_refs`, `InterfaceData::type_refs`, …).
///
/// NOTE: this is deliberately NOT
/// [`TypeRefStore::display`], which is the *full* `Display` form (whole path
/// + generic args); hover signatures use that form via [`TypeForm::Full`].
pub fn display_type_ref(store: &TypeRefStore, id: TypeRefId) -> String {
    use baml_compiler2_hir::type_ref::TypeRefKind as K;
    let rendered = match &store[id].kind {
        K::Path { segments, .. } => segments
            .last()
            .map(|n| n.as_str().to_string())
            .unwrap_or_else(|| "unknown".to_string()),
        // A projection has no brief form — render it fully.
        K::AssociatedTypeProjection { .. } => store.display(id).to_string(),
        K::Int => "int".to_string(),
        K::Bigint => "bigint".to_string(),
        K::Float => "float".to_string(),
        K::String => "string".to_string(),
        K::Bool => "bool".to_string(),
        K::Null => "null".to_string(),
        K::Uint8Array => "uint8array".to_string(),
        K::Media { kind } => format!("{kind:?}").to_lowercase(),
        K::Optional { inner } => format!("{}?", display_type_ref_as_postfix_base(store, *inner)),
        K::List { inner } => format!("{}[]", display_type_ref_as_postfix_base(store, *inner)),
        K::Map { key, value } => format!(
            "map<{}, {}>",
            display_type_ref(store, *key),
            display_type_ref(store, *value)
        ),
        K::Union { variants } => variants
            .iter()
            .map(|&v| display_type_ref(store, v))
            .collect::<Vec<_>>()
            .join(" | "),
        K::Literal { value } => value.to_string(),
        K::Function {
            params,
            ret,
            throws,
        } => {
            let ps: Vec<String> = params
                .iter()
                .map(|p| {
                    p.name
                        .as_ref()
                        .map(|n| {
                            let optional = if p.optional { "?" } else { "" };
                            format!("{}{}: {}", n, optional, display_type_ref(store, p.ty))
                        })
                        .unwrap_or_else(|| display_type_ref(store, p.ty))
                })
                .collect();
            let throws = throws
                .map(|t| display_type_ref(store, t))
                .map(|throws| format!(" throws {throws}"))
                .unwrap_or_default();
            format!(
                "({}) -> {}{}",
                ps.join(", "),
                display_type_ref_as_function_result(store, *ret),
                throws
            )
        }
        K::BuiltinUnknown => "unknown".to_string(),
        K::Never => "never".to_string(),
        K::Void => "void".to_string(),
        K::Type => "type".to_string(),
        K::Rust => "$rust_type".to_string(),
        K::Infer => "_".to_string(),
        K::Error | K::Unknown => "unknown".to_string(),
    };
    humanize_type_string(&rendered)
}

/// Which spelling of a firewall type reference fills a [`SigSlot::Syntax`]
/// slot at render time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TypeForm {
    /// Brief source-level form via [`display_type_ref`] (last path segment,
    /// generics dropped) — completion details, describe fallbacks.
    Brief,
    /// Full `Display` form via `TypeRefStore::display` (whole path + generic
    /// args) — hover signatures.
    #[default]
    Full,
}

impl TypeForm {
    fn render(self, store: &TypeRefStore, id: TypeRefId) -> String {
        match self {
            TypeForm::Brief => display_type_ref(store, id),
            TypeForm::Full => store.display(id).to_string(),
        }
    }
}

// ── Signature layout ──────────────────────────────────────────────────────────

/// The rendering of a slot whose *mandatory* annotation is absent mid-edit:
/// the compiler's own spelling for the error type, so hover and completions
/// read the same as a diagnosed `Ty::Error`.
pub const MISSING_RETURN: &str = "!error";

/// The rendering of a slot whose contract is legitimately inferred but not
/// resolved from the data at hand: the language's own inference-hole
/// spelling (`_`, as in `throws AppError | _`).
pub const PENDING_INFERENCE: &str = "_";

/// One type slot of a signature, kept *typed* until [`FnSigParts::render`]:
/// strings exist only at the final render, so naming policy and form live in
/// one place and machine consumers can reach the types themselves.
#[derive(Clone)]
pub enum SigSlot<'db> {
    /// A resolved semantic type (interface exports, inference). Rendered
    /// with the file-context strategy — shortest unambiguous naming.
    Resolved(&'db Ty),
    /// The declaration's own syntax (firewall type reference), for slots
    /// with no resolved type at hand — and the only form that preserves the
    /// user's spelling of expanded aliases.
    Syntax(&'db TypeRefStore, TypeRefId),
    /// A *mandatory* annotation that is absent (mid-edit declarations):
    /// renders [`MISSING_RETURN`]. BAML requires declarations to write
    /// return types (and interface method signatures their `throws`), so
    /// this is an error state to display, never an implicit type.
    Missing,
    /// A contract the language legitimately infers (an omitted `throws`; a
    /// lambda return) that this caller's data cannot see: renders
    /// [`PENDING_INFERENCE`]. Never invent a contract the compiler did not
    /// check.
    Inferred,
    /// A resolved semantic type the caller owns (a by-value query result
    /// such as `callable_throws`); rendered exactly like
    /// [`SigSlot::Resolved`].
    ResolvedOwned(Ty),
}

impl SigSlot<'_> {
    fn render(
        &self,
        db: &dyn baml_compiler2_ppir::Db,
        file: SourceFile,
        style: SigStyle,
    ) -> String {
        match self {
            SigSlot::Resolved(ty) => {
                if style.canonical_resolved {
                    display_ty_canonical_for_file(db, file, ty)
                } else {
                    display_ty_for_file(db, file, ty)
                }
            }
            SigSlot::ResolvedOwned(ty) => {
                if style.canonical_resolved {
                    display_ty_canonical_for_file(db, file, ty)
                } else {
                    display_ty_for_file(db, file, ty)
                }
            }
            SigSlot::Syntax(store, id) => style.type_form.render(store, *id),
            SigSlot::Missing => MISSING_RETURN.to_string(),
            SigSlot::Inferred => PENDING_INFERENCE.to_string(),
        }
    }
}

/// One parameter of a signature.
pub struct SigParam<'db> {
    pub name: String,
    /// Render with the `?` optional marker (default-valued parameter,
    /// [BEP-033]).
    ///
    /// [BEP-033]: https://beps.boundaryml.com/beps/33
    pub optional: bool,
    /// `None` renders the bare name — the `self` receiver, as written.
    pub ty: Option<SigSlot<'db>>,
}

/// The content of a function signature, slots still typed. Constructors
/// fill the slots from firewall item data; features holding resolved types
/// (describe, hover) overwrite individual slots with [`SigSlot::Resolved`].
pub struct FnSigParts<'db> {
    pub name: String,
    /// Rendered generic parameter list; empty renders no `<…>` brackets.
    pub generics: Vec<String>,
    pub params: Vec<SigParam<'db>>,
    /// Always rendered — see [`SigSlot`] for the absent-annotation arms.
    pub ret: SigSlot<'db>,
    /// Always rendered — an explicit ` throws never` when that is the
    /// resolved contract; [`SigSlot::Inferred`] when the source omitted the
    /// clause and only unresolved data is at hand.
    pub throws: SigSlot<'db>,
}

/// Layout options for [`FnSigParts::render`].
#[derive(Debug, Clone, Copy, Default)]
pub struct SigStyle {
    /// `function name<G>(…) …` when set; the bare `(…) -> …` form otherwise
    /// (completion details).
    pub keyword_and_name: bool,
    /// Drop a leading `self` receiver entirely — instance completions, where
    /// the receiver is already spelled in source (`img.base64()`).
    pub hide_self_receiver: bool,
    /// Spelling for [`SigSlot::Syntax`] slots.
    pub type_form: TypeForm,
    /// Collapse builtin companion classes to their lowercase alias in
    /// [`SigSlot::Resolved`] slots ([`display_ty_canonical_for_file`]) —
    /// describe and hover opt in.
    pub canonical_resolved: bool,
}

impl<'db> FnSigParts<'db> {
    /// Signature parts for a function's firewall data. A missing return
    /// annotation is an error ([`SigSlot::Missing`]); a missing `throws`
    /// clause means the contract is inferred from the body, which this
    /// unresolved data cannot see ([`SigSlot::Inferred`]).
    pub fn of_function_data(data: &'db FunctionData) -> FnSigParts<'db> {
        FnSigParts {
            name: data.name.as_str().to_string(),
            generics: render_generic_params(&data.generic_params, &data.type_refs),
            params: data
                .params
                .iter()
                .map(|param| SigParam {
                    name: param.name.as_str().to_string(),
                    optional: param.has_default,
                    ty: param
                        .type_ref
                        .map(|id| SigSlot::Syntax(&data.type_refs, id)),
                })
                .collect(),
            ret: data
                .return_type
                .map(|id| SigSlot::Syntax(&data.type_refs, id))
                .unwrap_or(SigSlot::Missing),
            throws: data
                .throws
                .map(|id| SigSlot::Syntax(&data.type_refs, id))
                .unwrap_or(SigSlot::Inferred),
        }
    }

    /// Signature parts for an interface method signature. Interface method
    /// declarations must declare BOTH the return type and the `throws`
    /// clause, so an absent slot here is a malformed declaration
    /// ([`SigSlot::Missing`]).
    pub fn of_interface_method(
        iface: &'db InterfaceData<'db>,
        method: &'db InterfaceMethodSigData,
    ) -> FnSigParts<'db> {
        FnSigParts {
            name: method.name.as_str().to_string(),
            generics: render_generic_params(&method.generic_params, &iface.type_refs),
            params: method
                .params
                .iter()
                .map(|param| SigParam {
                    name: param.name.as_str().to_string(),
                    optional: param.has_default,
                    ty: param
                        .type_ref
                        .map(|id| SigSlot::Syntax(&iface.type_refs, id)),
                })
                .collect(),
            ret: method
                .return_type
                .map(|id| SigSlot::Syntax(&iface.type_refs, id))
                .unwrap_or(SigSlot::Missing),
            throws: method
                .throws
                .map(|id| SigSlot::Syntax(&iface.type_refs, id))
                .unwrap_or(SigSlot::Missing),
        }
    }

    /// Render with the shared layout: `function name<G>(p: T, q?: U) -> R
    /// throws E`, or the bare `(p: T) -> R throws E` form per `style`.
    /// `file` anchors context-aware naming for [`SigSlot::Resolved`] slots.
    pub fn render(
        &self,
        db: &dyn baml_compiler2_ppir::Db,
        file: SourceFile,
        style: SigStyle,
    ) -> String {
        let params = self
            .params
            .iter()
            .enumerate()
            .filter_map(|(idx, param)| {
                if style.hide_self_receiver && idx == 0 && param.name == "self" {
                    return None;
                }
                Some(match &param.ty {
                    Some(slot) => {
                        let optional = if param.optional { "?" } else { "" };
                        format!(
                            "{}{}: {}",
                            param.name,
                            optional,
                            slot.render(db, file, style)
                        )
                    }
                    None => param.name.clone(),
                })
            })
            .collect::<Vec<_>>()
            .join(", ");
        let ret = format!(" -> {}", self.ret.render(db, file, style));
        let throws = format!(" throws {}", self.throws.render(db, file, style));
        if style.keyword_and_name {
            let generics = if self.generics.is_empty() {
                String::new()
            } else {
                format!("<{}>", self.generics.join(", "))
            };
            format!("function {}{generics}({params}){ret}{throws}", self.name)
        } else {
            format!("({params}){ret}{throws}")
        }
    }
}

/// Rendered generic parameter declarations (`T`, `T extends Bound & Other`).
pub fn render_generic_params(params: &[GenericParamData], store: &TypeRefStore) -> Vec<String> {
    params
        .iter()
        .map(|param| match render_generic_bounds(param, store) {
            Some(bounds) => format!("{} extends {bounds}", param.name.as_str()),
            None => param.name.as_str().to_string(),
        })
        .collect()
}

/// A parameter's declared bounds rendered as source (`A & B`), or `None`
/// when it is unbounded.
pub fn render_generic_bounds(param: &GenericParamData, store: &TypeRefStore) -> Option<String> {
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

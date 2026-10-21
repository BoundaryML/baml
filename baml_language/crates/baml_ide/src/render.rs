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

use baml_base::{LangRoots, Name, SourceFile, SourceRoot};
use baml_compiler2_hir::{
    item_data::{FunctionData, GenericParamData},
    package::lang_roots,
    type_ref::{TypeRefId, TypeRefStore},
};
use baml_compiler2_hir_ty::render::Viewpoint;
use baml_type::{DeclName, Ty, TyRenderStrategy, TypeName, user_facing::humanize_type_string};

// ── Package labels ────────────────────────────────────────────────────────────

/// A package's display label for listings, exports, and generated SDKs, as
/// seen from `viewer` — the package the reader is in: the package whose
/// program a listing or a generated SDK describes, the package being
/// exported, the file a hover is answered for
/// ([`Viewpoint::display_package`]: its declared name, else the name
/// `viewer`'s nearest edge reaches it by, else the stand-in). A database may
/// hold many packages, so no output is rendered from a package the reader
/// is not in. Display only: the SDK boundary keys its symbols on it until it
/// refers to declarations by image identity.
pub fn package_label(
    db: &dyn baml_compiler2_hir::Db,
    viewer: SourceRoot,
    root: SourceRoot,
) -> Name {
    Viewpoint::user_facing(db, viewer).display_label(root)
}

/// The name `viewer`'s root namespace addresses `root` by, for a listing a
/// reader pastes back into `baml describe`: the viewer's edge name for it
/// where it has one — a dependency declared `foo` but depended on as `bar`
/// resolves only as `bar` — else its display label ([`package_label`]), which
/// is also the viewer's own.
pub fn package_address(
    db: &dyn baml_compiler2_hir::Db,
    viewer: SourceRoot,
    root: SourceRoot,
) -> Name {
    let viewpoint = Viewpoint::user_facing(db, viewer);
    match viewpoint.package_prefix(root) {
        Some(prefix) => Name::new(prefix.as_ref()),
        None => viewpoint.display_label(root),
    }
}

/// A compile-time head packed into a wire name for the symbol pool
/// ([`Viewpoint::wire_head`]): the viewer's own package `Local`, by identity,
/// every other package under [`package_label`]. Never an identity.
pub fn wire_head(db: &dyn baml_compiler2_hir::Db, viewer: SourceRoot, decl: &DeclName) -> TypeName {
    Viewpoint::user_facing(db, viewer).wire_head(decl)
}

/// A compile-time head with its package always spelled under
/// [`package_label`] ([`Viewpoint::labelled_head`]), for the export, whose
/// ids name every package. Never an identity.
pub fn labelled_head(
    db: &dyn baml_compiler2_hir::Db,
    viewer: SourceRoot,
    decl: &DeclName,
) -> TypeName {
    Viewpoint::user_facing(db, viewer).labelled_head(decl)
}

/// The wire package of `root` for a symbol pool keyed from `viewer`: `Local`
/// for the viewer itself, by identity, else a dependency under
/// [`package_label`] — the rule [`wire_head`] applies to a head.
pub fn pool_package(
    db: &dyn baml_compiler2_hir::Db,
    viewer: SourceRoot,
    root: SourceRoot,
) -> baml_type::Package {
    if root == viewer {
        baml_type::Package::Local
    } else {
        baml_type::Package::Dep(package_label(db, viewer, root))
    }
}

// ── Readers ──────────────────────────────────────────────────────────────────

/// Who is reading an IDE answer, and the one viewpoint every path in that
/// answer is spelled from: a file (hover — paths as source in that file
/// writes them, bare for its own namespace) or a package as a whole
/// (`baml describe` — the package's root namespace, where every item of the
/// package is `root.<ns>.<Name>` or bare). Hover and describe share the code
/// that assembles an answer; the reader is what they do not share.
pub struct Reader<'a> {
    db: &'a dyn baml_compiler2_hir::Db,
    viewpoint: Viewpoint<'a>,
    viewer: SourceRoot,
}

impl<'a> Reader<'a> {
    /// A reader at `file`: a hover's.
    pub fn of_file(db: &'a dyn baml_compiler2_hir::Db, file: SourceFile) -> Self {
        Self {
            db,
            viewpoint: Viewpoint::for_file(db, file),
            viewer: file.source_root(db),
        }
    }

    /// A reader addressing `viewer`'s package as a whole: `baml describe`'s.
    pub fn of_package(db: &'a dyn baml_compiler2_hir::Db, viewer: SourceRoot) -> Self {
        Self {
            db,
            viewpoint: Viewpoint::user_facing(db, viewer),
            viewer,
        }
    }

    /// The reader's package.
    pub fn viewer(&self) -> SourceRoot {
        self.viewer
    }

    /// `ty` as the reader writes it, declarations spelled in full
    /// (`baml.String`): the form of a diagnostic or a type line.
    pub fn display_ty(&self, ty: &Ty) -> String {
        ty.render_with(&IdeTyRender {
            viewpoint: &self.viewpoint,
            collapse_aliases: None,
        })
    }

    /// `ty` in the paste-back spelling: builtin companion classes as their
    /// alias (`string`, `image`, `json`), every other declaration as the
    /// reader writes it. The form of a describe row, a signature, an owner.
    pub fn display_addressable_ty(&self, ty: &Ty) -> String {
        ty.render_with(&IdeTyRender {
            viewpoint: &self.viewpoint,
            collapse_aliases: Some(lang_roots(self.db)),
        })
    }

    /// A declaration in the paste-back spelling ([`Self::display_addressable_ty`]
    /// for a head).
    pub fn addressable_path(&self, qtn: &DeclName) -> String {
        IdeTyRender {
            viewpoint: &self.viewpoint,
            collapse_aliases: Some(lang_roots(self.db)),
        }
        .path(qtn)
    }
}

// ── Resolved-type rendering ───────────────────────────────────────────────────

/// The IDE's naming policy over a [`Viewpoint`]: the viewpoint spells every
/// declaration — bare in the viewing namespace, `root.`-qualified elsewhere
/// in the viewer's package, the viewer's edge name for a dependency, every
/// package spelled when canonical — and this adds the two IDE-only choices:
/// whether a builtin companion class collapses to its lowercase alias
/// (`baml.String` → `string`, `baml.media.Image` → `image`, `baml.json.json`
/// → `json`), which describe, hover and signatures opt into, and `callback`
/// for a synthetic effect parameter in every mode. Implements
/// [`TyRenderStrategy`] so the structural walk lives once in `baml_type`.
struct IdeTyRender<'v, 'a> {
    viewpoint: &'v Viewpoint<'a>,
    /// Collapse builtin companion classes to their alias through these
    /// language roots; `None` keeps the declaration spelling.
    collapse_aliases: Option<LangRoots>,
}

impl IdeTyRender<'_, '_> {
    fn path(&self, qtn: &DeclName) -> String {
        if let Some(lang) = self.collapse_aliases
            && let Some(alias) = qtn.builtin_alias(lang)
        {
            return alias.to_string();
        }
        self.viewpoint.path(qtn)
    }
}

impl TyRenderStrategy<DeclName> for IdeTyRender<'_, '_> {
    fn qtn(&self, qtn: &DeclName) -> String {
        self.path(qtn)
    }

    fn type_var(&self, name: &Name) -> String {
        if baml_type::is_synthetic_effect_param(name) {
            "callback".to_string()
        } else {
            name.to_string()
        }
    }
}

/// The canonical path of a declaration: every package spelled as the
/// program spells it, nothing elided.
pub fn canonical_path(db: &dyn baml_compiler2_hir::Db, qtn: &DeclName) -> String {
    Viewpoint::canonical(db).path(qtn)
}

/// The addressable spelling of a declaration as `viewer`'s root namespace
/// writes it: the shortest form that pastes back into `baml describe`, which
/// resolves from there — a builtin companion's lowercase alias (`string`);
/// the viewer's own type by its bare name at package root, `root.<ns>.<Name>`
/// in a namespace (the viewer's package is addressed as `root`); any other
/// package's type by the path the viewer reaches it under
/// (`baml.json.JsonObject`). Source in another namespace of the package
/// writes the first form `root.`-qualified ([`addressable_path_for_file`]).
pub fn addressable_path(
    db: &dyn baml_compiler2_hir::Db,
    viewer: SourceRoot,
    qtn: &DeclName,
) -> String {
    Reader::of_package(db, viewer).addressable_path(qtn)
}

/// The addressable spelling of a declaration as source in `file` writes it
/// ([`addressable_path`] from the file's namespace): bare for a declaration
/// of that namespace, `root.<ns>.<Name>` elsewhere in its package, the
/// file's package's edge name for any other. The hover form for an owner
/// line, which the reader pastes back from where the cursor is.
pub fn addressable_path_for_file(
    db: &dyn baml_compiler2_hir::Db,
    file: SourceFile,
    qtn: &DeclName,
) -> String {
    IdeTyRender {
        viewpoint: &Viewpoint::for_file(db, file),
        collapse_aliases: Some(lang_roots(db)),
    }
    .path(qtn)
}

/// Render `ty` in `file`'s package/namespace context — the hover/completion
/// form: what source in that file writes.
pub fn display_ty_for_file(db: &dyn baml_compiler2_hir::Db, file: SourceFile, ty: &Ty) -> String {
    ty.render_with(&IdeTyRender {
        viewpoint: &Viewpoint::for_file(db, file),
        collapse_aliases: None,
    })
}

/// Render `ty` as a reader anywhere in `viewer`'s package writes it: the
/// package's own declarations bare at its root namespace and `root.ns.Name`
/// elsewhere, every other package by the viewer's edge name. The hover and
/// describe form for a declaration READ from `viewer` but defined elsewhere
/// (another package, another namespace), whose own file is not the reader's.
pub fn display_ty_for_viewer(
    db: &dyn baml_compiler2_hir::Db,
    viewer: SourceRoot,
    ty: &Ty,
) -> String {
    Reader::of_package(db, viewer).display_ty(ty)
}

/// Like [`display_ty_for_file`], but collapses builtin companion classes to
/// their lowercase primitive/keyword alias (`baml.String` → `string`,
/// `baml.media.Image` → `image`, `baml.json.json` → `json`). This is the
/// canonical type printer used by the describe + hover + signature paths;
/// other call sites (diagnostics, completions, inlay hints) keep the
/// un-collapsed [`display_ty_for_file`].
pub fn display_ty_canonical_for_file(
    db: &dyn baml_compiler2_hir::Db,
    file: SourceFile,
    ty: &Ty,
) -> String {
    ty.render_with(&IdeTyRender {
        viewpoint: &Viewpoint::for_file(db, file),
        collapse_aliases: Some(lang_roots(db)),
    })
}

/// Format a resolved [`Ty`] as a user-friendly string without file context.
///
/// Full canonical paths so same-short-name types stay distinguishable;
/// synthetic effect params show as `callback`. With file context available,
/// prefer [`display_ty_for_file`].
pub fn display_ty(db: &dyn baml_compiler2_hir::Db, ty: &Ty) -> String {
    ty.render_with(&IdeTyRender {
        viewpoint: &Viewpoint::canonical(db),
        collapse_aliases: None,
    })
}

/// Render `ty` for an owner line where no reader is looking (a symbol
/// listing): full canonical paths, with builtin companion classes collapsed
/// to their reader-facing alias (`baml.String` → `string`). Combined with
/// `class_self_ty`'s builtin bridging this spells a method's container the
/// way the reader writes the receiver: `T[]`, `map<K, V>`, `string`,
/// `user.util.Widget<T>`.
pub fn display_owner_ty(db: &dyn baml_compiler2_hir::Db, ty: &Ty) -> String {
    ty.render_with(&IdeTyRender {
        viewpoint: &Viewpoint::canonical(db),
        collapse_aliases: Some(lang_roots(db)),
    })
}

/// Render `ty` for a `baml describe` row read from `viewer`: every named type
/// in the paste-back spelling of [`addressable_path`] (`string`, `Foo`,
/// `root.ns.Foo`, `baml.json.JsonObject`), so a type a row names can be fed
/// straight back into `baml describe`.
pub fn display_addressable_ty(
    db: &dyn baml_compiler2_hir::Db,
    viewer: SourceRoot,
    ty: &Ty,
) -> String {
    Reader::of_package(db, viewer).display_addressable_ty(ty)
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
        K::Path { path, .. } => path
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
        K::Unknown => "unknown".to_string(),
        K::Never => "never".to_string(),
        K::Type => "type".to_string(),
        K::Rust => "$rust_type".to_string(),
        K::Infer => "_".to_string(),
        K::Error | K::Missing => "unknown".to_string(),
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
    fn render(&self, db: &dyn baml_compiler2_hir::Db, file: SourceFile, style: SigStyle) -> String {
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

    /// Signature parts for a method known only by its exported descriptor —
    /// a mounted or precompiled impl's method, which has no source item in
    /// this database. Every slot is resolved; a `self` receiver stays bare;
    /// synthetic callback effect parameters are elided like every other
    /// renderer does.
    pub fn of_exported(
        exported: &'db baml_compiler2_hir_ty::package_interface::ExportedFunction,
    ) -> FnSigParts<'db> {
        FnSigParts {
            name: exported.name.as_str().to_string(),
            generics: exported
                .generic_params
                .iter()
                .filter(|param| !baml_type::is_synthetic_effect_param(param.name()))
                .map(|param| param.name().to_string())
                .collect(),
            params: exported
                .params
                .iter()
                .map(|param| {
                    let name = param
                        .name
                        .as_ref()
                        .map_or_else(|| "_".to_string(), |name| name.as_str().to_string());
                    SigParam {
                        ty: (name != "self").then_some(SigSlot::Resolved(&param.ty)),
                        optional: param.is_optional(),
                        name,
                    }
                })
                .collect(),
            ret: SigSlot::Resolved(&exported.return_type),
            throws: SigSlot::Resolved(&exported.callable_throws),
        }
    }

    /// Render with the shared layout: `function name<G>(p: T, q?: U) -> R
    /// throws E`, or the bare `(p: T) -> R throws E` form per `style`.
    /// `file` anchors context-aware naming for [`SigSlot::Resolved`] slots.
    pub fn render(
        &self,
        db: &dyn baml_compiler2_hir::Db,
        file: SourceFile,
        style: SigStyle,
    ) -> String {
        self.render_with(style, |slot| slot.render(db, file, style))
    }

    /// [`Self::render`] for a signature READ from `viewer`'s package rather
    /// than from the file defining it — a served package's row, or a source
    /// item of another package or namespace shown in a hover or `describe`:
    /// every resolved slot spelled from the viewer's package
    /// ([`display_addressable_ty`] / [`display_ty_for_viewer`] per `style`).
    pub fn render_for_viewer(
        &self,
        db: &dyn baml_compiler2_hir::Db,
        viewer: SourceRoot,
        style: SigStyle,
    ) -> String {
        self.render_for(&Reader::of_package(db, viewer), style)
    }

    /// [`Self::render`] for `reader`: every resolved slot spelled as the
    /// reader writes it ([`Reader::display_addressable_ty`] /
    /// [`Reader::display_ty`] per `style`).
    pub fn render_for(&self, reader: &Reader<'_>, style: SigStyle) -> String {
        let display = |ty: &Ty| {
            if style.canonical_resolved {
                reader.display_addressable_ty(ty)
            } else {
                reader.display_ty(ty)
            }
        };
        self.render_with(style, |slot| match slot {
            SigSlot::Resolved(ty) => display(ty),
            SigSlot::ResolvedOwned(ty) => display(ty),
            SigSlot::Syntax(store, id) => style.type_form.render(store, *id),
            SigSlot::Missing => MISSING_RETURN.to_string(),
            SigSlot::Inferred => PENDING_INFERENCE.to_string(),
        })
    }

    fn render_with(
        &self,
        style: SigStyle,
        render_slot: impl Fn(&SigSlot<'db>) -> String,
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
                        format!("{}{}: {}", param.name, optional, render_slot(slot))
                    }
                    None => param.name.clone(),
                })
            })
            .collect::<Vec<_>>()
            .join(", ");
        let ret = format!(" -> {}", render_slot(&self.ret));
        let throws = format!(" throws {}", render_slot(&self.throws));
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

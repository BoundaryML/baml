//! Declaration lowering: type syntax -> interned [`Ty`], with name
//! resolution - the rust-analyzer `TyLoweringContext` analog (S4).
//!
//! ONE syntax surface: everything lowers through span-free `TypeRef`s -
//! signatures, fields, and aliases from ppir's per-item stores, body
//! annotations from ppir's per-body stores (`body_type_refs`). This crate
//! never sees `ast::TypeExpr`. Name
//! resolution mirrors TIR's `resolve_type_in` algorithm exactly - namespace-
//! relative first, then `root.`-absolute, then package-prefixed, then the
//! `$stream` companion fallback - against ppir's canonical `package_items`
//! (which includes synthesized `*$stream` items).
//!
//! Semantics mirrored from TIR's `lower_type_expr` (reference, not a
//! dependency): aliases stay NOMINAL at lowering (expansion is lazy and
//! cycle-guarded, done by consumers through the fact oracle); `T?` is sugar
//! for `T | null`; enum variants read as literal types via the
//! path-resolution fallback; generic arity is enforced by truncate/pad-with-
//! `Error`; `baml.future.Future<V, E>` lowers to the dedicated `Future`
//! kind. Diagnostics are not emitted yet (S17); every failure lowers to the
//! `Error` sentinel.
//!
//! Not yet mirrored (later slices): `Self` types (I6), associated-type
//! projections and interface associated-type defaults (I5), generic bounds
//! into a param env (I2), map-key validation and other diagnostics (S17).

use baml_compiler2_hir::{
    contributions::Definition,
    loc::{ClassLoc, FunctionLoc, TypeAliasLoc},
    package::PackageId,
    type_ref::{TypeRefId, TypeRefKind, TypeRefStore},
};
use baml_compiler2_ppir::item_data::MethodOwner;
use rustc_hash::FxHashMap;
use baml_type::{
    Freshness, Name, ParamTy, TyAttr, TypeName,
    interned::{FunctionParam, Ty, TyKind},
};

/// Everything needed to lower type syntax appearing in one file, for one
/// generic frame.
pub struct LowerCtx<'db> {
    db: &'db dyn baml_compiler2_ppir::Db,
    package_items: &'db baml_compiler2_hir::package::PackageItems<'db>,
    ns_context: Vec<Name>,
    /// The flattened generic frame, innermost params last (lookup searches
    /// in reverse so inner frames shadow outer ones).
    generic_params: Vec<ParamTy>,
    /// The frame's declared interface bounds (I2's param env): each
    /// param's CONJUNCTION. Projections (`T.Output`) determine their
    /// interface through these.
    bounds: FxHashMap<ParamTy, Vec<baml_type::interned::InterfaceRef>>,
}

/// A lowering context for type syntax written in `file`, with an empty
/// generic frame.
pub fn lower_ctx_for_file(
    db: &dyn baml_compiler2_ppir::Db,
    file: baml_base::SourceFile,
) -> LowerCtx<'_> {
    let info = baml_compiler2_hir::file_package::file_package(db, file);
    let package_items =
        baml_compiler2_ppir::package_items(db, PackageId::new(db, info.package.clone()));
    LowerCtx {
        db,
        package_items,
        ns_context: info.namespace_path,
        generic_params: Vec::new(),
        bounds: FxHashMap::default(),
    }
}

impl<'db> LowerCtx<'db> {
    #[must_use]
    pub fn with_frame(mut self, frame: Vec<ParamTy>) -> LowerCtx<'db> {
        self.generic_params = frame;
        self
    }

    #[must_use]
    pub fn with_bounds(
        mut self,
        bounds: FxHashMap<ParamTy, Vec<baml_type::interned::InterfaceRef>>,
    ) -> LowerCtx<'db> {
        self.bounds = bounds;
        self
    }

    // -- Span-free surface (signatures, fields, aliases) ----------------------

    pub fn lower_type_ref(&self, store: &TypeRefStore, id: TypeRefId) -> Ty {
        let attr = TyAttr::default;
        match &store[id].kind {
            TypeRefKind::Int => Ty::int(),
            TypeRefKind::Bigint => Ty::intern(TyKind::Bigint { attr: attr() }),
            TypeRefKind::Float => Ty::float(),
            TypeRefKind::String => Ty::string(),
            TypeRefKind::Bool => Ty::bool(),
            TypeRefKind::Null => Ty::null(),
            TypeRefKind::Never => Ty::never(),
            TypeRefKind::Void => Ty::void(),
            TypeRefKind::Uint8Array => Ty::intern(TyKind::Uint8Array { attr: attr() }),
            TypeRefKind::Media { kind } => Ty::intern(TyKind::Media(*kind, attr())),
            TypeRefKind::BuiltinUnknown => Ty::intern(TyKind::Unknown { attr: attr() }),
            TypeRefKind::Type => Ty::intern(TyKind::Type { attr: attr() }),
            TypeRefKind::Rust => Ty::intern(TyKind::RustType { attr: attr() }),
            TypeRefKind::Optional { inner } => {
                Ty::union([self.lower_type_ref(store, *inner), Ty::null()])
            }
            TypeRefKind::List { inner } => Ty::list(self.lower_type_ref(store, *inner)),
            TypeRefKind::Map { key, value } => Ty::intern(TyKind::Map {
                key: self.lower_type_ref(store, *key),
                value: self.lower_type_ref(store, *value),
                attr: attr(),
            }),
            TypeRefKind::Union { variants } => Ty::union(
                variants
                    .iter()
                    .map(|variant| self.lower_type_ref(store, *variant)),
            ),
            TypeRefKind::Literal { value } => {
                Ty::intern(TyKind::Literal(value.clone(), Freshness::Regular, attr()))
            }
            TypeRefKind::Function {
                params,
                ret,
                throws,
            } => Ty::intern(TyKind::Function {
                params: params
                    .iter()
                    .map(|param| FunctionParam {
                        name: param.name.clone(),
                        ty: self.lower_type_ref(store, param.ty),
                        mode: if param.optional {
                            baml_type::FunctionParamMode::Optional
                        } else {
                            baml_type::FunctionParamMode::Required
                        },
                    })
                    .collect(),
                ret: self.lower_type_ref(store, *ret),
                // Elaboration rewrites every legal omitted throws into a
                // synthetic effect param; a survivor here recovers as
                // `never`, mirroring TIR.
                throws: throws
                    .map(|throws| self.lower_type_ref(store, throws))
                    .unwrap_or_else(Ty::never),
                attr: attr(),
            }),
            TypeRefKind::Path {
                segments,
                generic_args,
                associated_type_bindings,
            } => {
                let args: Vec<Ty> = generic_args
                    .iter()
                    .map(|arg| self.lower_type_ref(store, *arg))
                    .collect();
                let bindings: Vec<(Name, Ty)> = associated_type_bindings
                    .iter()
                    .map(|binding| (binding.name.clone(), self.lower_type_ref(store, binding.ty)))
                    .collect();
                self.lower_path(segments, args, bindings)
            }
            // A projection node: `T.Output` / `(T as I).Output`. The
            // qualifying interface is the written one, or determined from
            // the base's bound conjunction (the unique bound declaring the
            // member). Reduction stays with the fact oracle (I5); lowering
            // just builds the node - the pure-lowering discipline.
            TypeRefKind::AssociatedTypeProjection {
                base,
                interface,
                member,
            } => {
                let base_ty = self.lower_type_ref(store, *base);
                let qualifier = match interface {
                    Some(interface) => match self.lower_type_ref(store, *interface).kind() {
                        TyKind::Interface(name, args, pins, _) => {
                            Some(baml_type::interned::InterfaceRef::new(
                                name.clone(),
                                args.to_vec().into_boxed_slice(),
                                pins.to_vec(),
                            ))
                        }
                        _ => None,
                    },
                    None => self.projection_interface_for(&base_ty, member),
                };
                match qualifier {
                    Some(interface) => Ty::intern(TyKind::AssociatedTypeProjection {
                        base: base_ty,
                        interface,
                        member: member.clone(),
                        attr: attr(),
                    }),
                    None => Ty::error(),
                }
            }
            // `_` lowers to the var-less hole node; consumers apply policy
            // (signatures reject holes, inference instantiates them) - the
            // rust-analyzer pure-lowering + funnel discipline.
            TypeRefKind::Infer => Ty::intern(TyKind::Infer {
                var: None,
                attr: attr(),
            }),
            // `Unknown` is an omitted annotation (a signature must be
            // explicit; the diagnostic arrives with S17), `Error` was
            // already diagnosed at parse time.
            TypeRefKind::Error | TypeRefKind::Unknown => Ty::error(),
        }
    }

    /// The interface a bare projection (`T.Output`) resolves through: for
    /// a type-var base, the UNIQUE bound in its conjunction whose
    /// interface declares `member` (0 or ambiguity is Error - S17's
    /// diagnostic); for an interface-existential base, itself.
    fn projection_interface_for(
        &self,
        base: &Ty,
        member: &Name,
    ) -> Option<baml_type::interned::InterfaceRef> {
        let declares = |name: &TypeName| -> bool {
            let def = self
                .package_items
                .lookup_type(name.namespace(), name.name())
                .or_else(|| {
                    let package = baml_compiler2_hir::package::PackageId::new(
                        self.db,
                        name.package().clone(),
                    );
                    baml_compiler2_ppir::package_items(self.db, package)
                        .lookup_type(name.namespace(), name.name())
                });
            match def {
                Some(Definition::Interface(interface)) => {
                    baml_compiler2_ppir::item_data::interface_data(self.db, interface)
                        .associated_types
                        .iter()
                        .any(|assoc| assoc.name == *member)
                }
                _ => false,
            }
        };
        match base.kind() {
            TyKind::TypeVar(param, _) => {
                let candidates: Vec<&baml_type::interned::InterfaceRef> = self
                    .bounds
                    .get(param)
                    .map(|bounds| bounds.iter().filter(|b| declares(&b.name)).collect())
                    .unwrap_or_default();
                match candidates.as_slice() {
                    [only] => Some((*only).clone()),
                    _ => None,
                }
            }
            TyKind::Interface(name, args, pins, _) => Some(
                baml_type::interned::InterfaceRef::new(
                    name.clone(),
                    args.to_vec().into_boxed_slice(),
                    pins.to_vec(),
                ),
            ),
            _ => None,
        }
    }

    // -- Path resolution -------------------------------------------------------

    /// Lowers a resolved (or fallback-resolved) type path. Mirrors TIR's
    /// `lower_path` dispatch and its failure fallbacks: in-scope type var,
    /// then enum-variant reading, then `Error`.
    fn lower_path(&self, segments: &[Name], args: Vec<Ty>, bindings: Vec<(Name, Ty)>) -> Ty {
        let attr = TyAttr::default;
        let short = segments.last().expect("type paths are never empty");

        // `Self` types arrive with I6.
        if segments[0].as_str() == "Self" {
            return Ty::error();
        }

        if let Some(def) = self.resolve_type(segments) {
            return self.lower_definition(def, short, args, bindings);
        }

        // Fallback 1: a single segment naming an in-scope generic param
        // (inner frames shadow outer: search in reverse).
        if segments.len() == 1
            && let Some(param) = self
                .generic_params
                .iter()
                .rev()
                .find(|param| param.name() == &segments[0])
        {
            return Ty::intern(TyKind::TypeVar(param.clone(), attr()));
        }

        // Fallback 2: `Enum.Variant` read as a literal type.
        if segments.len() >= 2
            && args.is_empty()
            && bindings.is_empty()
            && let Some(Definition::Enum(enum_loc)) =
                self.resolve_type(&segments[..segments.len() - 1])
        {
            let enum_data = baml_compiler2_ppir::item_data::enum_data(self.db, enum_loc);
            if enum_data
                .variants
                .iter()
                .any(|variant| &variant.name == short)
            {
                let enum_short = &segments[segments.len() - 2];
                return Ty::intern(TyKind::EnumVariant(
                    self.qualify(Definition::Enum(enum_loc), enum_short),
                    short.clone(),
                    attr(),
                ));
            }
        }

        // Fallback 3 (associated projections) arrives with I5.
        Ty::error()
    }

    fn lower_definition(
        &self,
        def: Definition<'db>,
        short: &Name,
        mut args: Vec<Ty>,
        mut bindings: Vec<(Name, Ty)>,
    ) -> Ty {
        let attr = TyAttr::default;
        match def {
            Definition::Class(class_loc) => {
                let data = baml_compiler2_ppir::item_data::class_data(self.db, class_loc);
                enforce_arity(&mut args, data.generic_params.len());
                let qtn = self.qualify(def, short);
                class_ty(qtn, args)
            }
            Definition::Interface(interface_loc) => {
                let data = baml_compiler2_ppir::item_data::interface_data(self.db, interface_loc);
                enforce_arity(&mut args, data.generic_params.len());
                // Written bindings only; defaults and completeness checking
                // arrive with I5. Sorted for order-insensitive identity.
                bindings.sort_by(|(a, _), (b, _)| a.cmp(b));
                Ty::intern(TyKind::Interface(
                    self.qualify(def, short),
                    args.into(),
                    bindings.into(),
                    attr(),
                ))
            }
            Definition::Enum(_) => Ty::intern(TyKind::Enum(self.qualify(def, short), attr())),
            // Aliases stay nominal at lowering; expansion is lazy and
            // cycle-guarded, through the fact oracle.
            Definition::TypeAlias(_) => {
                Ty::intern(TyKind::TypeAlias(self.qualify(def, short), attr()))
            }
            // Value-namespace definitions are not types.
            Definition::Function(_)
            | Definition::TemplateString(_)
            | Definition::Client(_)
            | Definition::Test(_)
            | Definition::RetryPolicy(_)
            | Definition::Let(_) => Ty::error(),
        }
    }

    /// TIR's `resolve_type_in`, mirrored: (1) namespace-relative in the
    /// current package (no outward walk); (2) `root.`-absolute or
    /// package-prefixed; (3) the `$stream` companion fallback.
    fn resolve_type(&self, segments: &[Name]) -> Option<Definition<'db>> {
        let (item, seg_ns) = segments.split_last().expect("type paths are never empty");

        let relative_ns: Vec<Name> = if self.ns_context.is_empty() {
            seg_ns.to_vec()
        } else {
            self.ns_context.iter().chain(seg_ns).cloned().collect()
        };
        if let Some(def) = self.package_items.lookup_type(&relative_ns, item) {
            return Some(def);
        }

        if segments.len() >= 2 {
            let prefix_ns = &segments[1..segments.len() - 1];
            if segments[0].as_str() == "root" {
                if let Some(def) = self.package_items.lookup_type(prefix_ns, item) {
                    return Some(def);
                }
            } else {
                let dep_items = baml_compiler2_ppir::package_items(
                    self.db,
                    PackageId::new(self.db, segments[0].clone()),
                );
                if let Some(def) = dep_items.lookup_type(prefix_ns, item) {
                    return Some(def);
                }
            }
        }

        // `$stream` companions of classes/aliases resolve through their base
        // name; the caller re-qualifies under the `$stream` name.
        if let Some(base) = item.as_str().strip_suffix("$stream") {
            let mut base_segments = segments.to_vec();
            *base_segments.last_mut().expect("non-empty") = Name::new(base);
            return self
                .resolve_type(&base_segments)
                .filter(|def| matches!(def, Definition::Class(_) | Definition::TypeAlias(_)));
        }

        None
    }

    /// Value-namespace resolution, mirroring [`LowerCtx::resolve_type`]'s
    /// algorithm over `lookup_value` (functions, clients, lets). No
    /// `$stream` fallback: companions are functions with their own names.
    pub fn resolve_value(&self, segments: &[Name]) -> Option<Definition<'db>> {
        let (item, seg_ns) = segments.split_last()?;
        let relative_ns: Vec<Name> = if self.ns_context.is_empty() {
            seg_ns.to_vec()
        } else {
            self.ns_context.iter().chain(seg_ns).cloned().collect()
        };
        if let Some(def) = self.package_items.lookup_value(&relative_ns, item) {
            return Some(def);
        }
        if segments.len() >= 2 {
            let prefix_ns = &segments[1..segments.len() - 1];
            if segments[0].as_str() == "root" {
                if let Some(def) = self.package_items.lookup_value(prefix_ns, item) {
                    return Some(def);
                }
            } else {
                let dep_items = baml_compiler2_ppir::package_items(
                    self.db,
                    PackageId::new(self.db, segments[0].clone()),
                );
                if let Some(def) = dep_items.lookup_value(prefix_ns, item) {
                    return Some(def);
                }
            }
        }
        None
    }

    /// Type-namespace resolution, exposed for constructor and member typing.
    pub fn resolve_type_definition(&self, segments: &[Name]) -> Option<Definition<'db>> {
        self.resolve_type(segments)
    }

    /// [`LowerCtx::qualify`], exposed for constructor typing.
    pub fn qualify_definition(&self, def: Definition<'db>, short: &Name) -> TypeName {
        self.qualify(def, short)
    }

    /// TIR's `qualify_def`: the qualified name comes from the DEFINITION's
    /// file, while the short name is what the user wrote (which is what
    /// keeps `$stream` companions distinct from their base).
    fn qualify(&self, def: Definition<'db>, short: &Name) -> TypeName {
        let info = baml_compiler2_hir::file_package::file_package(self.db, def.file(self.db));
        TypeName::new(info.package, info.namespace_path, short.clone())
    }
}

/// Substitutes generic parameters by FRAME INDEX: `TypeVar(p)` becomes
/// `args[p.index()]` (absent slots stay symbolic). The instantiation fold
/// for call sites and constructors; flag-short-circuited.
pub fn substitute_params(ty: &baml_type::interned::Ty, args: &[baml_type::interned::Ty]) -> Ty {
    use baml_type::interned::TypeFlags;
    if !ty.flags().contains(TypeFlags::HAS_TYPEVAR) {
        return ty.clone();
    }
    if let TyKind::TypeVar(param, _) = ty.kind()
        && let Some(replacement) = args.get(param.index() as usize)
    {
        return replacement.clone();
    }
    Ty::intern(
        ty.kind()
            .map_children(|child| substitute_params(child, args)),
    )
}

/// Generic-arity recovery without diagnostics (S17): extras truncated,
/// missing padded with `Error`.
fn enforce_arity(args: &mut Vec<Ty>, expected: usize) {
    args.truncate(expected);
    while args.len() < expected {
        args.push(Ty::error());
    }
}

// -- Generic frames -----------------------------------------------------------

/// The flattened generic frame for a function: owner generics first (class
/// generics; interfaces prepend `Self` and append associated-type names,
/// mirroring TIR's layout), then the function's own generics, then its
/// synthetic effect params. Indices are absolute frame positions.
pub fn function_generic_frame<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    function: FunctionLoc<'db>,
) -> Vec<ParamTy> {
    let mut frame = Vec::new();
    match baml_compiler2_ppir::item_data::method_owner(db, function) {
        Some(MethodOwner::Class(class_loc)) => {
            let data = baml_compiler2_ppir::item_data::class_data(db, class_loc);
            extend_frame(&mut frame, &data.generic_params);
        }
        Some(MethodOwner::Interface(interface_loc)) => {
            let data = baml_compiler2_ppir::item_data::interface_data(db, interface_loc);
            extend_frame(&mut frame, &[Name::new("Self")]);
            extend_frame(&mut frame, &data.generic_params);
            let associated: Vec<Name> = data
                .associated_types
                .iter()
                .map(|assoc| assoc.name.clone())
                .collect();
            extend_frame(&mut frame, &associated);
        }
        // Free-impl generic frames arrive with the interface slices (I3/I4).
        Some(MethodOwner::FreeImpl(_)) | None => {}
    }
    let data = baml_compiler2_ppir::item_data::elaborated_function_data(db, function);
    extend_frame(&mut frame, &data.user_generic_params);
    extend_frame(&mut frame, &data.synthetic_effect_params);
    frame
}

/// The type a class reference denotes, with the builtin bridgings applied
/// uniformly: `baml.future.Future<V, E>` is the dedicated Future kind, and
/// (B-1080) the builtin `baml.Array<T>` / `baml.Map<K, V>` class spellings
/// ARE the structural types - lowering them to `List`/`Map` makes every
/// algebra arm relate them for free, instead of TIR's one-directional
/// argument-path patch. Keyed on the builtin package specifically: a
/// user-defined `class Array<T>` stays nominal. The single constructor for
/// class types, shared by annotation lowering and `class_self_ty`.
pub fn class_ty(qtn: TypeName, mut args: Vec<Ty>) -> Ty {
    let attr = TyAttr::default;
    if !qtn.is_local() && qtn.package().as_str() == "baml" {
        if qtn.namespace().len() == 1
            && qtn.namespace()[0].as_str() == "future"
            && qtn.name().as_str() == "Future"
            && args.len() == 2
        {
            let error_ty = args.pop().expect("checked len");
            let value_ty = args.pop().expect("checked len");
            return Ty::intern(TyKind::Future(value_ty, error_ty, attr()));
        }
        if qtn.namespace().is_empty() {
            if qtn.name().as_str() == "Array" && args.len() == 1 {
                let element = args.pop().expect("checked len");
                return Ty::intern(TyKind::List(element, attr()));
            }
            if qtn.name().as_str() == "Map" && args.len() == 2 {
                let value = args.pop().expect("checked len");
                let key = args.pop().expect("checked len");
                return Ty::intern(TyKind::Map {
                    key,
                    value,
                    attr: attr(),
                });
            }
        }
    }
    Ty::intern(TyKind::Class(qtn, args.into(), attr()))
}

/// The qualified name a class definition contributes, from its file's
/// package - the definition-side counterpart of `LowerCtx::qualify`.
pub fn class_qualified_name<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    class: ClassLoc<'db>,
) -> TypeName {
    let package = baml_compiler2_hir::file_package::file_package(db, class.file(db));
    TypeName::new(
        package.package.clone(),
        package.namespace_path,
        baml_compiler2_ppir::item_data::class_data(db, class).name.clone(),
    )
}

/// The type of `self` inside `class`: the class applied to its own generic
/// params as `TypeVar`s, through the same builtin bridging as written
/// annotations - so `self` in `baml.Array<T>` is `T[]`, and substituting
/// the receiver's args yields e.g. `int[]` with no per-class special case.
pub fn class_self_ty<'db>(db: &'db dyn baml_compiler2_ppir::Db, class: ClassLoc<'db>) -> Ty {
    let args: Vec<Ty> = class_generic_frame(db, class)
        .into_iter()
        .map(|param| Ty::intern(TyKind::TypeVar(param, TyAttr::default())))
        .collect();
    class_ty(class_qualified_name(db, class), args)
}

/// A generic frame from bare interface param names (no `Self` slot -
/// the registry's `requires` lowering; the full interface frame with
/// `Self` at index 0 stays `function_generic_frame`'s).
pub fn interface_generic_frame_params(names: &[Name]) -> Vec<ParamTy> {
    let mut frame = Vec::new();
    extend_frame(&mut frame, names);
    frame
}

/// The root generic frame for a class.
pub fn class_generic_frame<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    class: ClassLoc<'db>,
) -> Vec<ParamTy> {
    let mut frame = Vec::new();
    extend_frame(
        &mut frame,
        &baml_compiler2_ppir::item_data::class_data(db, class).generic_params,
    );
    frame
}

fn extend_frame(frame: &mut Vec<ParamTy>, names: &[Name]) {
    for name in names {
        let index = u32::try_from(frame.len()).expect("generic frame index overflow");
        frame.push(ParamTy::new(index, name.clone()));
    }
}

// -- Item queries -------------------------------------------------------------

/// A function's elaborated signature, lowered. Becomes a salsa query with
/// the S3 incremental work.
#[derive(Debug, Clone, PartialEq)]
pub struct FunctionSignature {
    pub generic_params: Vec<ParamTy>,
    pub params: Vec<SignatureParam>,
    pub ret: Ty,
    /// The declared clause when written, else the INFERRED effect via
    /// `callable_throws` (S12) - body-derived, fixpoint over mutual
    /// recursion, `never` when nothing throws.
    pub throws: Ty,
    /// Whether `throws` was written. The owner's own inference checks its
    /// throw sites against a DECLARED clause (the contract) and ignores an
    /// inferred one (which is derived FROM those sites).
    pub throws_declared: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SignatureParam {
    pub name: Name,
    pub ty: Ty,
    pub has_default: bool,
}

/// Ruling 4: `_` never infers in declaration signatures. Lowering emits the
/// hole node uniformly; this is the signature-side policy fold that rejects
/// survivors (the inference side instead instantiates holes as fresh vars).
pub fn reject_holes(ty: &Ty) -> Ty {
    if !ty.has_infer() {
        return ty.clone();
    }
    if matches!(ty.kind(), TyKind::Infer { var: None, .. }) {
        return Ty::error();
    }
    Ty::intern(ty.kind().map_children(reject_holes))
}

/// The declared interface bounds for a function's full generic frame
/// (class prefix + own params; effect params unbounded), keyed by the
/// same `ParamTy` identities `function_generic_frame` assigns.
pub fn function_generic_bounds<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    function: FunctionLoc<'db>,
) -> FxHashMap<ParamTy, Vec<baml_type::interned::InterfaceRef>> {
    let frame = function_generic_frame(db, function);
    let ctx = lower_ctx_for_file(db, function.file(db)).with_frame(frame.clone());
    let mut out = FxHashMap::default();
    let as_ref = |ty: &Ty| match ty.kind() {
        TyKind::Interface(name, args, pins, _) => Some(baml_type::interned::InterfaceRef::new(
            name.clone(),
            args.to_vec().into_boxed_slice(),
            pins.to_vec(),
        )),
        _ => None,
    };
    let mut frame_iter = frame.iter();
    match baml_compiler2_ppir::item_data::method_owner(db, function) {
        Some(MethodOwner::Class(class)) => {
            let class_data = baml_compiler2_ppir::item_data::class_data(db, class);
            for bound in &class_data.generic_param_bounds {
                let param = frame_iter.next();
                if let (Some(param), Some(type_ref)) = (param, bound)
                    && let Some(bound) =
                        as_ref(&ctx.lower_type_ref(&class_data.type_refs, *type_ref))
                {
                    out.insert(param.clone(), vec![bound]);
                }
            }
        }
        Some(MethodOwner::Interface(interface)) => {
            let data = baml_compiler2_ppir::item_data::interface_data(db, interface);
            // `Self` (frame slot 0) is bounded by the interface itself at
            // its own params.
            if let Some(self_param) = frame_iter.next() {
                let args: Vec<Ty> = data
                    .generic_params
                    .iter()
                    .enumerate()
                    .map(|(index, name)| {
                        Ty::intern(TyKind::TypeVar(
                            ParamTy::new(
                                u32::try_from(index + 1).expect("frame index overflow"),
                                name.clone(),
                            ),
                            TyAttr::default(),
                        ))
                    })
                    .collect();
                out.insert(
                    self_param.clone(),
                    vec![baml_type::interned::InterfaceRef::new(
                        interface_qualified_name(db, interface),
                        args.into_boxed_slice(),
                        Vec::new(),
                    )],
                );
            }
            for bound in &data.generic_param_bounds {
                let param = frame_iter.next();
                if let (Some(param), Some(type_ref)) = (param, bound)
                    && let Some(bound) = as_ref(&ctx.lower_type_ref(&data.type_refs, *type_ref))
                {
                    out.insert(param.clone(), vec![bound]);
                }
            }
            // The associated-type frame slots carry their declared bounds
            // when written (I5 consumes them for projections).
            for assoc in &data.associated_types {
                let param = frame_iter.next();
                if let (Some(param), Some(type_ref)) = (param, assoc.bound)
                    && let Some(bound) = as_ref(&ctx.lower_type_ref(&data.type_refs, type_ref))
                {
                    out.insert(param.clone(), vec![bound]);
                }
            }
        }
        Some(MethodOwner::FreeImpl(_)) | None => {}
    }
    let data = baml_compiler2_ppir::item_data::function_data(db, function);
    for bound in &data.generic_param_bounds {
        let param = frame_iter.next();
        if let (Some(param), Some(type_ref)) = (param, bound)
            && let Some(bound) = as_ref(&ctx.lower_type_ref(&data.type_refs, *type_ref))
        {
            out.insert(param.clone(), vec![bound]);
        }
    }
    out
}

/// The qualified name an interface definition contributes.
pub fn interface_qualified_name<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    interface: baml_compiler2_hir::loc::InterfaceLoc<'db>,
) -> TypeName {
    let package = baml_compiler2_hir::file_package::file_package(db, interface.file(db));
    TypeName::new(
        package.package,
        package.namespace_path,
        baml_compiler2_ppir::item_data::interface_data(db, interface)
            .name
            .clone(),
    )
}

pub fn function_signature<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    function: FunctionLoc<'db>,
) -> FunctionSignature {
    let data = baml_compiler2_ppir::item_data::elaborated_function_data(db, function);
    let frame = function_generic_frame(db, function);
    let bounds = function_generic_bounds(db, function);
    let ctx = lower_ctx_for_file(db, function.file(db))
        .with_frame(frame.clone())
        .with_bounds(bounds);
    // An unannotated `self` (elaboration leaves its slot `Unknown`) is
    // typed by the owner: the class's self type (through the builtin
    // bridging, so Array's `self` is `T[]`), or the interface's `Self`
    // type variable (frame position 0 by construction). Free-impl `self`
    // is the for-target - it arrives with the impl frames (I3).
    let owner = baml_compiler2_ppir::item_data::method_owner(db, function);
    let self_ty = |param: &baml_compiler2_ppir::item_data::ElaboratedParamData| {
        if param.name.as_str() != "self"
            || !matches!(data.type_refs[param.type_ref].kind, TypeRefKind::Unknown)
        {
            return None;
        }
        match owner {
            Some(MethodOwner::Class(class)) => Some(class_self_ty(db, class)),
            Some(MethodOwner::Interface(_)) => Some(Ty::intern(TyKind::TypeVar(
                frame.first().cloned().expect("interface frame starts with Self"),
                TyAttr::default(),
            ))),
            Some(MethodOwner::FreeImpl(_)) => Some(Ty::error()),
            None => None,
        }
    };
    let params = data
        .params
        .iter()
        .map(|param| SignatureParam {
            name: param.name.clone(),
            ty: self_ty(param)
                .unwrap_or_else(|| reject_holes(&ctx.lower_type_ref(&data.type_refs, param.type_ref))),
            has_default: param.has_default,
        })
        .collect();
    let ret = data
        .return_type
        .map(|ret| reject_holes(&ctx.lower_type_ref(&data.type_refs, ret)))
        .unwrap_or_else(Ty::error);
    let throws_declared = data.throws.is_some();
    let throws = data
        .throws
        .map(|throws| reject_holes(&ctx.lower_type_ref(&data.type_refs, throws)))
        .unwrap_or_else(|| {
            // Omitted: the body-inferred effect, fixpoint over mutual
            // recursion (S12's callable_throws).
            Ty::from_plain(&crate::callable::callable_throws(db, function).0)
        });
    FunctionSignature {
        generic_params: frame,
        params,
        ret,
        throws,
        throws_declared,
    }
}

/// A class's field types, lowered in the class's own generic frame.
pub fn class_field_types<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    class: ClassLoc<'db>,
) -> Vec<(Name, Ty)> {
    let data = baml_compiler2_ppir::item_data::class_data(db, class);
    let ctx = lower_ctx_for_file(db, class.file(db)).with_frame(class_generic_frame(db, class));
    data.fields
        .iter()
        .map(|field| {
            (
                field.name.clone(),
                reject_holes(&ctx.lower_type_ref(&data.type_refs, field.type_ref)),
            )
        })
        .collect()
}

/// A type alias's right-hand side, lowered (aliases are non-generic).
pub fn type_alias_value<'db>(db: &'db dyn baml_compiler2_ppir::Db, alias: TypeAliasLoc<'db>) -> Ty {
    let data = baml_compiler2_ppir::item_data::type_alias_data(db, alias);
    let ctx = lower_ctx_for_file(db, alias.file(db));
    data.value
        .map(|value| reject_holes(&ctx.lower_type_ref(&data.type_refs, value)))
        .unwrap_or_else(Ty::error)
}

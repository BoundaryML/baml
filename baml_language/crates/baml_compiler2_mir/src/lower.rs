use std::collections::{HashMap, HashSet};

use baml_base::{Name, TypePath};
use baml_type::{Ty, TyAttr, TyTemplate, TypeName};
use indexmap::IndexMap;

use crate::{
    builder::MirBuilder,
    ir::{
        AggregateKind, BasicBlock, BinOp, BlockId, CatchRegion, Constant, IndexKind, IntrinsicOp,
        ItemRef, Local, LocalDecl, LogLevel, MirFunction, MirFunctionBody, MirFunctionKind,
        Operand, Place, Rvalue, StatementKind, Terminator,
    },
    optimize,
};

/// Classifies what kind of switch a match/catch expression lowers to.
///
/// `Integer` and `EnumDiscriminant` are currently implemented.
/// `TypeTag` dispatches class-type and primitive-type match arms via runtime
/// type tags, using `Rvalue::TypeTag` for the switch operand.
enum SwitchKind {
    Integer,
    EnumDiscriminant(QualifiedTypeName),
    TypeTag,
}

/// What happens in the otherwise block of a switch.
#[derive(Clone, Copy)]
enum SwitchOtherwise {
    /// Match expression: goto join (non-exhaustive) or unreachable (exhaustive).
    Match { is_exhaustive: bool },
    /// Catch expression: rethrow unmatched errors.
    /// If `needs_throw_if_panic` is true, insert a `throw_if_panic` guard before wildcard body.
    Catch {
        error_local: Local,
        needs_throw_if_panic: bool,
    },
}

struct LoopContext {
    break_target: BlockId,
    continue_target: BlockId,
    watched_locals_depth: usize,
}

struct CatchContext {
    unwind_target: BlockId,
    error_local: Local,
}

// ─── Type conversion: TIR Ty → baml_type::Ty ────────────────────────────────

use baml_compiler2_tir::ty::{
    FunctionParamMode, FunctionParamTy as Tir2FunctionParamTy, QualifiedTypeName, Ty as Tir2Ty,
};

/// Pre-computed type alias data for inline expansion in `convert_tir2_ty`.
///
/// Bundles the alias map and recursion info that are always passed together.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ResolvedAliases {
    pub aliases: HashMap<QualifiedTypeName, Tir2Ty>,
    pub recursive: HashSet<QualifiedTypeName>,
}

impl ResolvedAliases {
    /// Build resolved aliases for a package, including dependency packages.
    pub fn for_package(db: &dyn crate::Db, pkg_id: baml_compiler2_hir::package::PackageId) -> Self {
        use baml_compiler2_hir::package::{package_dependencies, package_items};

        let pkg_items = package_items(db, pkg_id);
        let mut aliases = baml_compiler2_tir::inference::collect_type_aliases(db, pkg_items);
        for &dep_id in package_dependencies(db, pkg_id) {
            let dep_items = package_items(db, dep_id);
            aliases.extend(baml_compiler2_tir::inference::collect_type_aliases(
                db, dep_items,
            ));
        }
        let recursive = baml_compiler2_tir::normalize::find_recursive_aliases(&aliases);
        Self { aliases, recursive }
    }

    /// Convert a TIR type to `baml_type::Ty` using the cached alias data.
    pub fn convert(&self, ty: &Tir2Ty) -> Ty {
        convert_tir2_ty(ty, self)
    }
}

fn interface_tir_type_args_match_preserving_typevars(
    impl_iface_args: &[Tir2Ty],
    iface_type_args: &[Tir2Ty],
    aliases: &HashMap<QualifiedTypeName, Tir2Ty>,
) -> bool {
    impl_iface_args.len() == iface_type_args.len()
        && impl_iface_args
            .iter()
            .zip(iface_type_args.iter())
            .all(|(impl_arg, iface_arg)| {
                // A requested type-var (e.g. dispatching `b.get()` where
                // `b: Box<T>` inside `fn read<T>(..)`) is unconstrained at this
                // site — it matches any implementor instantiation, and the
                // runtime `IsType` guard on the concrete instance discriminates.
                (matches!(iface_arg, Tir2Ty::TypeVar(_, _))
                    && !matches!(impl_arg, Tir2Ty::TypeVar(_, _)))
                    || baml_compiler2_tir::normalize::is_same_normalized_type(
                        impl_arg, iface_arg, aliases,
                    )
            })
}

fn tir_type_satisfies_dispatch_request(
    actual: &Tir2Ty,
    requested: &Tir2Ty,
    aliases: &HashMap<QualifiedTypeName, Tir2Ty>,
) -> bool {
    if baml_compiler2_tir::normalize::is_same_normalized_type(actual, requested, aliases) {
        return true;
    }

    match (actual, requested) {
        (_, Tir2Ty::TypeVar(_, _) | Tir2Ty::AssociatedTypeProjection { .. }) => true,
        (Tir2Ty::List(actual, _), Tir2Ty::List(requested, _))
        | (Tir2Ty::EvolvingList(actual, _), Tir2Ty::EvolvingList(requested, _)) => {
            tir_type_satisfies_dispatch_request(actual, requested, aliases)
        }
        (
            Tir2Ty::Map {
                key: actual_key,
                value: actual_value,
                ..
            },
            Tir2Ty::Map {
                key: requested_key,
                value: requested_value,
                ..
            },
        )
        | (
            Tir2Ty::EvolvingMap(actual_key, actual_value, _),
            Tir2Ty::EvolvingMap(requested_key, requested_value, _),
        ) => {
            tir_type_satisfies_dispatch_request(actual_key, requested_key, aliases)
                && tir_type_satisfies_dispatch_request(actual_value, requested_value, aliases)
        }
        (
            Tir2Ty::Future(actual_value, actual_error, _),
            Tir2Ty::Future(requested_value, requested_error, _),
        ) => {
            tir_type_satisfies_dispatch_request(actual_value, requested_value, aliases)
                && tir_type_satisfies_dispatch_request(actual_error, requested_error, aliases)
        }
        (Tir2Ty::Union(actual, _), Tir2Ty::Union(requested, _))
            if actual.len() == requested.len() =>
        {
            actual
                .iter()
                .zip(requested.iter())
                .all(|(actual, requested)| {
                    tir_type_satisfies_dispatch_request(actual, requested, aliases)
                })
        }
        (
            Tir2Ty::Class(actual_qtn, actual_args, _),
            Tir2Ty::Class(requested_qtn, requested_args, _),
        ) if actual_qtn == requested_qtn && actual_args.len() == requested_args.len() => {
            actual_args
                .iter()
                .zip(requested_args.iter())
                .all(|(actual, requested)| {
                    tir_type_satisfies_dispatch_request(actual, requested, aliases)
                })
        }
        (
            Tir2Ty::Interface(actual_qtn, actual_args, actual_assoc, _),
            Tir2Ty::Interface(requested_qtn, requested_args, requested_assoc, _),
        ) if actual_qtn == requested_qtn && actual_args.len() == requested_args.len() => {
            actual_args
                .iter()
                .zip(requested_args.iter())
                .all(|(actual, requested)| {
                    tir_type_satisfies_dispatch_request(actual, requested, aliases)
                })
                && requested_assoc
                    .iter()
                    .all(|(requested_name, requested_ty)| {
                        actual_assoc
                            .iter()
                            .find(|(actual_name, _)| actual_name == requested_name)
                            .is_some_and(|(_, actual_ty)| {
                                tir_type_satisfies_dispatch_request(
                                    actual_ty,
                                    requested_ty,
                                    aliases,
                                )
                            })
                    })
        }
        (
            Tir2Ty::Function {
                params: actual_params,
                ret: actual_ret,
                throws: actual_throws,
                ..
            },
            Tir2Ty::Function {
                params: requested_params,
                ret: requested_ret,
                throws: requested_throws,
                ..
            },
        ) if actual_params.len() == requested_params.len()
            && actual_params
                .iter()
                .zip(requested_params.iter())
                .all(|(actual, requested)| actual.mode == requested.mode) =>
        {
            actual_params
                .iter()
                .zip(requested_params.iter())
                .all(|(actual, requested)| {
                    tir_type_satisfies_dispatch_request(&actual.ty, &requested.ty, aliases)
                })
                && tir_type_satisfies_dispatch_request(actual_ret, requested_ret, aliases)
                && tir_type_satisfies_dispatch_request(actual_throws, requested_throws, aliases)
        }
        _ => false,
    }
}

fn rewrite_dispatch_request_ty(actual: &Tir2Ty, requested: &Tir2Ty) -> Tir2Ty {
    match (actual, requested) {
        (_, Tir2Ty::TypeVar(_, _) | Tir2Ty::AssociatedTypeProjection { .. }) => actual.clone(),
        (Tir2Ty::List(actual, _), Tir2Ty::List(requested, attr)) => Tir2Ty::List(
            Box::new(rewrite_dispatch_request_ty(actual, requested)),
            attr.clone(),
        ),
        (Tir2Ty::EvolvingList(actual, _), Tir2Ty::EvolvingList(requested, attr)) => {
            Tir2Ty::EvolvingList(
                Box::new(rewrite_dispatch_request_ty(actual, requested)),
                attr.clone(),
            )
        }
        (
            Tir2Ty::Map {
                key: actual_key,
                value: actual_value,
                ..
            },
            Tir2Ty::Map {
                key: requested_key,
                value: requested_value,
                attr,
            },
        ) => Tir2Ty::Map {
            key: Box::new(rewrite_dispatch_request_ty(actual_key, requested_key)),
            value: Box::new(rewrite_dispatch_request_ty(actual_value, requested_value)),
            attr: attr.clone(),
        },
        (
            Tir2Ty::EvolvingMap(actual_key, actual_value, _),
            Tir2Ty::EvolvingMap(requested_key, requested_value, attr),
        ) => Tir2Ty::EvolvingMap(
            Box::new(rewrite_dispatch_request_ty(actual_key, requested_key)),
            Box::new(rewrite_dispatch_request_ty(actual_value, requested_value)),
            attr.clone(),
        ),
        (
            Tir2Ty::Future(actual_value, actual_error, _),
            Tir2Ty::Future(requested_value, requested_error, attr),
        ) => Tir2Ty::Future(
            Box::new(rewrite_dispatch_request_ty(actual_value, requested_value)),
            Box::new(rewrite_dispatch_request_ty(actual_error, requested_error)),
            attr.clone(),
        ),
        (Tir2Ty::Union(actual, _), Tir2Ty::Union(requested, attr))
            if actual.len() == requested.len() =>
        {
            Tir2Ty::Union(
                actual
                    .iter()
                    .zip(requested.iter())
                    .map(|(actual, requested)| rewrite_dispatch_request_ty(actual, requested))
                    .collect(),
                attr.clone(),
            )
        }
        (
            Tir2Ty::Class(actual_qtn, actual_args, _),
            Tir2Ty::Class(requested_qtn, requested_args, attr),
        ) if actual_qtn == requested_qtn && actual_args.len() == requested_args.len() => {
            Tir2Ty::Class(
                requested_qtn.clone(),
                actual_args
                    .iter()
                    .zip(requested_args.iter())
                    .map(|(actual, requested)| rewrite_dispatch_request_ty(actual, requested))
                    .collect(),
                attr.clone(),
            )
        }
        (
            Tir2Ty::Interface(actual_qtn, actual_args, actual_assoc, _),
            Tir2Ty::Interface(requested_qtn, requested_args, requested_assoc, attr),
        ) if actual_qtn == requested_qtn && actual_args.len() == requested_args.len() => {
            Tir2Ty::Interface(
                requested_qtn.clone(),
                actual_args
                    .iter()
                    .zip(requested_args.iter())
                    .map(|(actual, requested)| rewrite_dispatch_request_ty(actual, requested))
                    .collect(),
                requested_assoc
                    .iter()
                    .map(|(requested_name, requested_ty)| {
                        let rewritten = actual_assoc
                            .iter()
                            .find(|(actual_name, _)| actual_name == requested_name)
                            .map_or_else(
                                || requested_ty.clone(),
                                |(_, actual_ty)| {
                                    rewrite_dispatch_request_ty(actual_ty, requested_ty)
                                },
                            );
                        (requested_name.clone(), rewritten)
                    })
                    .collect(),
                attr.clone(),
            )
        }
        (
            Tir2Ty::Function {
                params: actual_params,
                ret: actual_ret,
                throws: actual_throws,
                ..
            },
            Tir2Ty::Function {
                generic_params,
                generic_param_bounds,
                params: requested_params,
                ret: requested_ret,
                throws: requested_throws,
                attr,
            },
        ) if actual_params.len() == requested_params.len()
            && actual_params
                .iter()
                .zip(requested_params.iter())
                .all(|(actual, requested)| actual.mode == requested.mode) =>
        {
            Tir2Ty::Function {
                generic_params: generic_params.clone(),
                generic_param_bounds: generic_param_bounds.clone(),
                params: actual_params
                    .iter()
                    .zip(requested_params.iter())
                    .map(|(actual, requested)| Tir2FunctionParamTy {
                        name: requested.name.clone(),
                        ty: rewrite_dispatch_request_ty(&actual.ty, &requested.ty),
                        mode: requested.mode,
                    })
                    .collect(),
                ret: Box::new(rewrite_dispatch_request_ty(actual_ret, requested_ret)),
                throws: Box::new(rewrite_dispatch_request_ty(actual_throws, requested_throws)),
                attr: attr.clone(),
            }
        }
        _ => requested.clone(),
    }
}

fn bind_interface_class_type_arg(
    name: &Name,
    actual: &Tir2Ty,
    bindings: &mut FxHashMap<Name, Tir2Ty>,
    aliases: &HashMap<QualifiedTypeName, Tir2Ty>,
) -> bool {
    match bindings.get(name) {
        Some(existing) => {
            baml_compiler2_tir::normalize::is_same_normalized_type(existing, actual, aliases)
        }
        None => {
            bindings.insert(name.clone(), actual.clone());
            true
        }
    }
}

fn infer_interface_class_bindings(
    formal: &Tir2Ty,
    actual: &Tir2Ty,
    class_params: &[Name],
    aliases: &HashMap<QualifiedTypeName, Tir2Ty>,
    bindings: &mut FxHashMap<Name, Tir2Ty>,
) -> bool {
    match (formal, actual) {
        // A dispatch request like `Iterator<Item = Self.Item, Error = Self.Error>`
        // inside an interface default method is receiver-relative evidence, not
        // a concrete type to compare against every implementor. The runtime
        // class guard chooses the implementor; concrete associated bindings
        // such as `Error = never` are valid for that implementor.
        (_, Tir2Ty::AssociatedTypeProjection { .. }) => true,
        // The requested arg is an open type-var from the dispatch site, not
        // necessarily the candidate class's parameter even if it has the same
        // spelling (`T`, `E`, ...). If the formal side was not a bindable
        // candidate parameter, treat it as a wildcard; this lets
        // `Iterator<R, E | E2>` satisfy an open `Iterator<T, E>` request.
        (Tir2Ty::TypeVar(name, _), Tir2Ty::TypeVar(_, _)) if !class_params.contains(name) => true,
        (Tir2Ty::TypeVar(name, _), _) => {
            bind_interface_class_type_arg(name, actual, bindings, aliases)
        }
        (_, Tir2Ty::TypeVar(_, _)) => true,
        (Tir2Ty::List(f, _), Tir2Ty::List(a, _))
        | (Tir2Ty::EvolvingList(f, _), Tir2Ty::EvolvingList(a, _))
        | (Tir2Ty::Future(f, _, _), Tir2Ty::Future(a, _, _)) => {
            infer_interface_class_bindings(f, a, class_params, aliases, bindings)
        }
        (
            Tir2Ty::Map {
                key: fk, value: fv, ..
            },
            Tir2Ty::Map {
                key: ak, value: av, ..
            },
        )
        | (Tir2Ty::EvolvingMap(fk, fv, _), Tir2Ty::EvolvingMap(ak, av, _)) => {
            infer_interface_class_bindings(fk, ak, class_params, aliases, bindings)
                && infer_interface_class_bindings(fv, av, class_params, aliases, bindings)
        }
        (
            Tir2Ty::Function {
                params: fp,
                ret: fr,
                throws: fth,
                ..
            },
            Tir2Ty::Function {
                params: ap,
                ret: ar,
                throws: ath,
                ..
            },
        ) => {
            fp.len() == ap.len()
                && fp.iter().zip(ap.iter()).all(|(fp, ap)| {
                    fp.mode == ap.mode
                        && infer_interface_class_bindings(
                            &fp.ty,
                            &ap.ty,
                            class_params,
                            aliases,
                            bindings,
                        )
                })
                && infer_interface_class_bindings(fr, ar, class_params, aliases, bindings)
                && infer_interface_class_bindings(fth, ath, class_params, aliases, bindings)
        }
        (Tir2Ty::Class(fqtn, fargs, _), Tir2Ty::Class(aqtn, aargs, _))
            if fqtn == aqtn && fargs.len() == aargs.len() =>
        {
            fargs
                .iter()
                .zip(aargs.iter())
                .all(|(f, a)| infer_interface_class_bindings(f, a, class_params, aliases, bindings))
        }
        (
            Tir2Ty::Interface(fqtn, fargs, f_assoc, _),
            Tir2Ty::Interface(aqtn, aargs, a_assoc, _),
        ) if fqtn == aqtn && fargs.len() == aargs.len() => {
            fargs
                .iter()
                .zip(aargs.iter())
                .all(|(f, a)| infer_interface_class_bindings(f, a, class_params, aliases, bindings))
                && a_assoc.iter().all(|(name, actual_ty)| {
                    f_assoc
                        .iter()
                        .find(|(formal_name, _)| formal_name == name)
                        .is_some_and(|(_, formal_ty)| {
                            infer_interface_class_bindings(
                                formal_ty,
                                actual_ty,
                                class_params,
                                aliases,
                                bindings,
                            )
                        })
                })
        }
        (Tir2Ty::Union(fparts, _), Tir2Ty::Union(aparts, _)) if fparts.len() == aparts.len() => {
            fparts
                .iter()
                .zip(aparts.iter())
                .all(|(f, a)| infer_interface_class_bindings(f, a, class_params, aliases, bindings))
        }
        (Tir2Ty::Union(fparts, _), actual @ Tir2Ty::Never { .. }) => fparts
            .iter()
            .all(|f| infer_interface_class_bindings(f, actual, class_params, aliases, bindings)),
        _ => baml_compiler2_tir::normalize::is_same_normalized_type(formal, actual, aliases),
    }
}

fn interface_class_guard_for_args(
    impl_iface_args: &[Tir2Ty],
    impl_iface_assoc: &[(Name, Tir2Ty)],
    requested_iface_args: &[Tir2Ty],
    requested_iface_assoc: &[(Name, Tir2Ty)],
    class_params: &[Name],
    aliases: &HashMap<QualifiedTypeName, Tir2Ty>,
) -> Option<InterfaceClassGuard> {
    // An *uninstantiated* request (no type args) matches any implementor
    // instantiation — e.g. `self: Container` inside a generic interface's
    // default method dispatches to `IntBox` (which implements `Container<int>`).
    // The runtime IsType on the concrete class still discriminates.
    let request_omits_type_args = requested_iface_args.is_empty() && !impl_iface_args.is_empty();
    if request_omits_type_args && requested_iface_assoc.is_empty() {
        return Some(InterfaceClassGuard::Any);
    }
    let mut bindings = FxHashMap::default();
    if !request_omits_type_args {
        if impl_iface_args.len() != requested_iface_args.len() {
            return None;
        }
        for (impl_arg, requested_arg) in impl_iface_args.iter().zip(requested_iface_args.iter()) {
            if !infer_interface_class_bindings(
                impl_arg,
                requested_arg,
                class_params,
                aliases,
                &mut bindings,
            ) {
                return None;
            }
        }
    }
    for (requested_name, requested_ty) in requested_iface_assoc {
        let (_, impl_ty) = impl_iface_assoc
            .iter()
            .find(|(impl_name, _)| impl_name == requested_name)?;
        if !infer_interface_class_bindings(
            impl_ty,
            requested_ty,
            class_params,
            aliases,
            &mut bindings,
        ) {
            return None;
        }
    }
    let substituted_args: Vec<_> = impl_iface_args
        .iter()
        .map(|arg| baml_compiler2_tir::generics::substitute_ty(arg, &bindings))
        .collect();
    if !request_omits_type_args
        && !interface_tir_type_args_match_preserving_typevars(
            &substituted_args,
            requested_iface_args,
            aliases,
        )
    {
        return None;
    }
    let substituted_assoc: Vec<_> = impl_iface_assoc
        .iter()
        .map(|(name, ty)| {
            (
                name.clone(),
                baml_compiler2_tir::generics::substitute_ty(ty, &bindings),
            )
        })
        .collect();
    for (requested_name, requested_ty) in requested_iface_assoc {
        let (_, impl_ty) = substituted_assoc
            .iter()
            .find(|(impl_name, _)| impl_name == requested_name)?;
        if !tir_type_satisfies_dispatch_request(impl_ty, requested_ty, aliases) {
            return None;
        }
    }
    if class_params.is_empty() {
        return Some(InterfaceClassGuard::Any);
    }
    // Per class type-param: `Some` when the requested interface args pinned it,
    // `None` (wildcard) otherwise. Crucially we no longer collapse to `Any` when
    // *some* params are unbound — that's exactly the `Pair<L,R>` case where one
    // block pins `L` and the other pins `R`; a partial guard keeps them distinct.
    let class_args: Vec<Option<Tir2Ty>> = class_params
        .iter()
        .map(|param| bindings.get(param).cloned())
        .collect();
    if class_args.iter().all(Option::is_none) {
        return Some(InterfaceClassGuard::Any);
    }
    Some(InterfaceClassGuard::Exact(class_args))
}

/// Choose how to seed the frame of a class-owned method dispatched through an
/// interface, given the matched arm's class guard and whether the implementor
/// is generic.
///
/// A fully-pinned guard names every class type arg statically (class-param
/// order), so we seed those directly — keeping the common, hot path
/// allocation-free and its bytecode unchanged. When the guard is `Any` or only
/// partially pins the class params (e.g. a generic class behind a *non-generic*
/// interface, or `Pair<L,R> implements Slot<L>`), the static guard can't name
/// the args, but the matched runtime instance always carries concrete ones — so
/// we bind the receiver and let the VM seed from `inst.class_type_args`. A
/// non-generic implementor needs nothing.
fn class_owned_frame_seed(guard: &InterfaceClassGuard, impl_is_generic: bool) -> CalleeFrameSeed {
    match guard {
        InterfaceClassGuard::Exact(args)
            if args.iter().all(|arg| {
                arg.as_ref()
                    .is_some_and(|ty| !contains_assoc_projection(ty))
            }) =>
        {
            CalleeFrameSeed::Static(
                args.iter()
                    .map(|a| a.clone().expect("all entries are Some"))
                    .collect(),
            )
        }
        _ if impl_is_generic => CalleeFrameSeed::FromReceiverInstance,
        _ => CalleeFrameSeed::Static(Vec::new()),
    }
}

fn contains_assoc_projection(ty: &Tir2Ty) -> bool {
    match ty {
        Tir2Ty::AssociatedTypeProjection { .. } => true,
        Tir2Ty::List(inner, _) | Tir2Ty::EvolvingList(inner, _) => contains_assoc_projection(inner),
        Tir2Ty::Future(value, err, _) => {
            contains_assoc_projection(value) || contains_assoc_projection(err)
        }
        Tir2Ty::Map {
            key: k, value: v, ..
        }
        | Tir2Ty::EvolvingMap(k, v, _) => {
            contains_assoc_projection(k) || contains_assoc_projection(v)
        }
        Tir2Ty::Union(parts, _) => parts.iter().any(contains_assoc_projection),
        Tir2Ty::Class(_, args, _) => args.iter().any(contains_assoc_projection),
        Tir2Ty::Interface(_, args, assoc, _) => {
            args.iter().any(contains_assoc_projection)
                || assoc.iter().any(|(_, ty)| contains_assoc_projection(ty))
        }
        Tir2Ty::Function {
            params,
            ret,
            throws,
            ..
        } => {
            params
                .iter()
                .any(|param| contains_assoc_projection(&param.ty))
                || contains_assoc_projection(ret)
                || contains_assoc_projection(throws)
        }
        _ => false,
    }
}

pub fn convert_tir2_ty(ty: &Tir2Ty, resolved: &ResolvedAliases) -> Ty {
    let attr = ty.attr().clone();
    match ty {
        // Primitives — identity now that the TIR type is `baml_type::Ty`.
        Tir2Ty::Int { attr } => Ty::Int { attr: attr.clone() },
        Tir2Ty::Bigint { attr } => Ty::Bigint { attr: attr.clone() },
        Tir2Ty::Float { attr } => Ty::Float { attr: attr.clone() },
        Tir2Ty::String { attr } => Ty::String { attr: attr.clone() },
        Tir2Ty::Bool { attr } => Ty::Bool { attr: attr.clone() },
        Tir2Ty::Null { attr } => Ty::Null { attr: attr.clone() },
        Tir2Ty::Uint8Array { attr } => Ty::Uint8Array { attr: attr.clone() },
        Tir2Ty::Media(kind, attr) => Ty::Media(*kind, attr.clone()),

        // Named types
        Tir2Ty::Class(qtn, type_args, attr) => {
            let resolved_args: Vec<Ty> = type_args
                .iter()
                .map(|a| convert_tir2_ty(a, resolved))
                .collect();
            Ty::Class(qtn.clone(), resolved_args, attr.clone())
        }
        Tir2Ty::Interface(qtn, type_args, associated_bindings, attr) => {
            let resolved_args: Vec<Ty> = type_args
                .iter()
                .map(|a| convert_tir2_ty(a, resolved))
                .collect();
            let resolved_bindings = associated_bindings
                .iter()
                .map(|(name, ty)| (name.clone(), convert_tir2_ty(ty, resolved)))
                .collect();
            Ty::Interface(qtn.clone(), resolved_args, resolved_bindings, attr.clone())
        }
        Tir2Ty::Enum(qtn, attr) => Ty::Enum(qtn.clone(), attr.clone()),
        Tir2Ty::TypeAlias(qtn, attr) => {
            if resolved.recursive.contains(qtn) {
                // Keep recursive aliases opaque — they need runtime resolution
                Ty::TypeAlias(qtn.clone(), attr.clone())
            } else if let Some(target) = resolved.aliases.get(qtn) {
                // Expand non-recursive aliases inline
                convert_tir2_ty(target, resolved)
            } else {
                // Unknown alias (e.g. from another package) — keep opaque
                Ty::TypeAlias(qtn.clone(), attr.clone())
            }
        }

        // EnumVariant → preserve variant-level type info
        Tir2Ty::EnumVariant(qtn, variant, attr) => {
            Ty::EnumVariant(qtn.clone(), variant.clone(), attr.clone())
        }

        // Containers
        Tir2Ty::List(inner, attr) => {
            Ty::List(Box::new(convert_tir2_ty(inner, resolved)), attr.clone())
        }
        Tir2Ty::Map {
            key: k,
            value: v,
            attr,
        } => Ty::Map {
            key: Box::new(convert_tir2_ty(k, resolved)),
            value: Box::new(convert_tir2_ty(v, resolved)),
            attr: attr.clone(),
        },
        Tir2Ty::Union(members, attr) => Ty::Union(
            members
                .iter()
                .map(|m| convert_tir2_ty(m, resolved))
                .collect(),
            attr.clone(),
        ),
        // Freshness is a compiler-only flag; runtime literal types are uniform,
        // so normalize to `Regular` at the boundary.
        Tir2Ty::Literal(lit, _freshness, attr) => {
            Ty::Literal(lit.clone(), baml_type::Freshness::Regular, attr.clone())
        }

        // Evolving containers → freeze to regular containers
        Tir2Ty::EvolvingList(inner, attr) => {
            Ty::List(Box::new(convert_tir2_ty(inner, resolved)), attr.clone())
        }
        Tir2Ty::EvolvingMap(k, v, attr) => Ty::Map {
            key: Box::new(convert_tir2_ty(k, resolved)),
            value: Box::new(convert_tir2_ty(v, resolved)),
            attr: attr.clone(),
        },

        // Functions — preserve the declared generics + param metadata (kept at
        // runtime for reflection); body type-vars are still erased by the
        // recursive `convert_tir2_ty` calls.
        Tir2Ty::Function {
            generic_params,
            generic_param_bounds,
            params,
            ret,
            throws,
            attr,
        } => Ty::Function {
            generic_params: generic_params.clone(),
            generic_param_bounds: generic_param_bounds
                .iter()
                .map(|b| b.as_ref().map(|t| convert_tir2_ty(t, resolved)))
                .collect(),
            params: params
                .iter()
                .map(|param| baml_type::FunctionParamTy {
                    name: param.name.clone(),
                    ty: convert_tir2_ty(&param.ty, resolved),
                    mode: match param.mode {
                        FunctionParamMode::Required => baml_type::FunctionParamMode::Required,
                        FunctionParamMode::Optional => baml_type::FunctionParamMode::Optional,
                    },
                })
                .collect(),
            ret: Box::new(convert_tir2_ty(ret, resolved)),
            throws: Box::new(convert_tir2_ty(throws, resolved)),
            attr: attr.clone(),
        },

        // Bottom / sentinel types
        Tir2Ty::Never { attr } => Ty::Void { attr: attr.clone() },
        Tir2Ty::Void { attr } => Ty::Void { attr: attr.clone() },
        Tir2Ty::BuiltinUnknown { attr } => Ty::BuiltinUnknown { attr: attr.clone() },
        // The opaque leaf types are shared between TIR and runtime; pass them
        // through as identity (preserving the SAP attr).
        Tir2Ty::RustType { attr } => Ty::RustType { attr: attr.clone() },
        Tir2Ty::Type { attr } => Ty::Type { attr: attr.clone() },
        Tir2Ty::Unknown { attr } => {
            // Map defensively to Void as error recovery.
            Ty::Void { attr: attr.clone() }
        }
        Tir2Ty::Error { attr } => {
            // Map defensively to Void as error recovery.
            Ty::Void { attr: attr.clone() }
        }
        Tir2Ty::AssociatedTypeProjection { attr, .. } => Ty::BuiltinUnknown { attr: attr.clone() },
        // Type variables that survive to runtime-facing metadata are
        // compile-time evidence we could not concretize. Preserve "some value"
        // semantics with `unknown`; never collapse them to `void`, which the VM
        // treats as "no value is produced".
        Tir2Ty::TypeVar(..) => Ty::BuiltinUnknown { attr },
        // BEP-034: future types pass through unchanged with both
        // value and error type parameters mapped.
        Tir2Ty::Future(value, error, attr) => Ty::Future(
            Box::new(convert_tir2_ty(value, resolved)),
            Box::new(convert_tir2_ty(error, resolved)),
            attr.clone(),
        ),
        // TIR never constructs these runtime-only variants; the arms exist only
        // so the match stays exhaustive over the shared `baml_type::Ty`. Pass
        // them through unchanged (recursing into `WatchAccessor`'s payload).
        Tir2Ty::Resource { attr } => Ty::Resource { attr: attr.clone() },
        Tir2Ty::PromptAst { attr } => Ty::PromptAst { attr: attr.clone() },
        Tir2Ty::WatchAccessor(inner, attr) => {
            Ty::WatchAccessor(Box::new(convert_tir2_ty(inner, resolved)), attr.clone())
        }
    }
}

// ─── Ty → TyTemplate conversion for already-resolved Ty values ──────────────

/// Convert an already-resolved `baml_type::Ty` back to a `TyTemplate`.
///
/// This is needed for `IsType` pattern-matching where the pattern type comes
/// through `convert_tir2_ty` (so `TypeVars` are already erased), but we still
/// need a `TyTemplate` to carry class-level type args for the VM to compare
/// against `Instance::class_type_args`.
///
/// For all leaf types that aren't `Ty::Class`, the result is
/// `TyTemplate::Concrete(ty)`.  For `Ty::Class(tn, args, _)` we produce
/// `TyTemplate::Class(tn, args.map(Concrete))` so the VM can compare the
/// resolved args against the instance's `class_type_args`.
///
/// Note: by the time `emit_is_type_branch` is called, any `TypeVars` in the
/// pattern have already been resolved to concrete types — so no
/// `generic_params` are needed.  If future patterns introduce `TypeVars` that
/// survive to MIR, thread `enclosing_generic_params()` through here.
/// Convert a `Tir2Ty` to `TyTemplate`, mapping any `TypeVar(name)` whose
/// `name` appears at position `N` in `generic_params` to `TypeArgRef(N)`.
///
/// Free function counterpart to `MirLowerer::ty_to_template`, exposed so
/// that callers outside of MIR (e.g. `baml_compiler2_emit`'s class-field
/// type lowering) can build the same templates.
/// Depth cap for recursive blanket-impl bound checking (guards pathological
/// `requires`/blanket cycles; real chains are short).
const BLANKET_BOUND_DEPTH: u32 = 16;

/// Recursively decide whether `actual` satisfies `bound`. For an interface
/// `bound` this re-enters `type_implements_interface_via_rule` so a blanket
/// impl whose bound is itself satisfied by *another* blanket impl verifies
/// (BEP-044 wf3 #2 `blanket-on-blanket`). Previously the bound callback was a
/// hard `|_,_| false`, which made `implements<T: Printable> Loud for T` fail to
/// see a blanket-provided `Printable` and crash the VM. Non-interface bounds
/// fall back to normalized-type equality, matching TIR's subtyping.
fn type_satisfies_bound(
    db: &dyn crate::Db,
    actual: &Tir2Ty,
    bound: &Tir2Ty,
    aliases: &std::collections::HashMap<QualifiedTypeName, Tir2Ty>,
    default_pkg: &baml_base::Name,
    depth: u32,
) -> bool {
    if depth == 0 {
        return false;
    }
    if let Tir2Ty::Interface(..) = bound {
        let pkg = match actual {
            Tir2Ty::Class(qtn, _, _) => qtn.package().clone(),
            _ => default_pkg.clone(),
        };
        let registry = baml_compiler2_tir::interfaces::package_implements_registry(
            db,
            baml_compiler2_hir::package::PackageId::new(db, pkg),
        );
        registry.type_implements_interface_via_rule(actual, bound, aliases, |na, nb| {
            type_satisfies_bound(db, na, nb, aliases, default_pkg, depth - 1)
        })
    } else {
        let resolver = baml_compiler2_tir::associated_projection::AssociatedProjectionResolver::new(
            db,
            aliases,
            &(),
        );
        let resolved_actual = resolver.resolve_deep(actual);
        let resolved_bound = resolver.resolve_deep(bound);
        resolver.types_equivalent(&resolved_actual, &resolved_bound)
    }
}

pub fn tir2_to_template(
    ty: &Tir2Ty,
    resolved: &ResolvedAliases,
    generic_params: &[baml_base::Name],
) -> TyTemplate {
    match ty {
        Tir2Ty::AssociatedTypeProjection { member, .. } => generic_params
            .iter()
            .position(|p| p == member)
            .map(|n| {
                TyTemplate::TypeArgRef(u32::try_from(n).expect("generic param index fits in u32"))
            })
            .unwrap_or_else(|| TyTemplate::Concrete(convert_tir2_ty(ty, resolved))),
        Tir2Ty::TypeVar(name, _) => {
            if let Some(n) = generic_params.iter().position(|p| p == name) {
                TyTemplate::TypeArgRef(u32::try_from(n).expect("generic param index fits in u32"))
            } else {
                TyTemplate::Concrete(Ty::Void {
                    attr: baml_type::TyAttr::default(),
                })
            }
        }
        Tir2Ty::List(inner, _) => {
            TyTemplate::Array(Box::new(tir2_to_template(inner, resolved, generic_params)))
        }
        Tir2Ty::Map {
            key: k, value: v, ..
        } => TyTemplate::Map(
            Box::new(tir2_to_template(k, resolved, generic_params)),
            Box::new(tir2_to_template(v, resolved, generic_params)),
        ),
        Tir2Ty::Union(parts, _) => TyTemplate::Union(
            parts
                .iter()
                .map(|p| tir2_to_template(p, resolved, generic_params))
                .collect(),
        ),
        Tir2Ty::Class(qtn, type_args, attr) => {
            if type_args
                .iter()
                .any(baml_compiler2_tir::generics::contains_typevar)
            {
                let template_args: Vec<TyTemplate> = type_args
                    .iter()
                    .map(|a| tir2_to_template(a, resolved, generic_params))
                    .collect();
                TyTemplate::Class(qtn.clone(), template_args)
            } else {
                let resolved_args: Vec<Ty> = type_args
                    .iter()
                    .map(|a| convert_tir2_ty(a, resolved))
                    .collect();
                TyTemplate::Concrete(Ty::Class(qtn.clone(), resolved_args, attr.clone()))
            }
        }
        Tir2Ty::Interface(qtn, type_args, associated_bindings, attr) => {
            if type_args
                .iter()
                .any(baml_compiler2_tir::generics::contains_typevar)
                || associated_bindings
                    .iter()
                    .any(|(_, ty)| baml_compiler2_tir::generics::contains_typevar(ty))
            {
                let template_args: Vec<TyTemplate> = type_args
                    .iter()
                    .map(|a| tir2_to_template(a, resolved, generic_params))
                    .collect();
                let template_bindings = associated_bindings
                    .iter()
                    .map(|(name, ty)| {
                        (name.clone(), tir2_to_template(ty, resolved, generic_params))
                    })
                    .collect();
                TyTemplate::Interface(qtn.clone(), template_args, template_bindings)
            } else {
                let resolved_args: Vec<Ty> = type_args
                    .iter()
                    .map(|a| convert_tir2_ty(a, resolved))
                    .collect();
                let resolved_bindings = associated_bindings
                    .iter()
                    .map(|(name, ty)| (name.clone(), convert_tir2_ty(ty, resolved)))
                    .collect();
                TyTemplate::Concrete(Ty::Interface(
                    qtn.clone(),
                    resolved_args,
                    resolved_bindings,
                    attr.clone(),
                ))
            }
        }
        Tir2Ty::EvolvingList(inner, _) => {
            TyTemplate::Array(Box::new(tir2_to_template(inner, resolved, generic_params)))
        }
        Tir2Ty::EvolvingMap(k, v, _) => TyTemplate::Map(
            Box::new(tir2_to_template(k, resolved, generic_params)),
            Box::new(tir2_to_template(v, resolved, generic_params)),
        ),
        other => TyTemplate::Concrete(convert_tir2_ty(other, resolved)),
    }
}

fn tir2_to_dispatch_guard_template(
    ty: &Tir2Ty,
    resolved: &ResolvedAliases,
    generic_params: &[baml_base::Name],
) -> TyTemplate {
    match ty {
        Tir2Ty::AssociatedTypeProjection { .. } => TyTemplate::Wildcard,
        Tir2Ty::TypeVar(name, _) => generic_params
            .iter()
            .position(|p| p == name)
            .map(|n| {
                TyTemplate::TypeArgRefOrWildcard(
                    u32::try_from(n).expect("generic param index fits in u32"),
                )
            })
            .unwrap_or(TyTemplate::Wildcard),
        Tir2Ty::List(inner, _) | Tir2Ty::EvolvingList(inner, _) => TyTemplate::Array(Box::new(
            tir2_to_dispatch_guard_template(inner, resolved, generic_params),
        )),
        Tir2Ty::Map {
            key: k, value: v, ..
        }
        | Tir2Ty::EvolvingMap(k, v, _) => TyTemplate::Map(
            Box::new(tir2_to_dispatch_guard_template(k, resolved, generic_params)),
            Box::new(tir2_to_dispatch_guard_template(v, resolved, generic_params)),
        ),
        Tir2Ty::Union(parts, _) => TyTemplate::Union(
            parts
                .iter()
                .map(|p| tir2_to_dispatch_guard_template(p, resolved, generic_params))
                .collect(),
        ),
        Tir2Ty::Class(qtn, type_args, _) => TyTemplate::Class(
            qtn.clone(),
            type_args
                .iter()
                .map(|arg| tir2_to_dispatch_guard_template(arg, resolved, generic_params))
                .collect(),
        ),
        Tir2Ty::Interface(qtn, type_args, assoc, _) => TyTemplate::Interface(
            qtn.clone(),
            type_args
                .iter()
                .map(|arg| tir2_to_dispatch_guard_template(arg, resolved, generic_params))
                .collect(),
            assoc
                .iter()
                .map(|(name, ty)| {
                    (
                        name.clone(),
                        tir2_to_dispatch_guard_template(ty, resolved, generic_params),
                    )
                })
                .collect(),
        ),
        other => tir2_to_template(other, resolved, generic_params),
    }
}

pub(crate) fn ty_to_template_from_resolved_ty(ty: &Ty) -> TyTemplate {
    match ty {
        Ty::Class(tn, args, _) if !args.is_empty() => {
            // Parametric class: produce TyTemplate::Class with Concrete leaves.
            // This allows the VM to check `expected_args == inst.class_type_args`.
            TyTemplate::Class(
                tn.clone(),
                args.iter().map(ty_to_template_from_resolved_ty).collect(),
            )
        }
        // All other types: wrap in Concrete.  The VM uses this for the
        // existing fast paths (primitive type tags, monomorphic classes).
        other => TyTemplate::Concrete(other.clone()),
    }
}

// ─── def_to_item_ref helper ──────────────────────────────────────────────────

use baml_compiler2_hir::{
    compiler2_all_files, contributions::Definition, file_package::file_package,
};
// Use the PPIR item tree (which includes synthetic *$stream items) rather than
// the bare HIR item tree. TIR resolves methods using PPIR `LocalItemId`s, so
// MIR must use the same tree to avoid index mismatches.
use baml_compiler2_ppir::file_item_tree;

pub fn def_to_item_ref<'db>(db: &'db dyn crate::Db, def: Definition<'db>) -> ItemRef {
    let file = def.file(db);
    let pkg_info = file_package(db, file);
    let item_tree = file_item_tree(db, file);

    let name: Name = match def {
        Definition::Function(loc) => item_tree[loc.id(db)].name.clone(),
        Definition::Class(loc) => item_tree[loc.id(db)].name.clone(),
        Definition::Enum(loc) => item_tree[loc.id(db)].name.clone(),
        Definition::Interface(loc) => item_tree[loc.id(db)].name.clone(),
        Definition::TypeAlias(loc) => item_tree[loc.id(db)].name.clone(),
        Definition::TemplateString(loc) => item_tree[loc.id(db)].name.clone(),
        Definition::Client(loc) => item_tree[loc.id(db)].name.clone(),
        Definition::Generator(loc) => item_tree[loc.id(db)].name.clone(),
        Definition::Test(loc) => item_tree[loc.id(db)].name.clone(),
        Definition::RetryPolicy(loc) => item_tree[loc.id(db)].name.clone(),
        Definition::Let(loc) => item_tree[loc.id(db)].name.clone(),
    };

    // For function definitions, check if this is a class method by searching
    // the item tree's class methods lists.
    if let Definition::Function(func_loc) = def {
        let func_local_id = func_loc.id(db);
        for (class_id, class_data) in &item_tree.classes {
            if class_data.methods.contains(&func_local_id) {
                let class_loc = baml_compiler2_hir::loc::ClassLoc::new(db, file, *class_id);
                return method_item_ref(db, class_loc, func_loc);
            }
        }
        // BEP-044: interface default methods are also stored in
        // `item_tree.functions` and need a Method-shaped ItemRef so they
        // get a distinct global slot keyed on the interface name (instead
        // of colliding with same-named free functions in the package).
        for iface_data in item_tree.interfaces.values() {
            if iface_data.default_methods.contains(&func_local_id) {
                return ItemRef::Method {
                    package: pkg_info.package.clone(),
                    namespace: pkg_info.namespace_path,
                    class: iface_data.name.clone(),
                    name,
                };
            }
        }
        for imp in &item_tree.implements_for {
            if imp.methods.contains(&func_local_id) {
                return ItemRef::Method {
                    package: pkg_info.package.clone(),
                    namespace: pkg_info.namespace_path,
                    class: Name::new(format!(
                        "{}$for${}",
                        imp.interface_target.expr, imp.for_target.expr
                    )),
                    name,
                };
            }
        }
    }

    ItemRef::Free {
        package: pkg_info.package.clone(),
        namespace: pkg_info.namespace_path,
        name,
    }
}

fn scoped_implements_method_name(
    item_tree: &baml_compiler2_hir::item_tree::ItemTree,
    func_id: baml_compiler2_hir::ids::LocalItemId<baml_compiler2_hir::ids::FunctionMarker>,
    method_name: &Name,
) -> Name {
    item_tree
        .method_to_iface_target
        .get(&func_id)
        .map(|target| Name::new(format!("{}.{}", target.expr, method_name)))
        .unwrap_or_else(|| method_name.clone())
}

fn method_item_ref<'db>(
    db: &'db dyn crate::Db,
    class_loc: baml_compiler2_hir::loc::ClassLoc<'db>,
    func_loc: baml_compiler2_hir::loc::FunctionLoc<'db>,
) -> ItemRef {
    let pkg_info = file_package(db, class_loc.file(db));
    let item_tree = file_item_tree(db, class_loc.file(db));
    let class_data = &item_tree[class_loc.id(db)];
    let func_id = func_loc.id(db);
    let func_data = &item_tree[func_id];
    ItemRef::Method {
        package: pkg_info.package,
        namespace: pkg_info.namespace_path,
        class: class_data.name.clone(),
        name: scoped_implements_method_name(&item_tree, func_id, &func_data.name),
    }
}

/// Convert a `MemberResolution` (from TIR) into an `ItemRef` (for MIR).
///
/// Only `Method` and `Free` variants are callable — callers must guard against
/// `Field` and `Variant` variants before calling this function.
fn resolution_to_item_ref(
    db: &dyn crate::Db,
    res: &baml_compiler2_tir::inference::MemberResolution<'_>,
) -> Option<ItemRef> {
    use baml_compiler2_tir::inference::MemberResolution;
    match res {
        MemberResolution::Free { func_loc } => {
            let pkg_info = file_package(db, func_loc.file(db));
            let item_tree = file_item_tree(db, func_loc.file(db));
            let func_data = &item_tree[func_loc.id(db)];
            Some(ItemRef::Free {
                package: pkg_info.package,
                namespace: pkg_info.namespace_path,
                name: func_data.name.clone(),
            })
        }
        MemberResolution::BoundMethod {
            class_loc,
            func_loc,
        }
        | MemberResolution::UnboundMethod {
            class_loc,
            func_loc,
        } => Some(method_item_ref(db, *class_loc, *func_loc)),
        MemberResolution::InterfaceDefaultMethod {
            iface_loc,
            func_loc,
        } => {
            let pkg_info = file_package(db, iface_loc.file(db));
            let item_tree = file_item_tree(db, iface_loc.file(db));
            let iface_data = &item_tree[iface_loc.id(db)];
            let func_data = &item_tree[func_loc.id(db)];
            Some(ItemRef::Method {
                package: pkg_info.package,
                namespace: pkg_info.namespace_path,
                class: iface_data.name.clone(),
                name: func_data.name.clone(),
            })
        }
        MemberResolution::Field { .. } | MemberResolution::Variant { .. } => None,
    }
}

// ─── LoweringContext ─────────────────────────────────────────────────────────

// Re-use ExprId from baml_compiler2_ast (already imported above via ExprId)
use baml_compiler2_ast::{
    AssignOp as AstAssignOp, AstSourceMap, BinaryOp as AstBinaryOp, Expr as AstExpr,
    ExprBody as AstExprBody, ExprId as AstExprId, Literal as AstLiteral, PatId as AstPatId,
    Pattern as AstPattern, Stmt as AstStmt, StmtId as AstStmtId, TypeExpr as AstTypeExpr,
    UnaryOp as AstUnaryOp,
};
use baml_compiler2_hir::{
    body::{FunctionBody, LetBody, let_body, let_body_source_map},
    loc::{FunctionLoc, LetLoc},
    package::{PackageId, package_items},
    scope::FileScopeId,
    semantic_index::{BindingId, DefinitionSite},
};
use baml_compiler2_ppir::file_semantic_index;
use baml_compiler2_tir::{
    inference::infer_scope_types,
    resolve::{ResolvedName, resolve_name_at_in_scope},
};
use rustc_hash::FxHashMap;

type ClassFieldIndices = IndexMap<TypeName, IndexMap<String, usize>>;
type ClassFieldTypes = IndexMap<TypeName, IndexMap<String, Ty>>;
type EnumVariantIndices = IndexMap<QualifiedTypeName, IndexMap<String, usize>>;
type InterfaceImplementors = IndexMap<TypeName, Vec<TypeName>>;
type InterfaceTypeView = (TypeName, Vec<Tir2Ty>, Vec<(Name, Tir2Ty)>);
#[derive(Clone, PartialEq, Eq)]
struct InterfaceTypeImplementor {
    runtime_ty: Ty,
    tir_ty: Tir2Ty,
    iface_args: Vec<Tir2Ty>,
    iface_assoc: Vec<(Name, Tir2Ty)>,
}
type InterfaceTypeImplementors = IndexMap<TypeName, Vec<InterfaceTypeImplementor>>;

fn lower_interface_target_args<'db>(
    db: &'db dyn crate::Db,
    target: &baml_compiler2_ast::TypeExpr,
    pkg_items: &baml_compiler2_hir::package::PackageItems<'db>,
    namespace_path: &[Name],
    generic_params: &[Name],
    diags: &mut Vec<baml_compiler2_tir::infer_context::TirTypeError>,
) -> Vec<Tir2Ty> {
    match target {
        baml_compiler2_ast::TypeExpr::Path { generic_args, .. } => generic_args
            .iter()
            .map(|arg| {
                baml_compiler2_tir::lower_type_expr::lower_type_expr_in_ns(
                    db,
                    arg,
                    pkg_items,
                    namespace_path,
                    generic_params,
                    diags,
                )
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn lower_interface_target_associated_bindings<'db>(
    db: &'db dyn crate::Db,
    target: &baml_compiler2_ast::TypeExpr,
    associated_type_bindings: &[baml_compiler2_ast::AssociatedTypeBindingDef],
    pkg_items: &baml_compiler2_hir::package::PackageItems<'db>,
    namespace_path: &[Name],
    generic_params: &[Name],
    diags: &mut Vec<baml_compiler2_tir::infer_context::TirTypeError>,
) -> Vec<(Name, Tir2Ty)> {
    let Some(target_loc) = baml_compiler2_tir::interfaces::resolve_path_to_interface(
        db,
        target,
        pkg_items,
        namespace_path,
    ) else {
        return Vec::new();
    };
    let target_tree = baml_compiler2_hir::file_item_tree(db, target_loc.file(db));
    let Some(target_data) = target_tree.interfaces.get(&target_loc.id(db)) else {
        return Vec::new();
    };
    let target_args =
        lower_interface_target_args(db, target, pkg_items, namespace_path, generic_params, diags);
    let mut bindings =
        baml_compiler2_tir::generics::bind_type_vars(&target_data.generic_params, &target_args);
    for param in generic_params {
        bindings.entry(param.clone()).or_insert_with(|| {
            Tir2Ty::TypeVar(param.clone(), baml_compiler2_tir::ty::TyAttr::default())
        });
    }
    let target_iface_pkg = baml_compiler2_hir::file_package::file_package(db, target_loc.file(db));
    target_data
        .associated_types
        .iter()
        .filter_map(|assoc| {
            if let Some(binding) = associated_type_bindings
                .iter()
                .find(|binding| binding.name == assoc.name)
                && let Some(type_expr) = &binding.type_expr
            {
                let ty = baml_compiler2_tir::generics::lower_type_expr_with_generics(
                    db,
                    &type_expr.expr,
                    pkg_items,
                    namespace_path,
                    &bindings,
                    diags,
                );
                bindings.insert(assoc.name.clone(), ty.clone());
                return Some((assoc.name.clone(), ty));
            }
            assoc.default.as_ref().map(|default| {
                let ty = baml_compiler2_tir::generics::lower_type_expr_with_generics(
                    db,
                    &default.expr,
                    pkg_items,
                    &target_iface_pkg.namespace_path,
                    &bindings,
                    diags,
                );
                bindings.insert(assoc.name.clone(), ty.clone());
                (assoc.name.clone(), ty)
            })
        })
        .collect()
}

fn class_type_name_from_qtn(db: &dyn crate::Db, class_qtn: &QualifiedTypeName) -> Option<TypeName> {
    let class_pkg_id = baml_compiler2_hir::package::PackageId::new(db, class_qtn.package().clone());
    let class_pkg_items = baml_compiler2_hir::package::package_items(db, class_pkg_id);
    let class_ns: Vec<Name> = class_qtn.namespace().clone();
    let Some(baml_compiler2_hir::contributions::Definition::Class(_)) =
        class_pkg_items.lookup_type(&class_ns, class_qtn.name())
    else {
        return None;
    };

    Some(class_qtn.clone())
}

fn interface_type_name_from_loc<'db>(
    db: &'db dyn crate::Db,
    iface_loc: baml_compiler2_hir::loc::InterfaceLoc<'db>,
) -> Option<TypeName> {
    let iface_tree = baml_compiler2_hir::file_item_tree(db, iface_loc.file(db));
    let iface_data = iface_tree.interfaces.get(&iface_loc.id(db))?;
    let qtn = baml_compiler2_tir::lower_type_expr::qualify_def(
        db,
        Definition::Interface(iface_loc),
        &iface_data.name,
    );
    Some(qtn)
}

fn push_unique_interface_implementor(
    interface_implementors: &mut InterfaceImplementors,
    iface_tn: TypeName,
    class_tn: &TypeName,
) {
    let entry = interface_implementors.entry(iface_tn).or_default();
    if !entry.contains(class_tn) {
        entry.push(class_tn.clone());
    }
}

fn register_class_for_interface_closure<'db>(
    db: &'db dyn crate::Db,
    root_iface_loc: baml_compiler2_hir::loc::InterfaceLoc<'db>,
    root_iface_args: &[Tir2Ty],
    pkg_items: &baml_compiler2_hir::package::PackageItems<'db>,
    namespace_path: &[Name],
    class_tn: &TypeName,
    interface_implementors: &mut InterfaceImplementors,
) {
    for (iface_loc, _iface_args, _iface_assoc) in
        baml_compiler2_tir::interfaces::interface_closure_locs_with_args_and_assoc(
            db,
            root_iface_loc,
            root_iface_args,
            &[],
            pkg_items,
            namespace_path,
        )
    {
        if let Some(iface_tn) = interface_type_name_from_loc(db, iface_loc) {
            push_unique_interface_implementor(interface_implementors, iface_tn, class_tn);
        }
    }
}

struct PackagePopulation<'a> {
    class_fields: &'a mut ClassFieldIndices,
    class_field_types: &'a mut ClassFieldTypes,
    enum_variants: &'a mut EnumVariantIndices,
    interface_implementors: &'a mut InterfaceImplementors,
    interface_type_implementors: &'a mut InterfaceTypeImplementors,
}

/// All package-invariant data needed to construct a [`LoweringContext`]: the
/// class/enum/interface schema maps plus resolved type aliases.
///
/// `LoweringContext::new` runs once per function, but every function in a
/// package sees the *same* schema. Building this inline per function made MIR
/// lowering `O(functions × classes)` — each function re-lowered every class
/// field type (`populate_from_package`) and recomputed every alias
/// (`ResolvedAliases::for_package`, which also re-runs `find_recursive_aliases`
/// over the whole project). Computing it once in the [`package_lowering_data`]
/// Salsa query collapses that to `O(classes)` total; the maps are then borrowed
/// (not cloned) into each `LoweringContext`.
#[derive(Clone, PartialEq, Eq, Default)]
struct PackageLoweringData {
    class_fields: ClassFieldIndices,
    class_field_types: ClassFieldTypes,
    enum_variants: EnumVariantIndices,
    interface_implementors: InterfaceImplementors,
    interface_type_implementors: InterfaceTypeImplementors,
    resolved_aliases: ResolvedAliases,
}

/// # Safety
///
/// Mirrors [`baml_compiler2_hir::package::PackageItems`]'s impl. The contained
/// maps hold no Salsa-interned (`'db`) data, so storing them by value is sound;
/// `maybe_update` uses `PartialEq` for proper Salsa early-cutoff.
#[allow(unsafe_code)]
unsafe impl salsa::Update for PackageLoweringData {
    unsafe fn maybe_update(old_pointer: *mut Self, new_value: Self) -> bool {
        // SAFETY: `old_pointer` is valid, aligned, and Salsa-owned.
        #[allow(unsafe_code)]
        let old = unsafe { &*old_pointer };
        if old == &new_value {
            false
        } else {
            #[allow(unsafe_code)]
            unsafe {
                std::ptr::drop_in_place(old_pointer);
                std::ptr::write(old_pointer, new_value);
            }
            true
        }
    }
}

/// Build the package-invariant [`PackageLoweringData`] once per package,
/// memoized by Salsa and shared across every function's `LoweringContext`.
#[salsa::tracked(returns(ref))]
fn package_lowering_data<'db>(
    db: &'db dyn crate::Db,
    pkg_id: baml_compiler2_hir::package::PackageId<'db>,
) -> PackageLoweringData {
    use baml_compiler2_hir::package::{package_dependencies, package_items};

    let resolved_aliases = ResolvedAliases::for_package(db, pkg_id);

    let mut class_fields = ClassFieldIndices::default();
    let mut class_field_types = ClassFieldTypes::default();
    let mut enum_variants = EnumVariantIndices::default();
    let mut interface_implementors = InterfaceImplementors::default();
    let mut interface_type_implementors = InterfaceTypeImplementors::default();
    {
        let mut population = PackagePopulation {
            class_fields: &mut class_fields,
            class_field_types: &mut class_field_types,
            enum_variants: &mut enum_variants,
            interface_implementors: &mut interface_implementors,
            interface_type_implementors: &mut interface_type_implementors,
        };

        // Dependency packages first (e.g., "baml" builtins); current-package
        // items overwrite on collision.
        for &dep_id in package_dependencies(db, pkg_id) {
            let dep_items = package_items(db, dep_id);
            let dep_name = dep_id.name(db);
            LoweringContext::populate_from_package(
                db,
                dep_items,
                &dep_name,
                &mut population,
                &resolved_aliases,
            );
        }

        let pkg_items = package_items(db, pkg_id);
        let pkg_name = pkg_id.name(db);
        LoweringContext::populate_from_package(
            db,
            pkg_items,
            &pkg_name,
            &mut population,
            &resolved_aliases,
        );
    }

    PackageLoweringData {
        class_fields,
        class_field_types,
        enum_variants,
        interface_implementors,
        interface_type_implementors,
        resolved_aliases,
    }
}

#[derive(Clone, Copy)]
struct InterfaceDispatchCall<'a> {
    expr_id: AstExprId,
    recv_local: Local,
    recv_tir_ty: Option<&'a Tir2Ty>,
    iface_tn: &'a TypeName,
    iface_type_args: &'a [Tir2Ty],
    iface_assoc: &'a [(Name, Tir2Ty)],
    method: &'a Name,
    args: &'a [AstExprId],
}

#[derive(Clone, Copy)]
struct InterfaceDefaultCallContext<'a> {
    iface_tn: &'a TypeName,
    iface_type_args: &'a [Tir2Ty],
    iface_assoc: &'a [(Name, Tir2Ty)],
    method: &'a Name,
}

#[derive(Clone)]
enum InterfaceClassGuard {
    Any,
    /// One entry per class type-param. `Some(ty)` pins that position; `None`
    /// is a wildcard (BEP-044: a partial guard, e.g. `implements Getter<L>`
    /// requested as `Getter<string>` pins `L` but leaves `R` free, so two
    /// blocks on the same class instantiate to distinguishable guards).
    Exact(Vec<Option<Tir2Ty>>),
}

#[derive(Clone)]
struct InterfaceFieldCandidate {
    impl_tn: TypeName,
    guard: InterfaceClassGuard,
    field_idx: usize,
}

#[derive(Clone)]
enum InterfaceDispatchGuard {
    Class {
        impl_tn: TypeName,
        guard: InterfaceClassGuard,
    },
    Type(Ty),
}

#[derive(Clone)]
struct InterfaceMethodCandidate {
    guard: InterfaceDispatchGuard,
    item_ref: ItemRef,
    /// How to seed the callee frame's `type_args` so a dispatched method that
    /// reads its enclosing `T` at runtime resolves it correctly.
    frame_seed: CalleeFrameSeed,
}

/// Strategy for seeding a dispatched method's `frame.type_args`.
#[derive(Clone)]
enum CalleeFrameSeed {
    /// Bind the receiver into a `BoundMethod` so the VM seeds `frame.type_args`
    /// from the runtime instance's `class_type_args` (class-param order), then
    /// the method call's own type args. Used for a class-owned method on a generic
    /// class whose class params are *not* fully pinned by the interface request
    /// (`Any`/partial guard) — the static guard can't name the args, but the
    /// matched instance always carries them.
    FromReceiverInstance,
    /// Seed statically with these resolved type args, in the De Bruijn order
    /// the callee body assumes (see `enclosing_generic_params`):
    ///   • a class-owned method with a fully-pinned guard ⇒ the implementor's
    ///     resolved class type args (class-param order);
    ///   • an inherited interface default ⇒ the resolved *interface* args
    ///     (which can differ from the implementor's class args when the
    ///     `implements` block renames/reorders params).
    /// Empty ⇒ no seeding (non-generic, or a path that does not thread args).
    /// May contain `TypeVar`s referring to the *caller's* enclosing generics;
    /// `emit_method_candidate_switch` lowers them via `ty_to_template`.
    Static(Vec<Tir2Ty>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum MetadataScope {
    Body(FileScopeId),
    ParameterDefault(FileScopeId),
}

type ExprMetadataKey = (MetadataScope, AstExprId);
type PatMetadataKey = (MetadataScope, AstPatId);

struct LoweringContext<'db> {
    db: &'db dyn crate::Db,
    builder: MirBuilder,
    locals: HashMap<Name, Local>,
    binding_locals: HashMap<BindingId, Local>,
    loop_context: Option<LoopContext>,
    catch_context: Option<CatchContext>,
    exit_block: BlockId,

    // Eagerly aggregated type maps from all scopes in the function.
    // Keyed by metadata namespace plus arena ID to avoid collisions between
    // function bodies, lambda bodies, and parameter-default arenas.
    expr_types: FxHashMap<ExprMetadataKey, Tir2Ty>,
    pat_types: FxHashMap<PatMetadataKey, Tir2Ty>,
    // Member resolutions from TIR.
    resolutions: FxHashMap<ExprMetadataKey, baml_compiler2_tir::inference::MemberResolution<'db>>,
    // Match expressions that TIR determined are exhaustive
    exhaustive_matches: rustc_hash::FxHashSet<ExprMetadataKey>,
    // TIR-inferred root segment type for each multi-segment Path expression.
    // Used by lower_multi_segment_path_as_field_chain to get the correct root
    // type even when the MIR local was declared with a coarser type (e.g.
    // catch variables declared as BuiltinUnknown).
    path_root_types: FxHashMap<ExprMetadataKey, Tir2Ty>,
    // TIR-inferred type of every prefix `segments[..=seg_idx]` for multi-segment
    // local-rooted Path expressions. Used by the Phase-8 method-call prepend to
    // read the receiver-prefix type (`segments[..segments.len() - 1]`) so that
    // class-level type args are threaded correctly through depth ≥ 3 paths
    // like `holder.box.describe()`.
    path_segment_types: FxHashMap<(MetadataScope, AstExprId, usize), Tir2Ty>,
    // Per-segment member resolutions for multi-segment local-rooted Path expressions.
    // Set by TIR's infer_local_rooted_path; indexed by (scope, Path ExprId).
    // path_member_resolutions[(scope, expr_id)][i] is the resolution for segments[i+1].
    path_member_resolutions:
        FxHashMap<ExprMetadataKey, Vec<baml_compiler2_tir::inference::MemberResolution<'db>>>,
    // Full-arity argument binding plans from TIR.
    call_plans: FxHashMap<ExprMetadataKey, baml_compiler2_tir::inference::CallPlan>,
    /// TIR-recorded generic instantiation per call whose callee declares type
    /// params (declared De Bruijn order). Lowered to `LoadType` operands when
    /// the call site writes no explicit `<...>`, so inferred-generic calls
    /// populate the callee's `frame.type_args` just like explicit ones.
    // Function-value adapters from TIR checked coercions.
    function_coercions: FxHashMap<ExprMetadataKey, baml_compiler2_tir::inference::FunctionCoercion>,
    // Function generic bounds, lowered in TIR space. MIR uses these to keep
    // bounded type variables ABI-erased while still lowering bound-member
    // access through the interface dispatch machinery.
    generic_param_bounds: FxHashMap<Name, Tir2Ty>,

    // TIR types of the in-scope lambda parameters, by name. TIR does not record
    // `path_segment_types` for a lambda-parameter receiver (`(a: T) -> a.m()`),
    // so interface dispatch on such a receiver falls back to this map to learn
    // its static type — e.g. a bounded type variable whose `extends` bound
    // names the dispatching interface (`a.compare(b)` where `T extends
    // Comparable`). Saved/restored across nested lambdas.
    lambda_param_tir_types: FxHashMap<Name, Tir2Ty>,

    // The FileScopeId of the expression body currently being lowered.
    // Updated when descending into lambda bodies (Phase 3+).
    current_scope: FileScopeId,
    // Metadata namespace for the expression arena currently being lowered.
    current_metadata_scope: MetadataScope,

    // AST expression body and source map
    body: AstExprBody,
    source_map: Option<AstSourceMap>,
    file: baml_base::SourceFile,
    func_loc: Option<FunctionLoc<'db>>,
    source_param_scope: Option<FileScopeId>,
    /// Raw function name from the item tree (e.g. `"Foo$render_prompt"`).
    /// Used to disambiguate companion scopes that share the same span.
    scope_func_name: Option<Name>,

    // Schema maps built from PackageItems.
    // class_fields and class_type_tags are keyed by TypeName (name + module_path)
    // so that e.g. baml.http.Request and a user-defined Request are distinct.
    // enum_variants is keyed by QualifiedTypeName for the same reason: distinct
    // namespaces can define enums with the same short name.
    // Borrowed from the package-keyed `package_lowering_data` query so every
    // function in a package shares one computation instead of rebuilding these
    // (see [`PackageLoweringData`]).
    class_fields: &'db ClassFieldIndices,
    class_field_types: &'db ClassFieldTypes,
    enum_variants: &'db EnumVariantIndices,
    /// Pre-computed type tags for class types, used by `SwitchKind::TypeTag`
    /// for union-type switch optimization (ported from MIR 1).
    class_type_tags: IndexMap<TypeName, i64>,
    /// BEP-044: for every interface, the list of classes that implement it
    /// (directly or transitively through interface `requires`). Lets the field-access
    /// and method-call lowering paths emit a type-tag switch over the
    /// implementor set when the static receiver type is an interface.
    interface_implementors: &'db InterfaceImplementors,
    /// BEP-044: non-class concrete implementors, such as
    /// `implements Debuggable for int`. These are kept separate from
    /// `interface_implementors` because reflection/runtime metadata stores
    /// named classes, while dispatch can use primitive type tags directly.
    interface_type_implementors: &'db InterfaceTypeImplementors,

    // Pre-computed type alias data for inline expansion in convert_tir2_ty.
    // Borrowed from `package_lowering_data` (shared across every function in
    // the package) rather than cloned per context.
    resolved_aliases: &'db ResolvedAliases,

    watched_locals_stack: Vec<Local>,

    // Counter for generating unique synthetic variable names (e.g. __for_idx, __for_idx_1)
    synthetic_name_counts: HashMap<String, usize>,

    // Lambda functions lowered during body traversal.
    // Collected here and moved into MirFunction.lambdas at the end of lowering.
    // Each entry is a fully-lowered MirFunction for one lambda expression.
    pending_lambdas: Vec<MirFunction>,

    // Generic params of the enclosing lambda(s), accumulated outermost-first.
    // Empty at top-level; `lower_lambda` extends it with the lambda's own
    // `generic_params` while lowering its body and restores it afterward.
    // `enclosing_generic_params()` appends this so that `reflect.type_of<T>`
    // (and other type-arg resolution) inside a generic lambda body resolves the
    // lambda's `T` to the correct `TypeArgRef` slot — `func_loc` only knows the
    // enclosing top-level function's (and class's) params, never a lambda's.
    lambda_generic_params: Vec<baml_base::Name>,

    // Capture map for the current lambda body.
    // `Some(map)` when lowering inside a lambda body; `None` for top-level functions.
    // Maps captured binding identity -> index into the closure's captures array.
    // Used by `lower_path_expr` to resolve references to captured variables as
    // `Place::Capture(idx)` instead of `Place::Local(_)`.
    capture_indices: Option<HashMap<BindingId, usize>>,

    // Bindings that were added to the current lambda's capture list transitively
    // because an inner lambda needed them but they were not in the HIR capture
    // list for this lambda. Collected by the parent `lower_lambda` call after
    // the body is lowered so it can extend the outer MakeClosure with extra captures.
    transitive_captures_needed: Vec<BindingId>,

    /// Stack of null-exit blocks for active `OptionalChain` scopes.
    /// When an `OptionalFieldAccess`/`OptionalIndex`/`OptionalCall` encounters null,
    /// it jumps to the top of this stack instead of creating its own null block.
    chain_null_exits: Vec<BlockId>,

    /// Optimization level controlling MIR-level transforms.
    /// At `OptLevel::Two`, constant folding and advanced transforms are applied.
    opt: crate::OptLevel,
}

#[allow(clippy::elidable_lifetime_names)]
impl<'db> LoweringContext<'db> {
    fn baml_iter_qtn(name: &str) -> QualifiedTypeName {
        QualifiedTypeName::new(Name::new("baml"), vec![Name::new("iter")], Name::new(name))
    }

    fn baml_iter_type_name(name: &str) -> TypeName {
        Self::baml_iter_qtn(name)
    }

    fn baml_iter_done_ty() -> Ty {
        Ty::Class(Self::baml_iter_type_name("Done"), vec![], TyAttr::default())
    }

    fn associated_binding_ty(bindings: &[(Name, Tir2Ty)], name: &str) -> Option<Tir2Ty> {
        bindings
            .iter()
            .find(|(binding_name, _)| binding_name.as_str() == name)
            .map(|(_, ty)| ty.clone())
    }

    fn substitute_class_params_in_interface_view(
        view: InterfaceTypeView,
        class_params: &[Name],
        class_args: &[Tir2Ty],
    ) -> InterfaceTypeView {
        if class_params.is_empty() {
            return view;
        }
        let mut bindings = FxHashMap::default();
        for (param, arg) in class_params.iter().zip(class_args.iter()) {
            bindings.insert(param.clone(), arg.clone());
        }
        for param in class_params {
            bindings
                .entry(param.clone())
                .or_insert_with(|| Tir2Ty::TypeVar(param.clone(), TyAttr::default()));
        }

        let (tn, args, assoc) = view;
        let args = args
            .into_iter()
            .map(|ty| baml_compiler2_tir::generics::substitute_ty(&ty, &bindings))
            .collect();
        let assoc = assoc
            .into_iter()
            .map(|(name, ty)| {
                (
                    name,
                    baml_compiler2_tir::generics::substitute_ty(&ty, &bindings),
                )
            })
            .collect();
        (tn, args, assoc)
    }

    fn interface_view_for_class_tir_ty(
        &self,
        class_qtn: &QualifiedTypeName,
        class_args: &[Tir2Ty],
        target_tn: &TypeName,
    ) -> Option<InterfaceTypeView> {
        let class_tn = class_qtn.clone();
        let class_loc = self.resolve_class_loc_by_type_name(&class_tn)?;
        let class_tree = file_item_tree(self.db, class_loc.file(self.db));
        let class_data = &class_tree[class_loc.id(self.db)];

        for impl_block in &class_data.implements {
            let view = self.resolve_implements_target_view(
                &impl_block.target,
                &impl_block.associated_type_bindings,
                class_loc,
            )?;
            let views = self.interface_closure_type_name_views(&view.0, &view.1, &view.2)?;
            for candidate in views {
                if candidate.0 == *target_tn {
                    return Some(Self::substitute_class_params_in_interface_view(
                        candidate,
                        &class_data.generic_params,
                        class_args,
                    ));
                }
            }
        }
        None
    }

    fn interface_view_from_registry(
        &self,
        actual_ty: &Tir2Ty,
        target_tn: &TypeName,
    ) -> Option<InterfaceTypeView> {
        let target_qtn = Self::baml_iter_qtn(target_tn.name().as_str());
        for pkg_id in self.registry_package_ids_for_interface_lookup(actual_ty, &target_qtn) {
            let default_pkg = pkg_id.name(self.db).clone();
            let registry =
                baml_compiler2_tir::interfaces::package_implements_registry(self.db, pkg_id);
            for rule in &registry.interface_impl_rules {
                let Some(bindings) = baml_compiler2_tir::interfaces::match_ty_pattern(
                    &rule.for_ty_pattern,
                    actual_ty,
                    &rule.generic_params,
                    &self.resolved_aliases.aliases,
                ) else {
                    continue;
                };
                let iface_ty =
                    baml_compiler2_tir::generics::substitute_ty(&rule.interface_ty, &bindings);
                if !registry.type_implements_interface_via_rule(
                    actual_ty,
                    &iface_ty,
                    &self.resolved_aliases.aliases,
                    |actual, bound| {
                        type_satisfies_bound(
                            self.db,
                            actual,
                            bound,
                            &self.resolved_aliases.aliases,
                            &default_pkg,
                            BLANKET_BOUND_DEPTH,
                        )
                    },
                ) {
                    continue;
                }
                let Tir2Ty::Interface(iface_qtn, iface_args, iface_assoc, _) = iface_ty else {
                    continue;
                };
                let iface_tn = iface_qtn.clone();
                let Some(views) =
                    self.interface_closure_type_name_views(&iface_tn, &iface_args, &iface_assoc)
                else {
                    continue;
                };
                if let Some(view) = views.into_iter().find(|(tn, _, _)| tn == target_tn) {
                    return Some(view);
                }
            }
        }
        None
    }

    fn registry_package_ids_for_interface_lookup(
        &self,
        actual_ty: &Tir2Ty,
        target_qtn: &QualifiedTypeName,
    ) -> Vec<PackageId<'_>> {
        let mut names = Vec::new();
        let mut seen = HashSet::new();
        let mut push_name = |name: Name| {
            if seen.insert(name.clone()) {
                names.push(name);
            }
        };

        let current_pkg =
            baml_compiler2_hir::file_package::file_package(self.db, self.file).package;
        push_name(current_pkg.clone());
        for &dep in baml_compiler2_hir::package::package_dependencies(
            self.db,
            PackageId::new(self.db, current_pkg),
        ) {
            push_name(dep.name(self.db).clone());
        }

        if let Some(actual_pkg) = Self::tir_type_package_name(actual_ty) {
            push_name(actual_pkg.clone());
            for &dep in baml_compiler2_hir::package::package_dependencies(
                self.db,
                PackageId::new(self.db, actual_pkg),
            ) {
                push_name(dep.name(self.db).clone());
            }
        }

        let target_pkg = target_qtn.package().clone();
        push_name(target_pkg.clone());
        for &dep in baml_compiler2_hir::package::package_dependencies(
            self.db,
            PackageId::new(self.db, target_pkg),
        ) {
            push_name(dep.name(self.db).clone());
        }

        names
            .into_iter()
            .map(|name| PackageId::new(self.db, name))
            .collect()
    }

    fn tir_type_package_name(ty: &Tir2Ty) -> Option<Name> {
        match ty {
            Tir2Ty::Class(qtn, _, _)
            | Tir2Ty::Interface(qtn, _, _, _)
            | Tir2Ty::Enum(qtn, _)
            | Tir2Ty::EnumVariant(qtn, _, _)
            | Tir2Ty::TypeAlias(qtn, _) => Some(qtn.package().clone()),
            Tir2Ty::Union(members, _) => {
                let mut out = None;
                for member in members {
                    let pkg = Self::tir_type_package_name(member)?;
                    match &out {
                        Some(existing) if existing != &pkg => return None,
                        None => out = Some(pkg),
                        _ => {}
                    }
                }
                out
            }
            _ => None,
        }
    }

    fn interface_view_for_tir_ty(
        &self,
        ty: &Tir2Ty,
        target_tn: &TypeName,
    ) -> Option<InterfaceTypeView> {
        match ty {
            Tir2Ty::Interface(qtn, args, assoc, _) => {
                let iface_tn = qtn.clone();
                self.interface_closure_type_name_views(&iface_tn, args, assoc)?
                    .into_iter()
                    .find(|(tn, _, _)| tn == target_tn)
            }
            Tir2Ty::Class(qtn, args, _) => self
                .interface_view_for_class_tir_ty(qtn, args, target_tn)
                .or_else(|| self.interface_view_from_registry(ty, target_tn)),
            Tir2Ty::TypeVar(name, _) => self
                .generic_param_bounds
                .get(name)
                .and_then(|bound| self.interface_view_for_tir_ty(bound, target_tn)),
            Tir2Ty::AssociatedTypeProjection { .. } => {
                let resolver =
                    baml_compiler2_tir::associated_projection::AssociatedProjectionResolver::new(
                        self.db,
                        &self.resolved_aliases.aliases,
                        &self.generic_param_bounds,
                    );
                let resolved = resolver.resolve_deep(ty);
                if &resolved != ty {
                    return self.interface_view_for_tir_ty(&resolved, target_tn);
                }
                resolver
                    .resolve_projection_bound(ty)
                    .and_then(|bound| self.interface_view_for_tir_ty(&bound, target_tn))
            }
            _ => self.interface_view_from_registry(ty, target_tn),
        }
    }

    fn iterable_view_for_tir_ty(&self, ty: &Tir2Ty) -> Option<InterfaceTypeView> {
        self.interface_view_for_tir_ty(ty, &Self::baml_iter_type_name("Iterable"))
    }

    fn lower_iterable_for_loop(
        &mut self,
        stmt_id: AstStmtId,
        binding: AstPatId,
        collection: AstExprId,
        body: AstExprId,
        iterable_view: InterfaceTypeView,
    ) {
        let saved_locals = self.locals.clone();
        let watched_depth = self.watched_locals_stack.len();
        let coll_tir_ty = self
            .expr_types
            .get(&self.expr_metadata_key(collection))
            .cloned();

        let coll_ty = self.expr_ty(collection);
        let coll_local = self.builder.temp(coll_ty);
        self.lower_expr(collection, Place::local(coll_local));

        let (iterable_tn, iterable_args, iterable_assoc) = iterable_view;
        let item_tir_ty =
            Self::associated_binding_ty(&iterable_assoc, "Item").unwrap_or_else(|| {
                Tir2Ty::Unknown {
                    attr: TyAttr::default(),
                }
            });
        let elem_ty = self.convert_tir_ty_for_runtime(&item_tir_ty);

        let iterator_tn = Self::baml_iter_type_name("Iterator");
        let iter_method = Name::new("iter");
        let iter_local = self
            .builder
            .temp(self.convert_tir_ty_for_runtime(&Tir2Ty::Interface(
                Self::baml_iter_qtn("Iterator"),
                vec![],
                iterable_assoc.clone(),
                TyAttr::default(),
            )));
        self.emit_interface_dispatch_switch(
            InterfaceDispatchCall {
                expr_id: body,
                recv_local: coll_local,
                recv_tir_ty: coll_tir_ty.as_ref(),
                iface_tn: &iterable_tn,
                iface_type_args: &iterable_args,
                iface_assoc: &iterable_assoc,
                method: &iter_method,
                args: &[],
            },
            &Place::local(iter_local),
        );

        let bb_header = self.builder.create_block();
        let bb_body = self.builder.create_block();
        let bb_exit = self.builder.create_block();

        let prev_loop = self.loop_context.take();
        self.loop_context = Some(LoopContext {
            break_target: bb_exit,
            continue_target: bb_header,
            watched_locals_depth: watched_depth,
        });

        if !self.builder.is_current_terminated() {
            self.builder.goto(bb_header);
        }

        self.builder.set_current_block(bb_header);
        let next_method = Name::new("next");
        let next_local = self.builder.temp(Ty::unknown());
        let iterator_tir_ty = Tir2Ty::Interface(
            Self::baml_iter_qtn("Iterator"),
            vec![],
            iterable_assoc.clone(),
            TyAttr::default(),
        );
        self.emit_interface_dispatch_switch(
            InterfaceDispatchCall {
                expr_id: body,
                recv_local: iter_local,
                recv_tir_ty: Some(&iterator_tir_ty),
                iface_tn: &iterator_tn,
                iface_type_args: &[],
                iface_assoc: &iterable_assoc,
                method: &next_method,
                args: &[],
            },
            &Place::local(next_local),
        );
        self.emit_is_type_branch(next_local, Self::baml_iter_done_ty(), bb_exit, bb_body);

        self.builder.set_current_block(bb_body);
        let elem_local = self.builder.declare_local(None, elem_ty, None, false);
        self.builder.assign(
            Place::local(elem_local),
            Rvalue::Use(Operand::Copy(Place::Local(next_local))),
        );
        self.bind_pattern_with_fresh_cells(elem_local, binding);
        let names: Vec<Name> = self.body.patterns[binding]
            .bound_names(&self.body.patterns)
            .into_iter()
            .cloned()
            .collect();
        for name in names {
            if let Some(&local) = self.locals.get(&name)
                && let Some(binding_id) =
                    self.binding_id_for_statement_name(stmt_id, binding, &name)
            {
                self.binding_locals.insert(binding_id, local);
            }
        }

        let body_temp = self.builder.temp(Ty::Void {
            attr: TyAttr::default(),
        });
        self.lower_expr(body, Place::local(body_temp));

        if !self.builder.is_current_terminated() {
            self.emit_unwatch_to_depth(watched_depth);
            self.builder.goto(bb_header);
        }
        self.restore_locals_after_scope(saved_locals, watched_depth);

        self.loop_context = prev_loop;
        self.builder.set_current_block(bb_exit);
    }

    /// Populate `class_fields` and `enum_variants` from a single package's items.
    ///
    /// Note: `class_type_tags` is built separately via `build_class_type_tags` to ensure
    /// the same file-iteration order as the emitter (`generate_project_bytecode`).
    fn populate_from_package(
        db: &'db dyn crate::Db,
        pkg_items: &baml_compiler2_hir::package::PackageItems<'db>,
        pkg_name: &Name,
        out: &mut PackagePopulation<'_>,
        resolved_aliases: &ResolvedAliases,
    ) {
        for (ns_names, ns) in &pkg_items.namespaces {
            // Build module_path: [pkg_name] ++ ns_names
            let mut module_path: Vec<Name> = vec![pkg_name.clone()];
            module_path.extend(ns_names.iter().cloned());

            for def in ns.types.values() {
                match def {
                    Definition::Class(class_loc) => {
                        let cfile = class_loc.file(db);
                        let citree = file_item_tree(db, cfile);
                        let class_data = &citree[class_loc.id(db)];

                        let class_qtn = QualifiedTypeName::new(
                            pkg_name.clone(),
                            ns_names.clone(),
                            class_data.name.clone(),
                        );
                        let tn = class_qtn.clone();

                        let mut fields = IndexMap::new();
                        let mut field_types = IndexMap::new();
                        let pkg_ns = baml_compiler2_hir::file_package::file_package(db, cfile)
                            .namespace_path;
                        let mut diags = Vec::new();
                        let mut idx_counter = 0usize;
                        let mut insert_field =
                            |name: &str,
                             type_expr: Option<&baml_compiler2_ast::SpannedTypeExpr>,
                             generic_params: &[Name],
                             ns: &[Name],
                             fields: &mut IndexMap<String, usize>,
                             field_types: &mut IndexMap<String, Ty>,
                             diags: &mut Vec<_>|
                             -> Option<(usize, Ty)> {
                                if let Some(idx) = fields.get(name).copied() {
                                    return field_types.get(name).cloned().map(|ty| (idx, ty));
                                }
                                let idx = idx_counter;
                                fields.insert(name.to_string(), idx);
                                idx_counter += 1;
                                let field_ty = type_expr
                                    .map(|te| {
                                        let tir_ty =
                                        baml_compiler2_tir::lower_type_expr::lower_type_expr_in_ns(
                                            db,
                                            &te.expr,
                                            pkg_items,
                                            ns,
                                            generic_params,
                                            diags,
                                        );
                                        resolved_aliases.convert(&tir_ty)
                                    })
                                    .unwrap_or(Ty::Null {
                                        attr: TyAttr::default(),
                                    });
                                field_types.insert(name.to_string(), field_ty.clone());
                                Some((idx, field_ty))
                            };

                        for field in &class_data.fields {
                            insert_field(
                                field.name.as_str(),
                                field.type_expr.as_ref(),
                                &class_data.generic_params,
                                &pkg_ns,
                                &mut fields,
                                &mut field_types,
                                &mut diags,
                            );
                        }
                        out.class_fields.insert(tn.clone(), fields);
                        out.class_field_types.insert(tn.clone(), field_types);

                        // BEP-044: register this class as an implementor of
                        // every interface its `implements` block targets,
                        // transitively through interface `requires`.
                        for impl_target in &class_data.implements {
                            let Some(iface_loc) =
                                baml_compiler2_tir::interfaces::resolve_path_to_interface(
                                    db,
                                    &impl_target.target.expr,
                                    pkg_items,
                                    &pkg_ns,
                                )
                            else {
                                continue;
                            };
                            for iface_loc in baml_compiler2_tir::interfaces::interface_closure_locs(
                                db, iface_loc, pkg_items, &pkg_ns,
                            ) {
                                let Some(iface_tn) = interface_type_name_from_loc(db, iface_loc)
                                else {
                                    continue;
                                };
                                let entry = out.interface_implementors.entry(iface_tn).or_default();
                                if !entry.contains(&tn) {
                                    entry.push(tn.clone());
                                }
                            }
                        }
                    }
                    Definition::Enum(enum_loc) => {
                        let efile = enum_loc.file(db);
                        let eitree = file_item_tree(db, efile);
                        let enum_data = &eitree[enum_loc.id(db)];
                        let enum_qtn = QualifiedTypeName::new(
                            pkg_name.clone(),
                            ns_names.clone(),
                            enum_data.name.clone(),
                        );

                        let mut variants = IndexMap::new();
                        for (idx, variant) in enum_data.variants.iter().enumerate() {
                            variants.insert(variant.name.to_string(), idx);
                        }
                        out.enum_variants.insert(enum_qtn, variants);
                    }
                    _ => {}
                }
            }
        }

        for file in compiler2_all_files(db) {
            let pkg_info = file_package(db, file);
            if pkg_info.package != *pkg_name {
                continue;
            }
            let item_tree = file_item_tree(db, file);
            for imp in &item_tree.implements_for {
                let Some(root_iface_loc) =
                    baml_compiler2_tir::interfaces::resolve_path_to_interface(
                        db,
                        &imp.interface_target.expr,
                        pkg_items,
                        &pkg_info.namespace_path,
                    )
                else {
                    continue;
                };

                let mut diags = Vec::new();
                let target_ty_tir = baml_compiler2_tir::lower_type_expr::lower_type_expr_in_ns(
                    db,
                    &imp.for_target.expr,
                    pkg_items,
                    &pkg_info.namespace_path,
                    &imp.generic_params,
                    &mut diags,
                );
                let is_generic_rule = !imp.generic_params.is_empty();

                if is_generic_rule {
                    if let baml_compiler2_tir::ty::Ty::Class(ref class_qtn, ref class_args, _) =
                        target_ty_tir
                    {
                        if class_args
                            .iter()
                            .any(|a| matches!(a, baml_compiler2_tir::ty::Ty::TypeVar(..)))
                        {
                            let root_iface_args_tir = lower_interface_target_args(
                                db,
                                &imp.interface_target.expr,
                                pkg_items,
                                &pkg_info.namespace_path,
                                &imp.generic_params,
                                &mut diags,
                            );
                            if let Some(class_tn) = class_type_name_from_qtn(db, class_qtn) {
                                register_class_for_interface_closure(
                                    db,
                                    root_iface_loc,
                                    &root_iface_args_tir,
                                    pkg_items,
                                    &pkg_info.namespace_path,
                                    &class_tn,
                                    out.interface_implementors,
                                );
                            }
                            continue;
                        }
                    }
                    if let baml_compiler2_tir::ty::Ty::TypeVar(type_var, _) = &target_ty_tir {
                        let Some(bound_ty) = imp
                            .generic_params
                            .iter()
                            .position(|param| param == type_var)
                            .and_then(|idx| imp.generic_param_bounds.get(idx))
                            .and_then(|bound| bound.as_ref())
                            .map(|bound| {
                                baml_compiler2_tir::lower_type_expr::lower_type_expr_in_ns(
                                    db,
                                    bound,
                                    pkg_items,
                                    &pkg_info.namespace_path,
                                    &imp.generic_params,
                                    &mut diags,
                                )
                            })
                        else {
                            continue;
                        };
                        if !matches!(bound_ty, baml_compiler2_tir::ty::Ty::Interface(..)) {
                            continue;
                        }
                        let root_iface_args_tir = lower_interface_target_args(
                            db,
                            &imp.interface_target.expr,
                            pkg_items,
                            &pkg_info.namespace_path,
                            &imp.generic_params,
                            &mut diags,
                        );
                        let registry = baml_compiler2_tir::interfaces::package_implements_registry(
                            db,
                            baml_compiler2_hir::package::PackageId::new(
                                db,
                                pkg_info.package.clone(),
                            ),
                        );
                        for class_qtn in registry.class_implements.keys() {
                            let actual = baml_compiler2_tir::ty::Ty::Class(
                                class_qtn.clone(),
                                Vec::new(),
                                baml_compiler2_tir::ty::TyAttr::default(),
                            );
                            if !registry.type_implements_interface_via_rule(
                                &actual,
                                &bound_ty,
                                &resolved_aliases.aliases,
                                |nested_actual, nested_bound| {
                                    type_satisfies_bound(
                                        db,
                                        nested_actual,
                                        nested_bound,
                                        &resolved_aliases.aliases,
                                        &pkg_info.package,
                                        BLANKET_BOUND_DEPTH,
                                    )
                                },
                            ) {
                                continue;
                            }
                            let class_tn = class_qtn.clone();
                            register_class_for_interface_closure(
                                db,
                                root_iface_loc,
                                &root_iface_args_tir,
                                pkg_items,
                                &pkg_info.namespace_path,
                                &class_tn,
                                out.interface_implementors,
                            );
                        }
                        continue;
                    }
                }

                let target_ty = resolved_aliases.convert(&target_ty_tir);
                if matches!(target_ty, Ty::Class(..)) {
                    continue;
                }

                let root_iface_args_tir = lower_interface_target_args(
                    db,
                    &imp.interface_target.expr,
                    pkg_items,
                    &pkg_info.namespace_path,
                    &imp.generic_params,
                    &mut diags,
                );
                let root_iface_assoc_tir = lower_interface_target_associated_bindings(
                    db,
                    &imp.interface_target.expr,
                    &imp.associated_type_bindings,
                    pkg_items,
                    &pkg_info.namespace_path,
                    &imp.generic_params,
                    &mut diags,
                );

                for (iface_loc, iface_args, iface_assoc) in
                    baml_compiler2_tir::interfaces::interface_closure_locs_with_args_and_assoc(
                        db,
                        root_iface_loc,
                        &root_iface_args_tir,
                        &root_iface_assoc_tir,
                        pkg_items,
                        &pkg_info.namespace_path,
                    )
                {
                    let Some(iface_tn) = interface_type_name_from_loc(db, iface_loc) else {
                        continue;
                    };
                    let entry = out.interface_type_implementors.entry(iface_tn).or_default();
                    if !entry.iter().any(|implementor| {
                        implementor.runtime_ty == target_ty
                            && implementor.iface_args == iface_args
                            && implementor.iface_assoc == iface_assoc
                    }) {
                        entry.push(InterfaceTypeImplementor {
                            runtime_ty: target_ty.clone(),
                            tir_ty: target_ty_tir.clone(),
                            iface_args,
                            iface_assoc,
                        });
                    }
                }
            }
        }
    }

    /// Build `class_type_tags` by iterating `compiler2_all_files` in the same order as the
    /// emitter (`generate_project_bytecode` in `baml_compiler2_emit`). This guarantees that
    /// the integer type tags stored in Switch arms exactly match the `class.type_tag` values
    /// assigned to runtime Class objects.
    fn build_class_type_tags(db: &'db dyn crate::Db) -> IndexMap<TypeName, i64> {
        let all_files = compiler2_all_files(db);
        let mut class_type_tags: IndexMap<TypeName, i64> = IndexMap::new();
        let mut class_type_tag_counter = 0i64;

        for file in &all_files {
            let item_tree = file_item_tree(db, *file);
            let pkg_info = file_package(db, *file);

            // Build module_path: [package] ++ namespace_path
            let mut module_path: Vec<Name> = vec![pkg_info.package.clone()];
            module_path.extend(pkg_info.namespace_path.iter().cloned());

            for class_data in item_tree.classes.values() {
                let class_qtn = QualifiedTypeName::new(
                    pkg_info.package.clone(),
                    pkg_info.namespace_path.clone(),
                    class_data.name.clone(),
                );
                let tn = class_qtn.clone();
                let type_tag = baml_type::typetag::CLASS_BASE + class_type_tag_counter;
                class_type_tag_counter += 1;
                // Use entry to avoid overwriting if the same class appears via multiple paths
                // (e.g., both FQ and short names). First encounter wins — consistent with emit.rs.
                class_type_tags.entry(tn).or_insert(type_tag);
            }
        }

        class_type_tags
    }

    fn new(
        db: &'db dyn crate::Db,
        func_loc: FunctionLoc<'db>,
        expr_body: AstExprBody,
        source_map: Option<AstSourceMap>,
        opt: crate::OptLevel,
    ) -> Self {
        let file = func_loc.file(db);

        // --- Resolve FunctionLoc → FileScopeId via span ---
        let item_tree = file_item_tree(db, file);
        let func_data = &item_tree[func_loc.id(db)];
        let func_span = func_data.span;

        let index = file_semantic_index(db, file);
        // For synthesized functions whose span is `0..0` (e.g. `$init_test_N`),
        // `scope_at_offset` may return a descendant Lambda scope instead of the
        // Function scope itself, because all synthesized expressions share span
        // `0..0` and the descendant search finds a matching lambda first.
        // Avoid this by searching explicitly for a `ScopeKind::Function` scope
        // with the correct name and span before falling back to `scope_at_offset`.
        let func_scope_id: FileScopeId = index
            .scopes
            .iter()
            .enumerate()
            .find_map(|(i, scope)| {
                if scope.kind == baml_compiler2_hir::scope::ScopeKind::Function
                    && scope.range == func_span
                    && scope.name.as_ref() == Some(&func_data.name)
                {
                    #[allow(clippy::cast_possible_truncation)]
                    Some(FileScopeId::new(i as u32))
                } else {
                    None
                }
            })
            .unwrap_or_else(|| index.scope_at_offset(func_span.start(), Some(&func_data.name)));

        // --- Eagerly aggregate expr_types, pat_types, resolutions, exhaustive_matches, path_root_types, and path_member_resolutions from all scopes ---
        let mut expr_types: FxHashMap<ExprMetadataKey, Tir2Ty> = FxHashMap::default();
        let mut pat_types: FxHashMap<PatMetadataKey, Tir2Ty> = FxHashMap::default();
        let mut resolutions: FxHashMap<
            ExprMetadataKey,
            baml_compiler2_tir::inference::MemberResolution<'db>,
        > = FxHashMap::default();
        let mut exhaustive_matches: rustc_hash::FxHashSet<ExprMetadataKey> =
            rustc_hash::FxHashSet::default();
        let mut path_root_types: FxHashMap<ExprMetadataKey, Tir2Ty> = FxHashMap::default();
        let mut path_segment_types: FxHashMap<(MetadataScope, AstExprId, usize), Tir2Ty> =
            FxHashMap::default();
        let mut path_member_resolutions: FxHashMap<
            ExprMetadataKey,
            Vec<baml_compiler2_tir::inference::MemberResolution<'db>>,
        > = FxHashMap::default();
        let mut call_plans: FxHashMap<ExprMetadataKey, baml_compiler2_tir::inference::CallPlan> =
            FxHashMap::default();
        let mut function_coercions: FxHashMap<
            ExprMetadataKey,
            baml_compiler2_tir::inference::FunctionCoercion,
        > = FxHashMap::default();

        let merge_scope = |fsi: FileScopeId,
                           expr_types: &mut FxHashMap<ExprMetadataKey, Tir2Ty>,
                           pat_types: &mut FxHashMap<PatMetadataKey, Tir2Ty>,
                           resolutions: &mut FxHashMap<
            ExprMetadataKey,
            baml_compiler2_tir::inference::MemberResolution<'db>,
        >,
                           exhaustive_matches: &mut rustc_hash::FxHashSet<ExprMetadataKey>,
                           path_root_types: &mut FxHashMap<ExprMetadataKey, Tir2Ty>,
                           path_segment_types: &mut FxHashMap<
            (MetadataScope, AstExprId, usize),
            Tir2Ty,
        >,
                           path_member_resolutions: &mut FxHashMap<
            ExprMetadataKey,
            Vec<baml_compiler2_tir::inference::MemberResolution<'db>>,
        >,
                           call_plans: &mut FxHashMap<
            ExprMetadataKey,
            baml_compiler2_tir::inference::CallPlan,
        >,
                           function_coercions: &mut FxHashMap<
            ExprMetadataKey,
            baml_compiler2_tir::inference::FunctionCoercion,
        >| {
            let scope_id = index.scope_ids[fsi.index() as usize];
            let inference = infer_scope_types(db, scope_id);
            let body_scope = MetadataScope::Body(fsi);
            for (&expr_id, ty) in inference.iter_expressions() {
                expr_types.insert((body_scope, expr_id), ty.clone());
            }
            for (&pat_id, ty) in inference.iter_bindings() {
                pat_types.insert((body_scope, pat_id), ty.clone());
            }
            for (&expr_id, res) in inference.iter_resolutions() {
                resolutions.insert((body_scope, expr_id), res.clone());
            }
            for &expr_id in inference.iter_exhaustive_matches() {
                exhaustive_matches.insert((body_scope, expr_id));
            }
            for (&expr_id, ty) in inference.iter_path_root_types() {
                path_root_types.insert((body_scope, expr_id), ty.clone());
            }
            for (&(expr_id, seg_idx), ty) in inference.iter_path_segment_types() {
                path_segment_types.insert((body_scope, expr_id, seg_idx), ty.clone());
            }
            for (&expr_id, member_resolutions) in inference.iter_path_member_resolutions() {
                path_member_resolutions.insert((body_scope, expr_id), member_resolutions.clone());
            }
            for (&expr_id, plan) in inference.iter_call_plans() {
                call_plans.insert((body_scope, expr_id), plan.clone());
            }
            for (&expr_id, coercion) in inference.iter_function_coercions() {
                function_coercions.insert((body_scope, expr_id), coercion.clone());
            }

            let default_scope = MetadataScope::ParameterDefault(fsi);
            for (&expr_id, ty) in inference.iter_default_expressions() {
                expr_types.insert((default_scope, expr_id), ty.clone());
            }
            for (&pat_id, ty) in inference.iter_default_bindings() {
                pat_types.insert((default_scope, pat_id), ty.clone());
            }
            for (&expr_id, res) in inference.iter_default_resolutions() {
                resolutions.insert((default_scope, expr_id), res.clone());
            }
            for &expr_id in inference.iter_default_exhaustive_matches() {
                exhaustive_matches.insert((default_scope, expr_id));
            }
            for (&expr_id, ty) in inference.iter_default_path_root_types() {
                path_root_types.insert((default_scope, expr_id), ty.clone());
            }
            for (&(expr_id, seg_idx), ty) in inference.iter_default_path_segment_types() {
                path_segment_types.insert((default_scope, expr_id, seg_idx), ty.clone());
            }
            for (&expr_id, member_resolutions) in inference.iter_default_path_member_resolutions() {
                path_member_resolutions
                    .insert((default_scope, expr_id), member_resolutions.clone());
            }
            for (&expr_id, plan) in inference.iter_default_call_plans() {
                call_plans.insert((default_scope, expr_id), plan.clone());
            }
            for (&expr_id, coercion) in inference.iter_default_function_coercions() {
                function_coercions.insert((default_scope, expr_id), coercion.clone());
            }
        };

        // Include the function scope itself
        merge_scope(
            func_scope_id,
            &mut expr_types,
            &mut pat_types,
            &mut resolutions,
            &mut exhaustive_matches,
            &mut path_root_types,
            &mut path_segment_types,
            &mut path_member_resolutions,
            &mut call_plans,
            &mut function_coercions,
        );

        // Include all descendant scopes (blocks, lambdas, etc.)
        let func_scope = &index.scopes[func_scope_id.index() as usize];
        let desc_start = func_scope.descendants.start.index();
        let desc_end = func_scope.descendants.end.index();
        for raw_idx in desc_start..desc_end {
            merge_scope(
                FileScopeId::new(raw_idx),
                &mut expr_types,
                &mut pat_types,
                &mut resolutions,
                &mut exhaustive_matches,
                &mut path_root_types,
                &mut path_segment_types,
                &mut path_member_resolutions,
                &mut call_plans,
                &mut function_coercions,
            );
        }

        // --- Build class_fields / enum_variants from PackageItems ---
        let pkg_info = file_package(db, file);
        let pkg_id = PackageId::new(db, pkg_info.package.clone());
        let pkg_items_for_bounds = package_items(db, pkg_id);
        let mut bound_param_names = Vec::new();
        let mut bound_exprs = Vec::new();
        if let Some(imp) = item_tree
            .implements_for
            .iter()
            .find(|imp| imp.methods.contains(&func_loc.id(db)))
        {
            bound_param_names.extend(imp.generic_params.iter().cloned());
            bound_exprs.extend(imp.generic_param_bounds.iter().cloned());
        } else if let Some(parent_idx) = func_scope.parent {
            let parent = &index.scopes[parent_idx.index() as usize];
            if matches!(parent.kind, baml_compiler2_hir::scope::ScopeKind::Class)
                && let Some(type_name) = &parent.name
            {
                if let Some(class_data) = item_tree
                    .classes
                    .values()
                    .find(|class_data| class_data.name == *type_name)
                {
                    bound_param_names.extend(class_data.generic_params.iter().cloned());
                    bound_exprs.extend(class_data.generic_param_bounds.iter().cloned());
                } else if let Some(iface_data) = item_tree
                    .interfaces
                    .values()
                    .find(|iface_data| iface_data.name == *type_name)
                {
                    bound_param_names.extend(iface_data.generic_params.iter().cloned());
                    bound_exprs.extend(iface_data.generic_param_bounds.iter().cloned());
                    bound_param_names.extend(
                        iface_data
                            .associated_types
                            .iter()
                            .map(|assoc| assoc.name.clone()),
                    );
                    bound_exprs.extend(
                        iface_data
                            .associated_types
                            .iter()
                            .map(|assoc| assoc.bound.as_ref().map(|bound| bound.expr.clone())),
                    );
                }
            }
        }
        bound_param_names.extend(func_data.generic_params.iter().cloned());
        bound_exprs.extend(func_data.generic_param_bounds.iter().cloned());
        let all_generic_params = bound_param_names.clone();
        let mut generic_param_bounds: FxHashMap<Name, Tir2Ty> = FxHashMap::default();
        for (idx, name) in bound_param_names.iter().enumerate() {
            let Some(Some(bound_te)) = bound_exprs.get(idx) else {
                continue;
            };
            let mut diags = Vec::new();
            let bound_ty = baml_compiler2_tir::lower_type_expr::lower_type_expr_in_ns(
                db,
                bound_te,
                pkg_items_for_bounds,
                &pkg_info.namespace_path,
                &all_generic_params,
                &mut diags,
            );
            if diags.is_empty() {
                generic_param_bounds.insert(name.clone(), bound_ty);
            }
        }
        // BEP-044 Self-as-type-variable: an interface default method's `self` is
        // a `Self` type variable bound by the interface (matching the TIR
        // typing in `inference.rs`). Registering the bound lets member access on
        // `self` dispatch through the interface — `interface_dispatch_target_for_tir_ty`
        // already follows type-var bounds — so default methods keep dispatching
        // through the concrete implementor.
        for iface in item_tree.interfaces.values() {
            if iface.default_methods.contains(&func_loc.id(db))
                && let Some(def) =
                    pkg_items_for_bounds.lookup_type(&pkg_info.namespace_path, &iface.name)
            {
                let qtn = baml_compiler2_tir::lower_type_expr::qualify_def(db, def, &iface.name);
                let args = iface
                    .generic_params
                    .iter()
                    .map(|p| Tir2Ty::TypeVar(p.clone(), baml_compiler2_tir::ty::TyAttr::default()))
                    .collect();
                let associated_bindings = iface
                    .associated_types
                    .iter()
                    .map(|assoc| {
                        (
                            assoc.name.clone(),
                            Tir2Ty::TypeVar(
                                assoc.name.clone(),
                                baml_compiler2_tir::ty::TyAttr::default(),
                            ),
                        )
                    })
                    .collect();
                generic_param_bounds.insert(
                    Name::new("Self"),
                    Tir2Ty::Interface(
                        qtn,
                        args,
                        associated_bindings,
                        baml_compiler2_tir::ty::TyAttr::default(),
                    ),
                );
                break;
            }
        }

        // Class/enum/interface schema + resolved aliases, memoized per package
        // (was rebuilt — and every class field re-lowered — per function).
        let pkg_data = package_lowering_data(db, pkg_id);

        // Build class_type_tags using the same file-iteration order as the emitter,
        // so that switch arms get the same integer tags as runtime class.type_tag fields.
        let class_type_tags = Self::build_class_type_tags(db);

        // --- Determine arity from function signature ---
        let sig = baml_compiler2_ppir::function_signature(db, func_loc);
        let arity = sig.params.len();

        // Detect if this function is a class method by checking the parent scope.
        // If so, qualify the function name as "ClassName.MethodName".
        let func_scope = &index.scopes[func_scope_id.index() as usize];
        let func_name = if let Some(parent_idx) = func_scope.parent {
            let parent = &index.scopes[parent_idx.index() as usize];
            if matches!(parent.kind, baml_compiler2_hir::scope::ScopeKind::Class) {
                if let Some(ref class_name) = parent.name {
                    Name::new(format!(
                        "{}.{}",
                        class_name.as_str(),
                        func_data.name.as_str()
                    ))
                } else {
                    func_data.name.clone()
                }
            } else {
                func_data.name.clone()
            }
        } else {
            func_data.name.clone()
        };

        LoweringContext {
            db,
            builder: MirBuilder::new(func_name, arity),
            locals: HashMap::new(),
            binding_locals: HashMap::new(),
            loop_context: None,
            catch_context: None,
            exit_block: BlockId(0), // placeholder; overwritten in lower_function_body
            expr_types,
            pat_types,
            resolutions,
            exhaustive_matches,
            path_root_types,
            path_segment_types,
            path_member_resolutions,
            call_plans,
            function_coercions,
            generic_param_bounds,
            lambda_param_tir_types: FxHashMap::default(),
            current_scope: func_scope_id,
            current_metadata_scope: MetadataScope::Body(func_scope_id),
            body: expr_body,
            source_map,
            file,
            func_loc: Some(func_loc),
            source_param_scope: Some(func_scope_id),
            scope_func_name: Some(func_data.name.clone()),
            class_fields: &pkg_data.class_fields,
            class_field_types: &pkg_data.class_field_types,
            enum_variants: &pkg_data.enum_variants,
            class_type_tags,
            interface_implementors: &pkg_data.interface_implementors,
            interface_type_implementors: &pkg_data.interface_type_implementors,
            pending_lambdas: Vec::new(),
            lambda_generic_params: Vec::new(),
            capture_indices: None,
            transitive_captures_needed: Vec::new(),
            resolved_aliases: &pkg_data.resolved_aliases,
            watched_locals_stack: Vec::new(),
            synthetic_name_counts: HashMap::new(),
            chain_null_exits: Vec::new(),
            opt,
        }
    }

    /// Create a lowering context for a top-level let binding.
    ///
    /// The let binding has no parameters — arity 0, no `func_loc`.
    /// Type information is gathered from the `ScopeKind::Let` scope.
    fn new_for_let(
        db: &'db dyn crate::Db,
        let_loc: LetLoc<'db>,
        expr_body: AstExprBody,
        source_map: Option<AstSourceMap>,
        opt: crate::OptLevel,
    ) -> Self {
        let file = let_loc.file(db);

        // --- Resolve LetLoc → FileScopeId via span ---
        let item_tree = file_item_tree(db, file);
        let let_data = &item_tree[let_loc.id(db)];
        let let_span = let_data.span;
        let let_name = let_data.name.clone();

        let index = file_semantic_index(db, file);
        let let_scope_id: FileScopeId = index.scope_at_offset(let_span.start(), Some(&let_name));

        // --- Eagerly aggregate expr_types, pat_types, resolutions, path_root_types, path_member_resolutions from let scope ---
        let mut expr_types: FxHashMap<ExprMetadataKey, Tir2Ty> = FxHashMap::default();
        let mut pat_types: FxHashMap<PatMetadataKey, Tir2Ty> = FxHashMap::default();
        let mut resolutions: FxHashMap<
            ExprMetadataKey,
            baml_compiler2_tir::inference::MemberResolution<'db>,
        > = FxHashMap::default();
        let mut exhaustive_matches: rustc_hash::FxHashSet<ExprMetadataKey> =
            rustc_hash::FxHashSet::default();
        let mut path_root_types: FxHashMap<ExprMetadataKey, Tir2Ty> = FxHashMap::default();
        let mut path_segment_types: FxHashMap<(MetadataScope, AstExprId, usize), Tir2Ty> =
            FxHashMap::default();
        let mut path_member_resolutions: FxHashMap<
            ExprMetadataKey,
            Vec<baml_compiler2_tir::inference::MemberResolution<'db>>,
        > = FxHashMap::default();
        let mut call_plans: FxHashMap<ExprMetadataKey, baml_compiler2_tir::inference::CallPlan> =
            FxHashMap::default();
        let mut function_coercions: FxHashMap<
            ExprMetadataKey,
            baml_compiler2_tir::inference::FunctionCoercion,
        > = FxHashMap::default();

        let merge_scope = |fsi: FileScopeId,
                           expr_types: &mut FxHashMap<ExprMetadataKey, Tir2Ty>,
                           pat_types: &mut FxHashMap<PatMetadataKey, Tir2Ty>,
                           resolutions: &mut FxHashMap<
            ExprMetadataKey,
            baml_compiler2_tir::inference::MemberResolution<'db>,
        >,
                           exhaustive_matches: &mut rustc_hash::FxHashSet<ExprMetadataKey>,
                           path_root_types: &mut FxHashMap<ExprMetadataKey, Tir2Ty>,
                           path_segment_types: &mut FxHashMap<
            (MetadataScope, AstExprId, usize),
            Tir2Ty,
        >,
                           path_member_resolutions: &mut FxHashMap<
            ExprMetadataKey,
            Vec<baml_compiler2_tir::inference::MemberResolution<'db>>,
        >,
                           call_plans: &mut FxHashMap<
            ExprMetadataKey,
            baml_compiler2_tir::inference::CallPlan,
        >,
                           function_coercions: &mut FxHashMap<
            ExprMetadataKey,
            baml_compiler2_tir::inference::FunctionCoercion,
        >| {
            let scope_id = index.scope_ids[fsi.index() as usize];
            let inference = infer_scope_types(db, scope_id);
            let body_scope = MetadataScope::Body(fsi);
            for (&expr_id, ty) in inference.iter_expressions() {
                expr_types.insert((body_scope, expr_id), ty.clone());
            }
            for (&pat_id, ty) in inference.iter_bindings() {
                pat_types.insert((body_scope, pat_id), ty.clone());
            }
            for (&expr_id, res) in inference.iter_resolutions() {
                resolutions.insert((body_scope, expr_id), res.clone());
            }
            for &expr_id in inference.iter_exhaustive_matches() {
                exhaustive_matches.insert((body_scope, expr_id));
            }
            for (&expr_id, ty) in inference.iter_path_root_types() {
                path_root_types.insert((body_scope, expr_id), ty.clone());
            }
            for (&(expr_id, seg_idx), ty) in inference.iter_path_segment_types() {
                path_segment_types.insert((body_scope, expr_id, seg_idx), ty.clone());
            }
            for (&expr_id, member_resolutions) in inference.iter_path_member_resolutions() {
                path_member_resolutions.insert((body_scope, expr_id), member_resolutions.clone());
            }
            for (&expr_id, plan) in inference.iter_call_plans() {
                call_plans.insert((body_scope, expr_id), plan.clone());
            }
            for (&expr_id, coercion) in inference.iter_function_coercions() {
                function_coercions.insert((body_scope, expr_id), coercion.clone());
            }

            let default_scope = MetadataScope::ParameterDefault(fsi);
            for (&expr_id, ty) in inference.iter_default_expressions() {
                expr_types.insert((default_scope, expr_id), ty.clone());
            }
            for (&pat_id, ty) in inference.iter_default_bindings() {
                pat_types.insert((default_scope, pat_id), ty.clone());
            }
            for (&expr_id, res) in inference.iter_default_resolutions() {
                resolutions.insert((default_scope, expr_id), res.clone());
            }
            for &expr_id in inference.iter_default_exhaustive_matches() {
                exhaustive_matches.insert((default_scope, expr_id));
            }
            for (&expr_id, ty) in inference.iter_default_path_root_types() {
                path_root_types.insert((default_scope, expr_id), ty.clone());
            }
            for (&(expr_id, seg_idx), ty) in inference.iter_default_path_segment_types() {
                path_segment_types.insert((default_scope, expr_id, seg_idx), ty.clone());
            }
            for (&expr_id, member_resolutions) in inference.iter_default_path_member_resolutions() {
                path_member_resolutions
                    .insert((default_scope, expr_id), member_resolutions.clone());
            }
            for (&expr_id, plan) in inference.iter_default_call_plans() {
                call_plans.insert((default_scope, expr_id), plan.clone());
            }
            for (&expr_id, coercion) in inference.iter_default_function_coercions() {
                function_coercions.insert((default_scope, expr_id), coercion.clone());
            }
        };

        // Include the let scope itself
        merge_scope(
            let_scope_id,
            &mut expr_types,
            &mut pat_types,
            &mut resolutions,
            &mut exhaustive_matches,
            &mut path_root_types,
            &mut path_segment_types,
            &mut path_member_resolutions,
            &mut call_plans,
            &mut function_coercions,
        );

        // Include all descendant scopes (blocks, closures within the initializer)
        let let_scope = &index.scopes[let_scope_id.index() as usize];
        let desc_start = let_scope.descendants.start.index();
        let desc_end = let_scope.descendants.end.index();
        for raw_idx in desc_start..desc_end {
            merge_scope(
                FileScopeId::new(raw_idx),
                &mut expr_types,
                &mut pat_types,
                &mut resolutions,
                &mut exhaustive_matches,
                &mut path_root_types,
                &mut path_segment_types,
                &mut path_member_resolutions,
                &mut call_plans,
                &mut function_coercions,
            );
        }

        // --- Build class_fields / enum_variants from PackageItems ---
        let pkg_id = PackageId::new(db, file_package(db, file).package);

        // Class/enum/interface schema + resolved aliases, memoized per package
        // (was rebuilt — and every class field re-lowered — per let binding).
        let pkg_data = package_lowering_data(db, pkg_id);

        // Build class_type_tags using the same file-iteration order as the emitter,
        // so that switch arms get the same integer tags as runtime class.type_tag fields.
        let class_type_tags = Self::build_class_type_tags(db);

        LoweringContext {
            db,
            builder: MirBuilder::new(let_name.clone(), 0),
            locals: HashMap::new(),
            binding_locals: HashMap::new(),
            loop_context: None,
            catch_context: None,
            exit_block: BlockId(0), // placeholder; overwritten in lower_let_body_inner
            expr_types,
            pat_types,
            resolutions,
            exhaustive_matches,
            path_root_types,
            path_segment_types,
            path_member_resolutions,
            call_plans,
            function_coercions,
            generic_param_bounds: FxHashMap::default(),
            lambda_param_tir_types: FxHashMap::default(),
            current_scope: let_scope_id,
            current_metadata_scope: MetadataScope::Body(let_scope_id),
            body: expr_body,
            source_map,
            file,
            func_loc: None,
            source_param_scope: None,
            scope_func_name: Some(let_name),
            class_fields: &pkg_data.class_fields,
            class_field_types: &pkg_data.class_field_types,
            enum_variants: &pkg_data.enum_variants,
            class_type_tags,
            interface_implementors: &pkg_data.interface_implementors,
            interface_type_implementors: &pkg_data.interface_type_implementors,
            resolved_aliases: &pkg_data.resolved_aliases,
            watched_locals_stack: Vec::new(),
            synthetic_name_counts: HashMap::new(),
            pending_lambdas: Vec::new(),
            lambda_generic_params: Vec::new(),
            capture_indices: None,
            transitive_captures_needed: Vec::new(),
            chain_null_exits: Vec::new(),
            opt,
        }
    }

    fn scope_is_descendant_or_self(
        index: &baml_compiler2_hir::semantic_index::FileSemanticIndex<'_>,
        scope_id: FileScopeId,
        ancestor_id: FileScopeId,
    ) -> bool {
        let mut current = Some(scope_id);
        while let Some(id) = current {
            if id == ancestor_id {
                return true;
            }
            current = index.scopes[id.index() as usize].parent;
        }
        false
    }

    fn binding_id_for_pattern_site_name(
        &self,
        pattern: AstPatId,
        site: DefinitionSite,
        name: &Name,
    ) -> Option<BindingId> {
        let index = file_semantic_index(self.db, self.file);
        let pattern_span = self
            .source_map
            .as_ref()
            .map(|source_map| source_map.pattern_span(pattern));

        for (scope_idx, bindings) in index.scope_bindings.iter().enumerate() {
            let scope_id = FileScopeId::new(u32::try_from(scope_idx).expect("scope id overflow"));
            if !Self::scope_is_descendant_or_self(index, scope_id, self.current_scope) {
                continue;
            }
            for (binding_idx, binding) in bindings.bindings.iter().enumerate() {
                let pattern_matches_name_range = pattern_span.is_none_or(|span| {
                    span == binding.name_range
                        || (span.start() <= binding.name_range.start()
                            && binding.name_range.end() <= span.end())
                });
                if binding.site == site
                    && binding.pattern == pattern
                    && binding.name == *name
                    && pattern_matches_name_range
                {
                    return Some(BindingId::local(scope_id, binding_idx));
                }
            }
        }
        None
    }

    fn any_pattern_binding_is_captured(&self, pattern: AstPatId, site: DefinitionSite) -> bool {
        let index = file_semantic_index(self.db, self.file);
        for (scope_idx, bindings) in index.scope_bindings.iter().enumerate() {
            let scope_id = FileScopeId::new(u32::try_from(scope_idx).expect("scope id overflow"));
            if !Self::scope_is_descendant_or_self(index, scope_id, self.current_scope) {
                continue;
            }
            for (binding_idx, binding) in bindings.bindings.iter().enumerate() {
                if binding.site == site && binding.pattern == pattern {
                    let binding_id = BindingId::local(scope_id, binding_idx);
                    if bindings.captured_bindings.contains(&binding_id) {
                        return true;
                    }
                }
            }
        }
        false
    }

    fn binding_id_for_statement_name(
        &self,
        stmt_id: AstStmtId,
        pattern: AstPatId,
        name: &Name,
    ) -> Option<BindingId> {
        self.binding_id_for_pattern_site_name(pattern, DefinitionSite::Statement(stmt_id), name)
    }

    fn record_pattern_binding_local(&mut self, pattern: AstPatId, name: &Name, local: Local) {
        if let Some(binding_id) = self.binding_id_for_pattern_site_name(
            pattern,
            DefinitionSite::PatternBinding(pattern),
            name,
        ) {
            self.binding_locals.insert(binding_id, local);
        }
    }

    fn pattern_binding_is_captured(&self, pattern: AstPatId) -> bool {
        self.any_pattern_binding_is_captured(pattern, DefinitionSite::PatternBinding(pattern))
    }

    fn binding_id_for_name_at(&self, expr_id: AstExprId, name: &Name) -> Option<BindingId> {
        let index = file_semantic_index(self.db, self.file);
        let (scope_id, offset) = if let Some(source_map) = self.source_map.as_ref() {
            let offset = source_map.expr_span(expr_id).start();
            (
                index.scope_at_offset(offset, self.scope_func_name.as_ref()),
                offset,
            )
        } else {
            // The source-map-less branch is only valid for **synthesized**
            // expressions emitted by the lowering itself (e.g. for-loop index
            // increments, capture forwarding, init function bodies). The
            // fallback uses `current_scope` and the scope's end offset, which
            // is correct for synthesized refs at the end of the current scope
            // but would silently pick the post-shadow binding for a
            // user-written name lowered without a source map.
            //
            // If you find yourself adding a user-visible expression that
            // hits this path: the right fix is to thread a `BindingId`
            // through to the call site, not to widen this fallback.
            let scope_id = self.current_scope;
            let offset = index.scopes[scope_id.index() as usize].range.end();
            (scope_id, offset)
        };
        index.visible_binding_at(scope_id, offset, name)
    }

    fn capture_index_for_name_at(&self, expr_id: AstExprId, name: &Name) -> Option<usize> {
        let binding_id = self.binding_id_for_name_at(expr_id, name)?;
        self.capture_indices
            .as_ref()
            .and_then(|captures| captures.get(&binding_id).copied())
    }

    /// Emit `unwatch` ops for every watched local at index `[watched_depth..]`
    /// of `watched_locals_stack`, in reverse declaration order.
    ///
    /// This is the single emitter for unwatch sequences. All scope-exit
    /// paths go through it:
    ///   - normal block fallthrough: `lower_scoped_block` (depth = entry stack len)
    ///   - normal `for`-body fallthrough (depth = entry stack len)
    ///   - normal match/catch arm-body fallthrough (depth = arm-entry stack len)
    ///   - `break` / `continue` (depth = `loop_context.watched_locals_depth`)
    ///   - `return` / `throw` (depth = 0 — the stack is swapped at lambda
    ///     boundaries, so 0 means "everything in the enclosing function")
    ///
    /// Does NOT truncate the stack — callers that own the scope are
    /// responsible for truncating via `restore_locals_after_scope`. Divergent
    /// callers (break/continue/return/throw) leave the stack alone because a
    /// dead block follows the divergent terminator.
    fn emit_unwatch_to_depth(&mut self, watched_depth: usize) {
        let watched = self.watched_locals_stack[watched_depth..].to_vec();
        for local in watched.into_iter().rev() {
            self.builder.unwatch(local);
        }
    }

    fn restore_locals_after_scope(
        &mut self,
        saved_locals: HashMap<Name, Local>,
        watched_depth: usize,
    ) {
        self.watched_locals_stack.truncate(watched_depth);
        self.locals = saved_locals;
    }

    fn restore_active_locals(&mut self, saved_locals: HashMap<Name, Local>) {
        self.locals = saved_locals;
    }

    fn mark_captured_locals_in_scope_tree(&mut self, root_scope: FileScopeId) {
        let index = file_semantic_index(self.db, self.file);
        let root = &index.scopes[root_scope.index() as usize];
        let start = root_scope.index();
        let end = root.descendants.end.index();

        for raw_idx in start..end {
            let scope_id = FileScopeId::new(raw_idx);
            let Some(scope_bindings) = index.scope_bindings.get(scope_id.index() as usize) else {
                continue;
            };
            for binding_id in &scope_bindings.captured_bindings {
                if let Some(&local) = self.binding_locals.get(binding_id) {
                    self.builder.local_decl_mut(local).is_captured = true;
                }
            }
        }
    }

    /// Get the `baml_type::Ty` for an expression by looking up in the aggregated map
    /// and converting from TIR Ty. Uses `current_metadata_scope` as the arena namespace.
    fn expr_metadata_key(&self, expr_id: AstExprId) -> ExprMetadataKey {
        (self.current_metadata_scope, expr_id)
    }

    fn pat_metadata_key(&self, pat_id: AstPatId) -> PatMetadataKey {
        (self.current_metadata_scope, pat_id)
    }

    fn erase_bound_typevars_for_runtime(&self, ty: &Tir2Ty) -> Tir2Ty {
        baml_compiler2_tir::generics::erase_typevars_matching(ty, &|name| {
            self.generic_param_bounds.contains_key(name)
        })
    }

    fn convert_tir_ty_for_runtime(&self, ty: &Tir2Ty) -> Ty {
        let erased = self.erase_bound_typevars_for_runtime(ty);
        convert_tir2_ty(&erased, self.resolved_aliases)
    }

    fn interface_dispatch_target_for_tir_ty(&self, ty: &Tir2Ty) -> Option<InterfaceTypeView> {
        match ty {
            Tir2Ty::Interface(qtn, type_args, associated_bindings, _) => {
                Some((qtn.clone(), type_args.clone(), associated_bindings.clone()))
            }
            Tir2Ty::TypeVar(name, _) => self
                .generic_param_bounds
                .get(name)
                .and_then(|bound| self.interface_dispatch_target_for_tir_ty(bound)),
            Tir2Ty::Class(qtn, type_args, _) => {
                let tn = qtn.clone();
                self.interface_implementors
                    .contains_key(&tn)
                    .then(|| (tn, type_args.clone(), Vec::new()))
            }
            Tir2Ty::AssociatedTypeProjection { .. } => {
                let resolver =
                    baml_compiler2_tir::associated_projection::AssociatedProjectionResolver::new(
                        self.db,
                        &self.resolved_aliases.aliases,
                        &self.generic_param_bounds,
                    );
                let resolved = resolver.resolve_deep(ty);
                if &resolved != ty {
                    return self.interface_dispatch_target_for_tir_ty(&resolved);
                }
                resolver
                    .resolve_projection_bound(ty)
                    .and_then(|bound| self.interface_dispatch_target_for_tir_ty(&bound))
            }
            _ => None,
        }
    }

    fn interface_dispatch_target_for_expr(&self, expr_id: AstExprId) -> Option<InterfaceTypeView> {
        self.source_param_interface_view_for_expr(expr_id)
            .or_else(|| {
                self.expr_types
                    .get(&self.expr_metadata_key(expr_id))
                    .and_then(|ty| self.interface_dispatch_target_for_tir_ty(ty))
            })
            .or_else(|| {
                self.self_typevar_for_expr(expr_id)
                    .and_then(|ty| self.interface_dispatch_target_for_tir_ty(&ty))
            })
            .or_else(|| self.upcast_target_interface_view(expr_id))
    }

    fn source_param_interface_view_for_expr(
        &self,
        expr_id: AstExprId,
    ) -> Option<InterfaceTypeView> {
        let AstExpr::Path(segments) = &self.body.exprs[expr_id] else {
            return None;
        };
        if segments.len() != 1 {
            return None;
        }
        self.source_param_interface_view_for_name_at(expr_id, &segments[0])
    }

    fn source_param_interface_view_for_name_at(
        &self,
        expr_id: AstExprId,
        name: &Name,
    ) -> Option<InterfaceTypeView> {
        let binding_id = self.binding_id_for_name_at(expr_id, name)?;
        self.source_param_interface_view_for_binding(name, binding_id)
    }

    fn source_param_interface_view_for_binding(
        &self,
        name: &Name,
        binding_id: BindingId,
    ) -> Option<InterfaceTypeView> {
        let func_loc = self.func_loc?;
        let param_scope = self.source_param_scope?;
        let sig = baml_compiler2_ppir::function_signature(self.db, func_loc);
        let (param_idx, param) = sig
            .params
            .iter()
            .enumerate()
            .find(|(_, param)| param.name == *name)?;
        if binding_id != BindingId::parameter(param_scope, param_idx) {
            return None;
        }

        let pkg_info = file_package(self.db, self.file);
        let pkg_id = PackageId::new(self.db, pkg_info.package);
        let pkg_items = package_items(self.db, pkg_id);
        let generic_params = self.enclosing_generic_params();
        let bindings = generic_params
            .iter()
            .map(|param| {
                (
                    param.clone(),
                    Tir2Ty::TypeVar(param.clone(), TyAttr::default()),
                )
            })
            .collect();
        let mut diags = Vec::new();
        let ty = baml_compiler2_tir::generics::lower_type_expr_with_generics(
            self.db,
            &param.ty,
            pkg_items,
            &pkg_info.namespace_path,
            &bindings,
            &mut diags,
        );
        self.interface_dispatch_target_for_tir_ty(&ty)
    }

    fn dispatch_receiver_static_tir_ty(&self, expr_id: AstExprId) -> Option<Tir2Ty> {
        if let AstExpr::Upcast { base, .. } = &self.body.exprs[expr_id] {
            return self
                .expr_types
                .get(&self.expr_metadata_key(*base))
                .cloned()
                .or_else(|| self.dispatch_receiver_static_tir_ty(*base));
        }
        self.expr_types
            .get(&self.expr_metadata_key(expr_id))
            .cloned()
            .or_else(|| self.self_typevar_for_expr(expr_id))
    }

    fn self_typevar_for_expr(&self, expr_id: AstExprId) -> Option<Tir2Ty> {
        let AstExpr::Path(segments) = &self.body.exprs[expr_id] else {
            return None;
        };
        if segments.len() == 1
            && segments[0].as_str() == "self"
            && self.generic_param_bounds.contains_key(&Name::new("Self"))
        {
            Some(Tir2Ty::TypeVar(
                Name::new("Self"),
                baml_compiler2_tir::ty::TyAttr::default(),
            ))
        } else {
            None
        }
    }

    fn upcast_target_interface_view(&self, expr_id: AstExprId) -> Option<InterfaceTypeView> {
        let AstExpr::Upcast { target, .. } = &self.body.exprs[expr_id] else {
            return None;
        };
        let pkg_info = baml_compiler2_hir::file_package::file_package(self.db, self.file);
        let pkg_id = baml_compiler2_hir::package::PackageId::new(self.db, pkg_info.package.clone());
        let pkg_items = baml_compiler2_hir::package::package_items(self.db, pkg_id);
        let generic_params = self.enclosing_generic_params();
        let mut diags = Vec::new();
        let target_ty = baml_compiler2_tir::lower_type_expr::lower_type_expr_in_ns(
            self.db,
            target,
            pkg_items,
            &pkg_info.namespace_path,
            &generic_params,
            &mut diags,
        );
        self.interface_dispatch_target_for_tir_ty(&target_ty)
    }

    fn class_dispatch_target_for_tir_ty(&self, ty: &Tir2Ty) -> Option<(TypeName, Vec<Ty>)> {
        match ty {
            Tir2Ty::Class(qtn, type_args, _) => Some((
                qtn.clone(),
                type_args
                    .iter()
                    .map(|arg| self.convert_tir_ty_for_runtime(arg))
                    .collect(),
            )),
            Tir2Ty::TypeVar(name, _) => self
                .generic_param_bounds
                .get(name)
                .and_then(|bound| self.class_dispatch_target_for_tir_ty(bound)),
            Tir2Ty::AssociatedTypeProjection { .. } => {
                let resolver =
                    baml_compiler2_tir::associated_projection::AssociatedProjectionResolver::new(
                        self.db,
                        &self.resolved_aliases.aliases,
                        &self.generic_param_bounds,
                    );
                let resolved = resolver.resolve_deep(ty);
                if &resolved != ty {
                    return self.class_dispatch_target_for_tir_ty(&resolved);
                }
                resolver
                    .resolve_projection_bound(ty)
                    .and_then(|bound| self.class_dispatch_target_for_tir_ty(&bound))
            }
            _ => None,
        }
    }

    /// BEP-044 wf3 #G7: for a *concrete* receiver whose
    /// method is provided by a blanket / out-of-body `implements … for …` rule
    /// (not an in-body block), find the single interface that provides `method`
    /// so a direct `recv.method()` dispatches through the normal interface
    /// switch. Returns the interface view. TIR has already rejected the
    /// ambiguous (>1 interface) case with E0121, so the first declaring match
    /// is unambiguous for a compiling program.
    fn registry_dispatch_target_for_concrete(
        &self,
        recv_ty: &Tir2Ty,
        method: &Name,
    ) -> Option<InterfaceTypeView> {
        // Only concrete receivers — interfaces/type-vars dispatch via the
        // arms above. Containers are concrete too (`implements<T> I for T[]`).
        if !matches!(
            recv_ty,
            Tir2Ty::Class(..)
                | Tir2Ty::Int { .. }
                | Tir2Ty::Bigint { .. }
                | Tir2Ty::Float { .. }
                | Tir2Ty::String { .. }
                | Tir2Ty::Bool { .. }
                | Tir2Ty::Null { .. }
                | Tir2Ty::Uint8Array { .. }
                | Tir2Ty::Media(..)
                | Tir2Ty::List(..)
                | Tir2Ty::Map { .. }
                | Tir2Ty::Future(..)
        ) {
            return None;
        }
        let pkg = baml_compiler2_hir::file_package::file_package(self.db, self.file).package;
        let pkg_id = baml_compiler2_hir::package::PackageId::new(self.db, pkg);
        let mut package_ids: Vec<_> =
            baml_compiler2_hir::package::package_dependencies(self.db, pkg_id).clone();
        package_ids.push(pkg_id);
        for package_id in package_ids {
            let registry =
                baml_compiler2_tir::interfaces::package_implements_registry(self.db, package_id);
            for rule in &registry.interface_impl_rules {
                let Some(bindings) = baml_compiler2_tir::interfaces::match_ty_pattern(
                    &rule.for_ty_pattern,
                    recv_ty,
                    &rule.generic_params,
                    &self.resolved_aliases.aliases,
                ) else {
                    continue;
                };
                let iface_ty =
                    baml_compiler2_tir::generics::substitute_ty(&rule.interface_ty, &bindings);
                let Tir2Ty::Interface(iface_qtn, iface_args, iface_assoc, _) = iface_ty else {
                    continue;
                };
                if self.mir_interface_declares_method(&iface_qtn, method) {
                    return Some((iface_qtn, iface_args, iface_assoc));
                }
            }
        }
        None
    }

    /// Whether `iface_qtn` or any interface in its `requires` closure declares a
    /// method named `method`. Mirrors the TIR-side check; used by
    /// `registry_dispatch_target_for_concrete`.
    fn mir_interface_declares_method(&self, iface_qtn: &QualifiedTypeName, method: &Name) -> bool {
        let pkg_id =
            baml_compiler2_hir::package::PackageId::new(self.db, iface_qtn.package().clone());
        let pkg_items = baml_compiler2_hir::package::package_items(self.db, pkg_id);
        let Some(baml_compiler2_hir::contributions::Definition::Interface(root_loc)) =
            pkg_items.lookup_type(iface_qtn.namespace(), iface_qtn.name())
        else {
            return false;
        };
        let root_pkg =
            baml_compiler2_hir::file_package::file_package(self.db, root_loc.file(self.db));
        baml_compiler2_tir::interfaces::interface_closure_locs(
            self.db,
            root_loc,
            pkg_items,
            &root_pkg.namespace_path,
        )
        .into_iter()
        .any(|iface_loc| {
            let iface_tree = baml_compiler2_hir::file_item_tree(self.db, iface_loc.file(self.db));
            iface_tree
                .interfaces
                .get(&iface_loc.id(self.db))
                .is_some_and(|iface_data| {
                    iface_data
                        .required_methods
                        .iter()
                        .any(|s| s.name == *method)
                        || iface_data
                            .default_methods
                            .iter()
                            .any(|&fn_id| iface_tree[fn_id].name == *method)
                })
        })
    }

    fn expr_ty(&self, expr_id: AstExprId) -> Ty {
        self.expr_types
            .get(&self.expr_metadata_key(expr_id))
            .map(|ty| self.convert_tir_ty_for_runtime(ty))
            .unwrap_or(Ty::Void {
                attr: TyAttr::default(),
            })
    }

    /// Compute the `TyTemplate` slice for the class-level type args of a class
    /// construction expression.
    ///
    /// Returns `vec![]` for non-generic (or unresolved) classes.
    fn class_type_arg_templates(&self, expr_id: AstExprId) -> Vec<TyTemplate> {
        let generic_params = self.enclosing_generic_params();
        match self.expr_types.get(&self.expr_metadata_key(expr_id)) {
            Some(Tir2Ty::Class(_, type_args, _)) if !type_args.is_empty() => type_args
                .iter()
                .map(|t| self.ty_to_template(t, &generic_params))
                .collect(),
            _ => vec![],
        }
    }

    fn object_class_type_arg_templates(
        &self,
        expr_id: AstExprId,
        explicit_type_args: &[AstTypeExpr],
    ) -> Vec<TyTemplate> {
        if explicit_type_args.is_empty() {
            self.class_type_arg_templates(expr_id)
        } else {
            self.generic_apply_type_arg_templates(explicit_type_args)
        }
    }

    /// Get the `baml_type::Ty` for a pattern binding
    fn pat_ty(&self, pat_id: AstPatId) -> Ty {
        self.pat_types
            .get(&self.pat_metadata_key(pat_id))
            .map(|ty| self.convert_tir_ty_for_runtime(ty))
            .unwrap_or(Ty::Void {
                attr: TyAttr::default(),
            })
    }

    fn is_pattern_type_recovery(ty: &Ty) -> bool {
        matches!(ty, Ty::Void { .. } | Ty::BuiltinUnknown { .. })
    }

    /// Get the TIR-inferred root segment type for a multi-segment Path expression.
    /// Returns `None` if no root type was recorded (e.g. single-segment paths).
    fn path_root_ty(&self, expr_id: AstExprId) -> Option<Ty> {
        self.path_root_types
            .get(&self.expr_metadata_key(expr_id))
            .map(|ty| self.convert_tir_ty_for_runtime(ty))
    }

    /// Get the TIR-inferred type of `segments[..=seg_idx]` for a multi-segment
    /// local-rooted Path expression. Returns `None` if not recorded.
    #[allow(dead_code)]
    fn path_segment_ty(&self, expr_id: AstExprId, seg_idx: usize) -> Option<Ty> {
        self.path_segment_types
            .get(&(self.current_metadata_scope, expr_id, seg_idx))
            .map(|ty| self.convert_tir_ty_for_runtime(ty))
    }

    /// Resolve a `TypeExpr` annotation directly to a `baml_type::Ty`.
    /// Used for `TypedBinding` patterns where TIR may not have populated the
    /// bindings map (e.g. catch arm and match arm patterns).
    fn resolve_type_annotation(&self, ty_expr: &baml_compiler2_ast::TypeExpr) -> Ty {
        use baml_compiler2_tir::lower_type_expr::lower_type_expr_in_ns;
        let pkg_info = file_package(self.db, self.file);
        let pkg_id = PackageId::new(self.db, pkg_info.package);
        let pkg_items = package_items(self.db, pkg_id);
        let mut diags = Vec::new();
        let tir_ty = lower_type_expr_in_ns(
            self.db,
            ty_expr,
            pkg_items,
            &pkg_info.namespace_path,
            &[],
            &mut diags,
        );
        self.resolved_aliases.convert(&tir_ty)
    }

    /// Lower a pattern's type annotation to TIR with the enclosing function's
    /// generic params in scope, so `TypeVar`s survive (unlike
    /// [`Self::resolve_type_annotation`], which erases them). Used by typed
    /// pattern tests, where a surviving `TypeVar` lowers to a `TypeArgRef`
    /// template instead of a constant-false `Void` test.
    fn lower_type_annotation_tir(&self, ty_expr: &baml_compiler2_ast::TypeExpr) -> Tir2Ty {
        use baml_compiler2_tir::lower_type_expr::lower_type_expr_in_ns;
        let generic_params = self.enclosing_generic_params();
        let pkg_info = file_package(self.db, self.file);
        let pkg_id = PackageId::new(self.db, pkg_info.package);
        let pkg_items = package_items(self.db, pkg_id);
        let mut diags = Vec::new();
        lower_type_expr_in_ns(
            self.db,
            ty_expr,
            pkg_items,
            &pkg_info.namespace_path,
            &generic_params,
            &mut diags,
        )
    }

    /// Build a `Span` from an expression's source range.
    /// Returns `None` if no source map is available (e.g. synthesized bodies).
    fn span_for_expr(&self, expr_id: AstExprId) -> Option<baml_base::Span> {
        let sm = self.source_map.as_ref()?;
        let range = sm.expr_span(expr_id);
        Some(baml_base::Span::new(self.file.file_id(self.db), range))
    }

    /// Build a `Span` from a statement's source range.
    fn span_for_stmt(&self, stmt_id: AstStmtId) -> Option<baml_base::Span> {
        let sm = self.source_map.as_ref()?;
        let range = sm.stmt_span(stmt_id);
        Some(baml_base::Span::new(self.file.file_id(self.db), range))
    }
}

// ─── 3.1: lower_function_body ────────────────────────────────────────────────

#[allow(clippy::elidable_lifetime_names)]
impl<'db> LoweringContext<'db> {
    fn lower_function_body(&mut self) -> MirFunction {
        use baml_compiler2_tir::lower_type_expr::lower_type_expr_in_ns;

        let func_loc = self
            .func_loc
            .expect("lower_function_body called on non-function LoweringContext");
        let sig = baml_compiler2_ppir::function_signature(self.db, func_loc);

        // Return place _0
        let pkg_info = file_package(self.db, self.file);
        let pkg_id = PackageId::new(self.db, pkg_info.package);
        let pkg_items = package_items(self.db, pkg_id);

        let ret_ty = sig
            .return_type
            .as_ref()
            .map(|te| {
                let mut diags = Vec::new();
                let tir_ty = lower_type_expr_in_ns(
                    self.db,
                    te,
                    pkg_items,
                    &pkg_info.namespace_path,
                    &[],
                    &mut diags,
                );
                self.resolved_aliases.convert(&tir_ty)
            })
            .unwrap_or(Ty::Null {
                attr: TyAttr::default(),
            });
        let ret = self
            .builder
            .declare_local(Some(Name::new("_0")), ret_ty, None, false);

        // Detect enclosing class for `self` parameter resolution
        let index = file_semantic_index(self.db, self.file);
        let item_tree = file_item_tree(self.db, self.file);
        let func_data = &item_tree[func_loc.id(self.db)];
        // Set the function-level span on the builder so MirFunction::span is populated.
        self.builder.set_span(baml_base::Span::new(
            self.file.file_id(self.db),
            func_data.span,
        ));
        let func_scope_id: FileScopeId =
            index.scope_at_offset(func_data.span.start(), Some(&func_data.name));
        let func_scope = &index.scopes[func_scope_id.index() as usize];
        let enclosing_class_name: Option<Name> = func_scope.parent.and_then(|parent_idx| {
            let parent = &index.scopes[parent_idx.index() as usize];
            if matches!(parent.kind, baml_compiler2_hir::scope::ScopeKind::Class) {
                parent.name.clone()
            } else {
                None
            }
        });
        let enclosing_impl = item_tree
            .implements_for
            .iter()
            .find(|imp| imp.methods.contains(&func_loc.id(self.db)));

        // Parameter locals _1..=_n
        // For `self` with no annotation, use the active rule receiver pattern
        // for out-of-body implementations, otherwise the enclosing class type.
        for (param_idx, param) in sig.params.iter().enumerate() {
            let param_ty = if param.name.as_str() == "self"
                && matches!(param.ty, baml_compiler2_ast::TypeExpr::Unknown { .. })
            {
                if let Some(imp) = enclosing_impl {
                    let mut diags = Vec::new();
                    let generic_params = self.enclosing_generic_params();
                    let tir_ty = lower_type_expr_in_ns(
                        self.db,
                        &imp.for_target.expr,
                        pkg_items,
                        &pkg_info.namespace_path,
                        &generic_params,
                        &mut diags,
                    );
                    self.convert_tir_ty_for_runtime(&tir_ty)
                } else {
                    enclosing_class_name
                        .as_ref()
                        .and_then(|cn| {
                            pkg_items
                                .lookup_type(&pkg_info.namespace_path, cn)
                                .map(|def| {
                                    let tir_ty = baml_compiler2_tir::ty::Ty::Class(
                                        baml_compiler2_tir::lower_type_expr::qualify_def(
                                            self.db, def, cn,
                                        ),
                                        vec![],
                                        baml_compiler2_tir::ty::TyAttr::default(),
                                    );
                                    self.resolved_aliases.convert(&tir_ty)
                                })
                        })
                        .unwrap_or(Ty::Null {
                            attr: TyAttr::default(),
                        })
                }
            } else {
                let mut diags = Vec::new();
                let generic_params = self.enclosing_generic_params();
                let tir_ty = lower_type_expr_in_ns(
                    self.db,
                    &param.ty,
                    pkg_items,
                    &pkg_info.namespace_path,
                    &generic_params,
                    &mut diags,
                );
                self.convert_tir_ty_for_runtime(&tir_ty)
            };
            let local = self
                .builder
                .declare_local(Some(param.name.clone()), param_ty, None, false);
            self.locals.insert(param.name.clone(), local);
            self.binding_locals
                .insert(BindingId::parameter(self.current_scope, param_idx), local);
        }

        // Entry and exit blocks
        let entry = self.builder.create_block();
        let exit = self.builder.create_block();
        self.exit_block = exit;
        self.builder.set_current_block(entry);

        let parameter_defaults =
            baml_compiler2_ppir::function_parameter_defaults(self.db, func_loc);
        self.lower_default_parameter_prologue(func_data, &parameter_defaults);

        // Lower root expression into return place
        let root_expr = self.body.root_expr;
        if let Some(root) = root_expr {
            self.lower_expr(root, Place::local(ret));
        } else {
            self.builder.assign(
                Place::local(ret),
                Rvalue::Use(Operand::Constant(Constant::Null)),
            );
        }

        // Goto exit, emit Return terminator
        if !self.builder.is_current_terminated() {
            self.builder.goto(self.exit_block);
        }
        self.builder.set_current_block(self.exit_block);
        self.builder.return_();

        // Mark locals captured by nested lambdas. HIR stores this by binding
        // identity, including block-owned bindings.
        self.mark_captured_locals_in_scope_tree(self.current_scope);

        // Take the builder out of self to call `build()` which consumes it
        let dummy = MirBuilder::new(Name::new("_dummy"), 0);
        let builder = std::mem::replace(&mut self.builder, dummy);
        let mut mir = builder.build();
        optimize::optimize_function(&mut mir);

        // Drain any lambda functions lowered during this function's body into the
        // MirFunction's lambdas list.  The lambda_idx values in MakeClosure rvalues
        // index into this vec.
        mir.lambdas = std::mem::take(&mut self.pending_lambdas);

        mir
    }

    fn lower_default_parameter_prologue(
        &mut self,
        func_data: &baml_compiler2_hir::item_tree::Function,
        parameter_defaults: &baml_compiler2_hir::signature::FunctionParameterDefaults,
    ) {
        for (index, param) in func_data.params.iter().enumerate() {
            let Some(default_ref) = parameter_defaults.param_default(index) else {
                continue;
            };

            let Some(&param_local) = self.locals.get(&param.name) else {
                continue;
            };

            let test_local = self.builder.temp(Ty::Bool {
                attr: TyAttr::default(),
            });
            self.builder.assign(
                Place::local(test_local),
                Rvalue::BinaryOp {
                    op: BinOp::Eq,
                    left: Operand::Copy(Place::local(param_local)),
                    right: Operand::Constant(Constant::OmittedArg),
                },
            );

            let default_block = self.builder.create_block();
            let next_block = self.builder.create_block();
            self.builder.branch(
                Operand::Copy(Place::local(test_local)),
                default_block,
                next_block,
            );

            self.builder.set_current_block(default_block);
            self.lower_default_expr(
                default_ref.expr.expr(),
                &parameter_defaults.defaults,
                Place::local(param_local),
            );
            if !self.builder.is_current_terminated() {
                self.builder.goto(next_block);
            }

            self.builder.set_current_block(next_block);
        }
    }

    fn lower_default_expr(
        &mut self,
        expr_id: AstExprId,
        defaults: &baml_compiler2_ast::FunctionDefaults,
        dest: Place,
    ) {
        let saved_body = std::mem::replace(&mut self.body, defaults.exprs.clone());
        let saved_source_map = self.source_map.replace(defaults.source_map.clone());
        let saved_metadata_scope = self.current_metadata_scope;
        self.current_metadata_scope = MetadataScope::ParameterDefault(self.current_scope);
        self.lower_expr(expr_id, dest);
        self.current_metadata_scope = saved_metadata_scope;
        self.source_map = saved_source_map;
        self.body = saved_body;
    }

    /// Lower a top-level let binding's initializer into a zero-arg `MirFunctionBody`.
    ///
    /// The resulting body has arity 0, a single `_0` return place (type unknown/null),
    /// and evaluates the initializer expression, leaving the result in `_0`.
    /// This is used by `compile_init_function` to compile let initializers into bytecode
    /// that can then be called and have their result stored via `StoreGlobal`.
    fn lower_let_body_inner(&mut self) -> MirFunctionBody {
        // Return place _0 (type unknown — let bodies don't have type annotations)
        let ret = self.builder.declare_local(
            Some(Name::new("_0")),
            Ty::Null {
                attr: TyAttr::default(),
            },
            None,
            false,
        );

        // Entry and exit blocks
        let entry = self.builder.create_block();
        let exit = self.builder.create_block();
        self.exit_block = exit;
        self.builder.set_current_block(entry);

        // Lower root expression into return place
        if let Some(root) = self.body.root_expr {
            self.lower_expr(root, Place::local(ret));
        } else {
            self.builder.assign(
                Place::local(ret),
                Rvalue::Use(Operand::Constant(Constant::Null)),
            );
        }

        // Goto exit, emit Return terminator
        if !self.builder.is_current_terminated() {
            self.builder.goto(self.exit_block);
        }
        self.builder.set_current_block(self.exit_block);
        self.builder.return_();

        // Take the builder out and build the MirFunctionBody
        let dummy = MirBuilder::new(Name::new("_dummy"), 0);
        let builder = std::mem::replace(&mut self.builder, dummy);
        let mut body = builder.build_body();
        optimize::optimize_function_body(&mut body);
        body
    }

    fn lower_optional_function_adapter(
        &mut self,
        expr_id: AstExprId,
        coercion: &baml_compiler2_tir::inference::FunctionCoercion,
        dest: Place,
    ) {
        let original_ty = self.expr_ty(expr_id);
        let original_local = self.builder.temp(original_ty);
        self.lower_expr_without_function_coercion(expr_id, Place::Local(original_local));
        self.builder.local_decl_mut(original_local).is_captured = true;

        let parent_name = self.builder.name().to_string();
        let adapter_count = self
            .synthetic_name_counts
            .entry("__optional_adapter".to_string())
            .or_insert(0);
        let adapter_idx = *adapter_count;
        *adapter_count += 1;
        let adapter_name = format!("<optional-adapter({parent_name}, {adapter_idx})>");

        let mut adapter_builder =
            MirBuilder::new(Name::new(&adapter_name), coercion.target_params.len());

        let ret_ty = convert_tir2_ty(&coercion.target_return, self.resolved_aliases);
        let ret = adapter_builder.declare_local(Some(Name::new("_0")), ret_ty, None, false);

        for param in &coercion.target_params {
            let param_ty = convert_tir2_ty(&param.ty, self.resolved_aliases);
            adapter_builder.declare_local(param.name.clone(), param_ty, None, false);
        }

        let entry = adapter_builder.create_block();
        let after_call = adapter_builder.create_block();
        adapter_builder.set_current_block(entry);

        let mut next_required_target = 0usize;
        let mut source_args = Vec::with_capacity(coercion.source_params.len());
        for source_param in &coercion.source_params {
            match source_param.mode {
                FunctionParamMode::Required => {
                    let target_index = coercion
                        .target_params
                        .iter()
                        .enumerate()
                        .filter(|(_, param)| param.is_required())
                        .nth(next_required_target)
                        .map(|(idx, _)| idx)
                        .unwrap_or(next_required_target);
                    next_required_target += 1;
                    source_args.push(Operand::Copy(Place::Local(Local(target_index + 1))));
                }
                FunctionParamMode::Optional => {
                    let target_index = source_param.name.as_ref().and_then(|name| {
                        coercion.target_params.iter().position(|param| {
                            param.is_optional() && param.name.as_ref() == Some(name)
                        })
                    });
                    if let Some(target_index) = target_index {
                        source_args.push(Operand::Copy(Place::Local(Local(target_index + 1))));
                    } else {
                        source_args.push(Operand::Constant(Constant::OmittedArg));
                    }
                }
            }
        }

        adapter_builder.call(
            Operand::Copy(Place::Capture(0)),
            source_args,
            Place::Local(ret),
            after_call,
            None,
        );
        adapter_builder.set_current_block(after_call);
        adapter_builder.return_();

        let mut adapter_mir = adapter_builder.build();
        optimize::optimize_function(&mut adapter_mir);
        adapter_mir.item_ref = ItemRef::Free {
            package: Name::new(""),
            namespace: vec![],
            name: Name::new(&adapter_name),
        };

        let lambda_idx = self.pending_lambdas.len();
        self.pending_lambdas.push(adapter_mir);
        self.builder.assign(
            dest,
            Rvalue::MakeClosure {
                lambda_idx,
                captures: vec![Operand::Copy(Place::Local(original_local))],
                type_arg_templates: vec![],
            },
        );
    }

    /// Lower a lambda expression into a nested `MirFunction` and emit a
    /// `Rvalue::MakeClosure` assignment into `dest`.
    ///
    /// Saves all parent-body state, sets up a fresh builder for the lambda,
    /// lowers the lambda body, then restores the parent state.  The completed
    /// `MirFunction` is pushed into `self.pending_lambdas`; its index becomes
    /// the `lambda_idx` in the emitted `MakeClosure` rvalue.
    ///
    /// Captures are empty in Phase 3 (non-capturing lambdas only).
    #[allow(clippy::cast_possible_truncation)]
    fn lower_lambda(
        &mut self,
        func_def: &baml_compiler2_ast::FunctionDef,
        expr_id: AstExprId,
        dest: Place,
    ) {
        use baml_compiler2_tir::lower_type_expr::lower_type_expr_in_ns;

        // Generate a unique synthetic name for this lambda.
        let parent_name = self.builder.name().to_string();
        let lambda_count = self
            .synthetic_name_counts
            .entry("__lambda".to_string())
            .or_insert(0);
        let lambda_idx_name = *lambda_count;
        *lambda_count += 1;
        let lambda_name = format!("<lambda({parent_name}, {lambda_idx_name})>");

        // Find the lambda's FileScopeId from the HIR index.
        // The HIR builder registered a ScopeKind::Lambda at the lambda expression's span.
        let lambda_scope_id: FileScopeId = if let Some(ref sm) = self.source_map {
            let lambda_span = sm.expr_span(expr_id);
            let index = file_semantic_index(self.db, self.file);
            // Find the Lambda scope containing this span by searching for it.
            // We look for a Lambda-kind scope whose range matches the lambda span.
            let mut found = None;
            for (i, scope) in index.scopes.iter().enumerate() {
                if scope.kind == baml_compiler2_hir::scope::ScopeKind::Lambda
                    && scope.range == lambda_span
                {
                    found = Some(FileScopeId::new(i as u32));
                    break;
                }
            }
            found.unwrap_or(self.current_scope)
        } else {
            self.current_scope
        };

        // Pull out the lambda's body and source map.
        let (lambda_body, lambda_source_map) = match func_def.body.as_ref() {
            Some(baml_compiler2_ast::FunctionBodyDef::Expr(body, sm)) => {
                (body.clone(), Some(sm.clone()))
            }
            _ => {
                // No body — emit a panic stub and return.
                self.emit_panic_call("lambda without body", expr_id);
                return;
            }
        };

        // Read HIR captures for this lambda scope.
        // `captures` lists the exact binding identities that the lambda reads
        // from enclosing scopes. We build `capture_indices` so path/lvalue
        // lowering can emit `Place::Capture(idx)` without collapsing shadows by name.
        let hir_captures: Vec<(Name, BindingId)> = {
            let index = file_semantic_index(self.db, self.file);
            index
                .scope_bindings
                .get(lambda_scope_id.index() as usize)
                .map(|sb| sb.captures.clone())
                .unwrap_or_default()
        };
        let lambda_capture_indices: HashMap<BindingId, usize> = hir_captures
            .iter()
            .enumerate()
            .map(|(i, (_, binding_id))| (*binding_id, i))
            .collect();

        // Save parent state.
        let saved_builder = std::mem::replace(
            &mut self.builder,
            MirBuilder::new(Name::new(&lambda_name), 0),
        );
        let saved_body = std::mem::replace(&mut self.body, lambda_body);
        let saved_source_map = std::mem::replace(&mut self.source_map, lambda_source_map);
        let saved_locals = std::mem::take(&mut self.locals);
        let saved_binding_locals = std::mem::take(&mut self.binding_locals);
        let saved_exit_block = self.exit_block;
        let saved_loop_context = self.loop_context.take();
        let saved_catch_context = self.catch_context.take();
        let saved_watched_locals = std::mem::take(&mut self.watched_locals_stack);
        let saved_current_scope = self.current_scope;
        let saved_metadata_scope = self.current_metadata_scope;
        // Extend the enclosing-lambda generic params with this lambda's own
        // params for the duration of its body, so `reflect.type_of<T>` (and any
        // type-arg resolution) inside resolves `T` to the right frame slot.
        // Appended after the enclosing params, matching the runtime layout:
        // frame.type_args = [captured enclosing params..., this lambda's args...].
        let saved_lambda_generic_params = self.lambda_generic_params.clone();
        self.lambda_generic_params
            .extend(func_def.generic_params.iter().cloned());
        // NOTE: synthetic_name_counts is intentionally NOT saved — its counter
        // keeps incrementing across the whole function for uniqueness.
        //
        // pending_lambdas IS saved so each lambda collects only its own direct
        // children. The lambda body's nested lambdas are collected separately
        // and attached to the lambda as its `.lambdas` field.
        let saved_pending_lambdas = std::mem::take(&mut self.pending_lambdas);
        let saved_capture_indices = self.capture_indices.take();
        // Save transitive_captures_needed: after lowering this lambda's body,
        // newly discovered transitive captures will be in this field.  We save
        // the parent's list and restore it after collecting.
        let saved_transitive_captures = std::mem::take(&mut self.transitive_captures_needed);

        // Switch to the lambda scope and install capture map.
        // Always use Some(map) — even for empty HIR captures — so that
        // add_transitive_capture can extend it at runtime.
        self.current_scope = lambda_scope_id;
        self.current_metadata_scope = MetadataScope::Body(lambda_scope_id);
        self.capture_indices = Some(lambda_capture_indices);

        // Set up a fresh builder with the correct arity.
        let arity = func_def.params.len();
        self.builder = MirBuilder::new(Name::new(&lambda_name), arity);

        // Declare return place _0.
        let pkg_info = file_package(self.db, self.file);
        let pkg_id = PackageId::new(self.db, pkg_info.package.clone());
        let pkg_items = package_items(self.db, pkg_id);
        let ret = self.builder.declare_local(
            Some(Name::new("_0")),
            baml_type::Ty::Null {
                attr: baml_type::TyAttr::default(),
            },
            None,
            false,
        );

        // Declare parameter locals _1..=_n. Lower their annotations with the
        // enclosing generic params in scope so a parameter typed as an
        // enclosing type variable (`(a: T) -> ...`) resolves to that variable,
        // and record the lowered TIR type so interface dispatch on the
        // parameter can recover its (possibly bounded) static type — TIR does
        // not surface it via `path_segment_types` for lambda receivers.
        // Restored after the body (`saved_lambda_param_tir_types` below).
        let saved_lambda_param_tir_types = self.lambda_param_tir_types.clone();
        let enclosing_generics = self.enclosing_generic_params();
        for (param_idx, param) in func_def.params.iter().enumerate() {
            let param_ty = match &param.type_expr {
                Some(spanned_te) => {
                    let mut diags = Vec::new();
                    let tir_ty = lower_type_expr_in_ns(
                        self.db,
                        &spanned_te.expr,
                        pkg_items,
                        &pkg_info.namespace_path,
                        &enclosing_generics,
                        &mut diags,
                    );
                    self.lambda_param_tir_types
                        .insert(param.name.clone(), tir_ty.clone());
                    self.resolved_aliases.convert(&tir_ty)
                }
                None => baml_type::Ty::Null {
                    attr: baml_type::TyAttr::default(),
                },
            };
            let local = self
                .builder
                .declare_local(Some(param.name.clone()), param_ty, None, false);
            self.locals.insert(param.name.clone(), local);
            self.binding_locals
                .insert(BindingId::parameter(self.current_scope, param_idx), local);
        }

        // Create entry and exit blocks.
        let entry = self.builder.create_block();
        let exit_blk = self.builder.create_block();
        self.exit_block = exit_blk;
        self.builder.set_current_block(entry);

        // Lower the root expression into the return place.
        if let Some(root) = self.body.root_expr {
            self.lower_expr(root, Place::local(ret));
        } else {
            self.builder.assign(
                Place::local(ret),
                Rvalue::Use(Operand::Constant(Constant::Null)),
            );
        }

        // Terminate: goto exit, then return.
        if !self.builder.is_current_terminated() {
            self.builder.goto(self.exit_block);
        }
        self.builder.set_current_block(self.exit_block);
        self.builder.return_();

        // Mark locals captured by nested lambdas. HIR stores this by binding
        // identity, including block-owned bindings.
        self.mark_captured_locals_in_scope_tree(lambda_scope_id);

        // Build the lambda MirFunction.
        // First, collect any nested lambdas that were encountered while lowering
        // this lambda's body (direct children only — saved_pending_lambdas holds
        // any lambdas from the parent scope that were already pending before
        // entering this lambda).
        let nested_lambdas = std::mem::take(&mut self.pending_lambdas);

        let dummy = MirBuilder::new(Name::new("_dummy"), 0);
        let lambda_builder = std::mem::replace(&mut self.builder, dummy);
        let mut lambda_mir = lambda_builder.build();
        optimize::optimize_function(&mut lambda_mir);
        // Override item_ref with the synthetic name.
        lambda_mir.item_ref = ItemRef::Free {
            package: Name::new(""),
            namespace: vec![],
            name: Name::new(&lambda_name),
        };
        // Attach nested lambdas as direct children.
        lambda_mir.lambdas = nested_lambdas;

        // Collect transitive captures that inner lambda bodies discovered were
        // needed (names that weren't in hir_captures but that inner lambdas
        // required via transitive capture).
        let newly_needed_transitive = std::mem::take(&mut self.transitive_captures_needed);

        // Restore parent state.
        self.lambda_param_tir_types = saved_lambda_param_tir_types;
        self.builder = saved_builder;
        self.body = saved_body;
        self.source_map = saved_source_map;
        self.locals = saved_locals;
        self.binding_locals = saved_binding_locals;
        self.exit_block = saved_exit_block;
        self.loop_context = saved_loop_context;
        self.catch_context = saved_catch_context;
        self.watched_locals_stack = saved_watched_locals;
        self.current_scope = saved_current_scope;
        self.current_metadata_scope = saved_metadata_scope;
        self.lambda_generic_params = saved_lambda_generic_params;
        self.capture_indices = saved_capture_indices;
        // Restore parent's pending_lambdas (siblings of this lambda).
        self.pending_lambdas = saved_pending_lambdas;
        // Restore the parent's transitive captures (not ours).
        self.transitive_captures_needed = saved_transitive_captures;

        // Extend hir_captures with any transitively-needed names discovered
        // during body lowering (for inner lambdas that needed grandparent vars).
        // Do NOT propagate here — the capture operands building loop below will
        // handle propagation by pushing to `transitive_captures_needed` when a
        // name is not found in the current scope's locals or captures.
        let mut extended_hir_captures = hir_captures;
        for binding_id in newly_needed_transitive {
            if !extended_hir_captures
                .iter()
                .any(|(_, existing)| *existing == binding_id)
            {
                extended_hir_captures.push((Name::new("_capture"), binding_id));
            }
        }

        // Build capture operands from restored parent locals/captures.
        // Each captured name must be in the parent's locals map; we pass the cell
        // pointer (the slot itself, not the inner value) via Operand::Copy(Place::Local(local)).
        // The emit phase later replaces this with a LoadVar of the cell slot (not LoadDeref).
        //
        // If a name is not in the parent's locals AND not in the parent's
        // capture_indices, we add it as a transitive capture of the current
        // lambda — i.e. the current lambda (f) will need to capture it from ITS
        // parent, and g will receive it via f's capture slot.
        let mut capture_operands: Vec<Operand> = Vec::with_capacity(extended_hir_captures.len());
        for (_, binding_id) in &extended_hir_captures {
            if let Some(&local) = self.binding_locals.get(binding_id) {
                // Mark the local as captured at the capture site — this is the
                // definitive place where we know the exact Local being captured,
                // even in the presence of shadowing.
                self.builder.local_decl_mut(local).is_captured = true;
                capture_operands.push(Operand::Copy(Place::Local(local)));
            } else if let Some(cap_idx) = self
                .capture_indices
                .as_ref()
                .and_then(|m| m.get(binding_id))
                .copied()
            {
                // The variable is itself a capture in the current scope.
                capture_operands.push(Operand::Copy(Place::Capture(cap_idx)));
            } else {
                // Not in current scope's locals or captures.
                // Add as a new transitive capture of the current lambda so our
                // parent will pass it through to us, and we can forward it to
                // the inner lambda.
                let new_idx = {
                    let ci = self.capture_indices.get_or_insert_with(HashMap::new);
                    let idx = ci.len();
                    ci.insert(*binding_id, idx);
                    idx
                };
                // Signal to our parent lambda that it needs to capture this name.
                self.transitive_captures_needed.push(*binding_id);
                capture_operands.push(Operand::Copy(Place::Capture(new_idx)));
            }
        }

        // Push this lambda into the parent's pending_lambdas and emit MakeClosure.
        let lambda_pending_idx = self.pending_lambdas.len();
        self.pending_lambdas.push(lambda_mir);

        // Build TyTemplate entries for each enclosing generic type param so
        // the closure can materialise them at runtime.  These resolve in the
        // **outer** frame (TypeArgRef(N) → outer frame.type_args[N]).
        let enclosing_params = self.enclosing_generic_params();
        let type_arg_templates: Vec<TyTemplate> = enclosing_params
            .iter()
            .enumerate()
            .map(|(n, _)| TyTemplate::TypeArgRef(n as u32))
            .collect();

        self.builder.assign(
            dest,
            Rvalue::MakeClosure {
                lambda_idx: lambda_pending_idx,
                captures: capture_operands,
                type_arg_templates,
            },
        );
    }
}

// ─── 3.2: Core lower_expr dispatch ───────────────────────────────────────────

impl LoweringContext<'_> {
    fn lower_scoped_block(
        &mut self,
        stmts: &[AstStmtId],
        tail_expr: Option<AstExprId>,
        dest: Place,
    ) {
        let saved_locals = self.locals.clone();
        let watched_depth = self.watched_locals_stack.len();

        for &stmt_id in stmts {
            self.lower_stmt(stmt_id);
            if self.builder.is_current_terminated() {
                break;
            }
        }

        if !self.builder.is_current_terminated() {
            match tail_expr {
                Some(tail) => self.lower_expr(tail, dest),
                None => {
                    self.builder
                        .assign(dest, Rvalue::Use(Operand::Constant(Constant::Null)));
                }
            }
        }

        if !self.builder.is_current_terminated() {
            self.emit_unwatch_to_depth(watched_depth);
        }
        self.restore_locals_after_scope(saved_locals, watched_depth);
    }

    fn lower_expr(&mut self, expr_id: AstExprId, dest: Place) {
        if let Some(coercion) = self
            .function_coercions
            .get(&self.expr_metadata_key(expr_id))
            .cloned()
        {
            self.lower_optional_function_adapter(expr_id, &coercion, dest);
        } else {
            self.lower_expr_without_function_coercion(expr_id, dest);
        }
    }

    fn lower_expr_without_function_coercion(&mut self, expr_id: AstExprId, dest: Place) {
        let prev_span = self.builder.current_source_span;
        if let Some(span) = self.span_for_expr(expr_id) {
            self.builder.current_source_span = Some(span);
        }

        // Clone expr to avoid borrow issues
        let expr = self.body.exprs[expr_id].clone();
        match expr {
            AstExpr::Literal(lit) => {
                let constant = Self::lower_literal(&lit);
                self.builder
                    .assign(dest, Rvalue::Use(Operand::Constant(constant)));
            }

            AstExpr::ByteStringLiteral(bytes) => {
                self.builder.assign(dest, Rvalue::Uint8Array(bytes));
            }

            AstExpr::Null => {
                self.builder
                    .assign(dest, Rvalue::Use(Operand::Constant(Constant::Null)));
            }

            AstExpr::Path(segments) => {
                self.lower_path_expr(expr_id, &segments, dest);
            }

            AstExpr::If {
                condition,
                then_branch,
                else_branch,
            } => {
                self.lower_if(expr_id, condition, then_branch, else_branch, dest);
            }

            AstExpr::IfLet {
                pattern,
                scrutinee,
                then_branch,
                else_branch,
            } => {
                self.lower_if_let(expr_id, pattern, scrutinee, then_branch, else_branch, dest);
            }

            AstExpr::Binary { op, lhs, rhs } => {
                self.lower_binary(expr_id, op, lhs, rhs, dest);
            }

            AstExpr::Unary { op, expr } => {
                self.lower_unary(expr_id, op, expr, dest);
            }

            AstExpr::Call { callee, args, .. } => {
                let arg_exprs: Vec<_> = args.iter().map(|arg| arg.expr).collect();
                self.lower_call(expr_id, callee, &arg_exprs, dest);
            }

            AstExpr::Array { elements } => {
                let operands: Vec<Operand> =
                    elements.iter().map(|&e| self.lower_to_operand(e)).collect();
                self.builder.assign(dest, Rvalue::Array(operands));
            }

            AstExpr::Map { entries } => {
                let pairs: Vec<(Operand, Operand)> = entries
                    .iter()
                    .map(|&(k, v)| (self.lower_to_operand(k), self.lower_to_operand(v)))
                    .collect();
                self.builder.assign(dest, Rvalue::Map(pairs));
            }

            AstExpr::Object {
                type_name,
                type_args,
                fields,
                spreads,
                ..
            } => {
                self.lower_object(
                    expr_id,
                    type_name.as_ref(),
                    &type_args,
                    &fields,
                    &spreads,
                    dest,
                );
            }

            AstExpr::MemberAccess { base, member } => {
                self.lower_member_access(expr_id, base, &member, dest);
            }

            AstExpr::Upcast { base, .. } => {
                // `.as<I>` is a static type projection. Runtime representation
                // is the original value.
                self.lower_expr(base, dest);
            }

            AstExpr::GenericApply { base, type_args } => {
                self.lower_generic_apply(base, &type_args, dest);
            }

            AstExpr::OptionalMemberAccess { base, member } => {
                self.lower_optional_member_access(expr_id, base, &member, dest);
            }

            AstExpr::OptionalIndex { base, index } => {
                self.lower_optional_index(expr_id, base, index, dest);
            }

            AstExpr::OptionalCall { callee, args } => {
                let arg_exprs: Vec<_> = args.iter().map(|arg| arg.expr).collect();
                self.lower_optional_call(expr_id, callee, &arg_exprs, dest);
            }

            AstExpr::Index { base, index } => {
                self.lower_index(expr_id, base, index, dest);
            }

            AstExpr::Block { stmts, tail_expr } => {
                self.lower_scoped_block(&stmts, tail_expr, dest);
            }

            AstExpr::Match {
                scrutinee, arms, ..
            } => {
                let arms_owned = arms;
                self.lower_match(expr_id, scrutinee, &arms_owned, dest);
            }

            AstExpr::Is { scrutinee, pattern } => {
                // `<scrutinee> is <pattern>` — runtime pattern test that
                // yields `true` if the pattern matches, `false` otherwise.
                // We reuse `lower_pattern_test`, the same engine match-arm
                // dispatch uses, with two terminal blocks that write the
                // boolean constant into `dest` and jump to a join.
                let scrutinee_local = self.try_resolve_to_local(scrutinee).unwrap_or_else(|| {
                    let op = self.lower_to_operand(scrutinee);
                    let ty = self.expr_ty(scrutinee);
                    self.operand_to_local(op, ty)
                });

                let bb_true = self.builder.create_block();
                let bb_false = self.builder.create_block();
                let bb_join = self.builder.create_block();

                self.lower_pattern_test(scrutinee_local, pattern, bb_true, bb_false);

                self.builder.set_current_block(bb_true);
                self.builder.assign(
                    dest.clone(),
                    Rvalue::Use(Operand::Constant(Constant::Bool(true))),
                );
                self.builder.goto(bb_join);

                self.builder.set_current_block(bb_false);
                self.builder
                    .assign(dest, Rvalue::Use(Operand::Constant(Constant::Bool(false))));
                self.builder.goto(bb_join);

                self.builder.set_current_block(bb_join);
            }

            AstExpr::Catch { base, clauses } => {
                let clauses_owned = clauses;
                self.lower_catch(expr_id, base, &clauses_owned, &dest);
            }

            AstExpr::Throw { value } => {
                let val_op = self.lower_to_operand(value);
                if let Some(catch_ctx) = &self.catch_context {
                    // Inside a catch block: store the value into the error
                    // local and jump to the handler instead of unwinding.
                    let error_local = catch_ctx.error_local;
                    let unwind_target = catch_ctx.unwind_target;
                    self.builder
                        .assign(Place::Local(error_local), Rvalue::Use(val_op));
                    self.builder.goto(unwind_target);
                } else {
                    self.builder.throw(val_op);
                }
                // Start a dead block for any code after this (unreachable)
                let dead = self.builder.create_block();
                self.builder.set_current_block(dead);
            }

            AstExpr::Lambda(func_def) => {
                self.lower_lambda(&func_def, expr_id, dest);
            }

            AstExpr::OptionalChain { expr } => {
                self.lower_optional_chain(expr_id, expr, dest);
            }

            AstExpr::Missing => {
                self.emit_panic_call("parse error", expr_id);
            }

            AstExpr::Spawn {
                name,
                with_exprs,
                body,
            } => {
                self.lower_spawn(expr_id, name, &with_exprs, body, dest);
            }

            AstExpr::Await { future } => {
                self.lower_await(expr_id, future, dest);
            }
        }

        self.builder.current_source_span = prev_span;
    }

    /// Lower `spawn name? with? { body }` into:
    ///   1. A `MakeClosure` for the body wrapped as a 0-arg lambda.
    ///   2. A name temp (string operand or null constant).
    ///   3. An optional config operand from the `with baml.spawn.options(...)`
    ///      clause (BEP-034 spawn options).
    ///   4. A `Terminator::Spawn` writing the resulting Future handle.
    fn lower_spawn(
        &mut self,
        expr_id: AstExprId,
        name: Option<AstExprId>,
        with_exprs: &[AstExprId],
        body: AstExprId,
        dest: Place,
    ) {
        // The AST-lower step has already wrapped the spawn body in a
        // synthetic 0-arg `Expr::Lambda`. Lowering it through the
        // standard expression path emits a `MakeClosure` rvalue, which
        // is exactly what we want as the closure operand to `Spawn`.
        let closure_local = self.builder.temp(Ty::Null {
            attr: TyAttr::default(),
        });
        let closure_place = Place::Local(closure_local);
        self.lower_expr(body, closure_place.clone());
        let closure_op = Operand::Copy(closure_place);

        // Lower the optional name into an operand.
        let name_op = match name {
            Some(name_id) => self.lower_to_operand(name_id),
            None => Operand::Constant(Constant::Null),
        };

        // BEP-034 middleware: with transformers present, package the body
        // closure + name into a `baml.spawn.SpawnParams` instance, apply each
        // `with` expression to it left-to-right (each is a function
        // `(SpawnParams<T, E>) -> SpawnParams<U, F>`), and hand the FINAL
        // params to the spawn as the config operand. The engine reads
        // body/name/group/cancel/detach from its fields — a transformer may
        // have replaced any of them, including the body. Fields are built in
        // declaration order (the engine reads them BY INDEX; see
        // ns_spawn/spawn.baml).
        let config_op = if with_exprs.is_empty() {
            None
        } else {
            let params_local = self.builder.temp(Ty::Null {
                attr: TyAttr::default(),
            });
            self.builder.assign(
                Place::Local(params_local),
                Rvalue::Aggregate {
                    kind: AggregateKind::Class {
                        name: "baml.spawn.SpawnParams".to_string(),
                        type_arg_templates: Vec::new(),
                    },
                    fields: vec![
                        closure_op.clone(),
                        name_op.clone(),
                        Operand::Constant(Constant::Null),
                        Operand::Constant(Constant::Null),
                        Operand::Constant(Constant::Bool(false)),
                    ],
                },
            );
            let unwind = self.catch_context.as_ref().map(|c| c.unwind_target);
            let mut cur = params_local;
            for &with_id in with_exprs {
                let transformer_op = self.lower_to_operand(with_id);
                let next = self.builder.temp(Ty::Null {
                    attr: TyAttr::default(),
                });
                let resume = self.builder.create_block();
                self.builder.call(
                    transformer_op,
                    vec![Operand::Copy(Place::Local(cur))],
                    Place::Local(next),
                    resume,
                    unwind,
                );
                self.builder.set_current_block(resume);
                cur = next;
            }
            Some(Box::new(Operand::Copy(Place::Local(cur))))
        };

        // Allocate the future temp. Phase C uses a defaulted `Null` type
        // for the future local; the TIR-tracked value/error types flow
        // through to runtime via the surrounding context. A follow-up
        // can plumb `Tir2Ty::Future` directly through `convert_tir2_ty`
        // here once we read it from `self.expr_types`.
        let future_local = self.builder.temp(Ty::Null {
            attr: TyAttr::default(),
        });
        let future_place = Place::Local(future_local);

        let resume = self.builder.create_block();
        self.builder
            .spawn(closure_op, name_op, config_op, future_place.clone(), resume);
        self.builder.set_current_block(resume);
        // The result of `spawn` is the Future handle.
        self.builder
            .assign(dest, Rvalue::Use(Operand::Copy(future_place)));
        // Phase C: `expr_id` is recorded for source-span tracking but
        // is not used for type lookup here.
        let _ = expr_id;
    }

    /// Lower `await expr` into a `Terminator::Await` whose destination is
    /// the awaited value.
    fn lower_await(&mut self, _expr_id: AstExprId, future: AstExprId, dest: Place) {
        let future_local = self.builder.temp(Ty::Null {
            attr: TyAttr::default(),
        });
        let future_place = Place::Local(future_local);
        self.lower_expr(future, future_place.clone());

        // `Terminator::Await` requires its destination to be `Place::Local`.
        // If the caller handed us a projection (field/index), await into a
        // temp local and then assign through to the projection — mirrors
        // how `lower_call` normalizes its destination.
        let (await_dest, projection_dest) = match dest {
            Place::Local(_) => (dest, None),
            projection => {
                let tmp = self.builder.temp(Ty::Null {
                    attr: TyAttr::default(),
                });
                (Place::Local(tmp), Some(projection))
            }
        };

        let resume = self.builder.create_block();
        let unwind = self.catch_context.as_ref().map(|c| c.unwind_target);
        self.builder
            .await_(future_place, await_dest.clone(), resume, unwind);
        self.builder.set_current_block(resume);

        if let Some(projection) = projection_dest {
            self.builder
                .assign(projection, Rvalue::Use(Operand::Copy(await_dest)));
        }
    }
}

// ─── Literal helper ───────────────────────────────────────────────────────────

impl LoweringContext<'_> {
    /// Whether `segments` is rooted at the BEP-044 `default` receiver keyword
    /// and that keyword is not shadowed by a local of the same name. See
    /// [`baml_compiler2_ast::DEFAULT_RECEIVER_KEYWORD`].
    fn is_default_receiver_root(&self, segments: &[Name]) -> bool {
        segments
            .first()
            .is_some_and(|s| s.as_str() == baml_compiler2_ast::DEFAULT_RECEIVER_KEYWORD)
            && !self.locals.contains_key(&segments[0])
    }

    fn lower_literal(lit: &AstLiteral) -> Constant {
        use baml_base::Literal;
        match lit {
            Literal::Int(v) => Constant::Int(*v),
            Literal::Bigint(v) => Constant::Bigint(v.clone()),
            Literal::Float(s) => {
                // Literal::Float stores a string representation — parse to f64
                let v: f64 = s.parse().unwrap_or(0.0);
                Constant::Float(v)
            }
            Literal::String(v) => Constant::String(v.clone()),
            Literal::Bool(v) => Constant::Bool(*v),
        }
    }
}

// ─── 3.3: Path expression lowering ───────────────────────────────────────────

#[allow(clippy::elidable_lifetime_names)]
impl<'db> LoweringContext<'db> {
    fn lower_path_expr(&mut self, expr_id: AstExprId, segments: &[Name], dest: Place) {
        // Multi-segment paths (e.g. baml.llm.render_prompt, self.field, obj.method) — check TIR resolution first
        if segments.len() > 1 {
            // Check path_member_resolutions first (set by infer_local_rooted_path for local-rooted paths).
            // This takes priority over the flat resolutions map since infer_local_rooted_path
            // moves resolutions from the flat map into path_member_resolutions.
            if let Some(member_resolutions) = self
                .path_member_resolutions
                .get(&self.expr_metadata_key(expr_id))
                .cloned()
            {
                use baml_compiler2_tir::inference::MemberResolution;
                // The last resolution corresponds to the final segment of the path.
                // - If the last resolution is a BoundMethod/UnboundMethod/Free, this path is a
                //   callee reference; emit a function constant. The receiver will be prepended
                //   by lower_call.
                // - If the last resolution is a Field, this is a pure field-chain access.
                // Note: for paths like `user.profile.items.slice`, the member_resolutions
                // are [Field{profile}, Field{items}, BoundMethod{slice}], so we check last().
                match member_resolutions.last() {
                    Some(MemberResolution::BoundMethod { .. }) => {
                        // Bound method reference: lower receiver and emit MakeBoundMethod.
                        let resolution = member_resolutions.into_iter().last().unwrap();
                        if let Some(item) = resolution_to_item_ref(self.db, &resolution) {
                            let receiver_segments = &segments[..segments.len() - 1];
                            let receiver_op = if receiver_segments.len() == 1 {
                                if let Some(&recv_local) = self.locals.get(&receiver_segments[0]) {
                                    Operand::Copy(Place::Local(recv_local))
                                } else if let Some(cap_idx) =
                                    self.capture_index_for_name_at(expr_id, &receiver_segments[0])
                                {
                                    // Receiver is a captured variable — use capture slot.
                                    Operand::Copy(Place::Capture(cap_idx))
                                } else {
                                    Operand::Constant(Constant::Null)
                                }
                            } else {
                                // Multi-segment receiver (e.g. `cfg.encoder`): lower as field chain.
                                let recv_ty = self.expr_ty(expr_id);
                                let recv_local = self.builder.temp(recv_ty);
                                self.lower_multi_segment_path_as_field_chain(
                                    expr_id,
                                    receiver_segments,
                                    Place::local(recv_local),
                                );
                                Operand::Copy(Place::local(recv_local))
                            };
                            self.builder.assign(
                                dest,
                                Rvalue::MakeBoundMethod {
                                    item_ref: item,
                                    receiver: receiver_op,
                                },
                            );
                            return;
                        }
                    }
                    Some(
                        MemberResolution::UnboundMethod { .. }
                        | MemberResolution::Free { .. }
                        | MemberResolution::InterfaceDefaultMethod { .. },
                    ) => {
                        // Unbound method or free function reference — emit a plain function constant.
                        let resolution = member_resolutions.into_iter().last().unwrap();
                        if let Some(item) = resolution_to_item_ref(self.db, &resolution) {
                            self.builder.assign(
                                dest,
                                Rvalue::Use(Operand::Constant(Constant::Function(item))),
                            );
                            return;
                        }
                    }
                    Some(MemberResolution::Field { .. }) => {
                        // Local-rooted field access — chain field projections.
                        self.lower_multi_segment_path_as_field_chain(expr_id, segments, dest);
                        return;
                    }
                    Some(MemberResolution::Variant { .. }) => {
                        // Handled by expr_types check below.
                    }
                    None => {}
                }
            }

            // Check flat resolutions (set by infer_multi_segment_path for package-rooted paths
            // like baml.fs.open, baml.env.get, etc.).
            if let Some(resolution) = self
                .resolutions
                .get(&self.expr_metadata_key(expr_id))
                .cloned()
            {
                use baml_compiler2_tir::inference::MemberResolution;
                match &resolution {
                    MemberResolution::BoundMethod { .. } => {
                        // Bound method reference via flat resolutions: emit MakeBoundMethod.
                        if let Some(item) = resolution_to_item_ref(self.db, &resolution) {
                            let receiver_segments = &segments[..segments.len() - 1];
                            let receiver_op = if receiver_segments.len() == 1 {
                                if let Some(&recv_local) = self.locals.get(&receiver_segments[0]) {
                                    Operand::Copy(Place::Local(recv_local))
                                } else if let Some(cap_idx) =
                                    self.capture_index_for_name_at(expr_id, &receiver_segments[0])
                                {
                                    // Receiver is a captured variable — use capture slot.
                                    Operand::Copy(Place::Capture(cap_idx))
                                } else {
                                    Operand::Constant(Constant::Null)
                                }
                            } else {
                                let recv_ty = self.expr_ty(expr_id);
                                let recv_local = self.builder.temp(recv_ty);
                                self.lower_multi_segment_path_as_field_chain(
                                    expr_id,
                                    receiver_segments,
                                    Place::local(recv_local),
                                );
                                Operand::Copy(Place::local(recv_local))
                            };
                            self.builder.assign(
                                dest,
                                Rvalue::MakeBoundMethod {
                                    item_ref: item,
                                    receiver: receiver_op,
                                },
                            );
                            return;
                        }
                    }
                    MemberResolution::UnboundMethod { .. }
                    | MemberResolution::Free { .. }
                    | MemberResolution::InterfaceDefaultMethod { .. } => {
                        if let Some(item) = resolution_to_item_ref(self.db, &resolution) {
                            self.builder.assign(
                                dest,
                                Rvalue::Use(Operand::Constant(Constant::Function(item))),
                            );
                            return;
                        }
                    }
                    MemberResolution::Variant { .. } => {
                        // Handled by expr_types check below.
                    }
                    MemberResolution::Field { .. } => {
                        // Local-rooted field access — chain field projections.
                        // The root segment is a local; chain through class fields.
                        self.lower_multi_segment_path_as_field_chain(expr_id, segments, dest);
                        return;
                    }
                }
            }
            // An interface method referenced as a *value* on a generic- or
            // interface-typed receiver (`let f = x.eq`): no single concrete method
            // exists statically, so bind the implementor's method by the
            // receiver's runtime type (captured now). Mirrors the direct-call
            // dispatch in `lower_call`, but yields a bound value. Candidates are
            // resolved *before* lowering the receiver so a field access (no method
            // candidates) falls through without lowering the prefix twice.
            //
            // Unlike the direct-call path this does not strip a trailing
            // type-qualifier segment (`x.Iface.method`): a qualified method
            // *reference* resolves through TIR's `member_resolutions` / flat
            // `resolutions` above and returns before reaching here, so the
            // receiver is always `segments[..len-1]`.
            if segments.len() >= 2
                && let Some(&recv_root_local) = self.locals.get(&segments[0])
            {
                let method_name = segments.last().unwrap().clone();
                let recv_seg_idx = if segments.len() == 2 {
                    0
                } else {
                    segments.len() - 2
                };
                let recv_tir_ty = self
                    .path_segment_types
                    .get(&(self.current_metadata_scope, expr_id, recv_seg_idx))
                    .cloned();
                if let Some((iface_tn, iface_type_args, iface_assoc)) = recv_tir_ty
                    .as_ref()
                    .and_then(|ty| self.interface_dispatch_target_for_tir_ty(ty))
                {
                    let resolved = self.interface_method_candidates_for(
                        &iface_tn,
                        &iface_type_args,
                        &iface_assoc,
                        &method_name,
                        recv_tir_ty.as_ref(),
                    );
                    if !resolved.is_empty() {
                        let receiver_segments = &segments[..segments.len() - 1];
                        let recv_local = self.lower_path_receiver_to_local(
                            expr_id,
                            receiver_segments,
                            recv_root_local,
                        );
                        self.emit_bound_method_candidate_switch(recv_local, &resolved, &dest);
                        return;
                    }
                }
            }
            if self.locals.contains_key(&segments[0])
                || self
                    .capture_index_for_name_at(expr_id, &segments[0])
                    .is_some()
                // BEP-044 wf3 #4: `default.<field>` as a value — the field-chain
                // lowerer maps the `default` root to `self`-viewed-as-interface.
                // (The `default.method(...)` call form is intercepted earlier in
                // `lower_call`, so this only catches the value/field form.)
                || self.is_default_receiver_root(segments)
            {
                self.lower_multi_segment_path_as_field_chain(expr_id, segments, dest);
                return;
            }
            // Check for enum variant (e.g. Status.Active lowered to Path(["Status","Active"]))
            if let Some(Tir2Ty::EnumVariant(qtn, variant, _)) = self
                .expr_types
                .get(&self.expr_metadata_key(expr_id))
                .cloned()
                .as_ref()
            {
                let enum_ref = ItemRef::EnumType {
                    package: qtn.package().clone(),
                    namespace: qtn.namespace().clone(),
                    name: qtn.name().clone(),
                };
                self.builder.assign(
                    dest,
                    Rvalue::Use(Operand::Constant(Constant::EnumVariant {
                        enum_ref,
                        variant: variant.clone(),
                    })),
                );
                return;
            }
            // Namespace intermediate or unresolved — emit null placeholder.
            self.builder
                .assign(dest, Rvalue::Use(Operand::Constant(Constant::Null)));
            return;
        }

        let name = &segments[0];

        let span_start = self
            .source_map
            .as_ref()
            .map(|sm| sm.expr_span(expr_id).start())
            .unwrap_or_default();

        let resolved = resolve_name_at_in_scope(
            self.db,
            self.file,
            span_start,
            name,
            self.scope_func_name.as_ref(),
        );
        match resolved {
            ResolvedName::Local {
                name: local_name, ..
            } => {
                if let Some(&local) = self.locals.get(&local_name) {
                    self.builder
                        .assign(dest, Rvalue::Use(Operand::Copy(Place::Local(local))));
                } else if let Some(cap_idx) = self.capture_index_for_name_at(expr_id, &local_name) {
                    // This variable is captured from an enclosing scope.
                    // Emit a LoadCapture via Place::Capture.
                    self.builder
                        .assign(dest, Rvalue::Use(Operand::Copy(Place::Capture(cap_idx))));
                } else {
                    let msg = format!("unresolved local: {local_name}");
                    self.emit_panic_call(&msg, expr_id);
                }
            }
            ResolvedName::Item(def) => {
                self.lower_item_ref(expr_id, def, dest);
            }
            ResolvedName::Builtin(def) => {
                let item = def_to_item_ref(self.db, def);
                self.builder.assign(
                    dest,
                    Rvalue::Use(Operand::Constant(Constant::Function(item))),
                );
            }
            ResolvedName::Unknown => {
                // If TIR recorded a type for this expr, it was handled as a package
                // path intermediate (e.g. `baml` in `baml.HttpMethod.Get`). Emit a
                // null placeholder — the outer FieldAccess will produce the real value.
                if self
                    .expr_types
                    .contains_key(&self.expr_metadata_key(expr_id))
                {
                    self.builder
                        .assign(dest, Rvalue::Use(Operand::Constant(Constant::Null)));
                } else {
                    let msg = format!("unresolved name: {name}");
                    self.emit_panic_call(&msg, expr_id);
                }
            }
        }
    }

    /// Lower a multi-segment `Path` expression (`a.b.c`) as chained field projections.
    ///
    /// The first segment is resolved as a local variable; subsequent segments are
    /// projected as struct fields (using `class_fields`) or map keys (fallback).
    fn lower_multi_segment_path_as_field_chain(
        &mut self,
        expr_id: AstExprId,
        segments: &[Name],
        dest: Place,
    ) {
        let (mut current_place, mut current_ty) =
            if let Some(&root_local) = self.locals.get(&segments[0]) {
                let place = Place::Local(root_local);
                let ty = if let Some(tir_root) = self.path_root_ty(expr_id) {
                    // If TIR inferred a more specific type for the root local,
                    // update the MIR local's declared type so the emitter can
                    // resolve field names for display (e.g. `load_field .index`).
                    if matches!(self.builder.local_ty(root_local), Ty::BuiltinUnknown { .. })
                        && !matches!(tir_root, Ty::BuiltinUnknown { .. } | Ty::Void { .. })
                    {
                        self.builder.local_decl_mut(root_local).ty = tir_root.clone();
                    }
                    tir_root
                } else {
                    self.builder.local_ty(root_local)
                };
                (place, ty)
            } else if let Some(cap_idx) = self.capture_index_for_name_at(expr_id, &segments[0]) {
                let place = Place::Capture(cap_idx);
                let ty = self
                    .path_root_ty(expr_id)
                    .unwrap_or_else(|| Ty::BuiltinUnknown {
                        attr: TyAttr::default(),
                    });
                (place, ty)
            } else if self.is_default_receiver_root(segments)
                && let Some(&self_local) = self.locals.get(&Name::new("self"))
            {
                // BEP-044 wf3 #4: `default.<field>` denotes the enclosing `self`
                // viewed at the declaring interface. TIR typed the root as
                // `Ty::Interface`, so reuse that and let the interface-prefix
                // routing below resolve the field view (same path as
                // `self.as<I>.field`). Without this the `default` root is not a
                // local → null → `string + null` VM crash.
                let place = Place::Local(self_local);
                let ty = self
                    .path_root_ty(expr_id)
                    .unwrap_or_else(|| self.builder.local_ty(self_local));
                (place, ty)
            } else {
                // Root not found as a local or capture — emit null.
                self.builder
                    .assign(dest, Rvalue::Use(Operand::Constant(Constant::Null)));
                return;
            };

        let mut skip_next_segment = false;
        for (offset, seg) in segments[1..].iter().enumerate() {
            if skip_next_segment {
                skip_next_segment = false;
                continue;
            }
            let seg_idx = offset + 1;
            let is_last = seg_idx + 1 == segments.len();
            let interface_prefix =
                self.interface_receiver_for_path_prefix(expr_id, seg_idx - 1, &current_ty);
            if let Some((tn, class_type_args)) =
                self.class_receiver_for_path_prefix(expr_id, seg_idx - 1, &current_ty)
            {
                if let Some(fields) = self.class_fields.get(&tn) {
                    if let Some(&idx) = fields.get(seg.as_str()) {
                        // Substitute the receiver's class type-args into the
                        // declared field type so chained access through generic
                        // positions (`b.value.name` where `b: Box<User>`)
                        // produces `Ty::Class(User, ...)` rather than the
                        // erased runtime metadata. Without this, the next iteration
                        // falls through to the dynamic map-key path below and the
                        // VM hits `expected Map, got Instance`.
                        let next_ty = self.class_field_ty(&tn, seg, &class_type_args);
                        current_place = Place::Field {
                            base: Box::new(current_place),
                            field: idx,
                        };
                        current_ty = next_ty;
                        continue;
                    }
                    if !is_last {
                        let qualified = Name::new(format!("{}.{}", seg, segments[seg_idx + 1]));
                        if let Some(&idx) = fields.get(qualified.as_str()) {
                            let next_ty = self.class_field_ty(&tn, &qualified, &class_type_args);
                            current_place = Place::Field {
                                base: Box::new(current_place),
                                field: idx,
                            };
                            current_ty = next_ty;
                            skip_next_segment = true;
                            continue;
                        }
                    }
                }
            }

            let target_ty =
                self.path_segment_ty(expr_id, seg_idx)
                    .unwrap_or_else(|| Ty::BuiltinUnknown {
                        attr: TyAttr::default(),
                    });
            let target_place = if is_last {
                dest.clone()
            } else {
                Place::local(self.builder.temp(target_ty.clone()))
            };
            let base_local = match current_place.clone() {
                Place::Local(local) => local,
                place => {
                    let local = self.builder.temp(current_ty.clone());
                    self.builder
                        .assign(Place::local(local), Rvalue::Use(Operand::Copy(place)));
                    local
                }
            };
            if let Some((iface_tn, iface_type_args, iface_assoc)) = interface_prefix
                && self.try_lower_interface_field_access(
                    base_local,
                    &iface_tn,
                    &iface_type_args,
                    &iface_assoc,
                    seg,
                    &target_place,
                )
            {
                if is_last {
                    return;
                }
                current_place = target_place;
                current_ty = target_ty;
                continue;
            }
            if self.lower_union_class_field_access(
                expr_id,
                base_local,
                &current_ty,
                seg,
                &target_place,
            ) {
                if is_last {
                    return;
                }
                current_place = target_place;
                current_ty = target_ty;
                continue;
            }
            // Receiver prefix may be a union containing an interface member
            // (`(Dog | Named).name`): dispatch the field read on the runtime
            // class across all members' implementors.
            if let Some(members) = self
                .path_segment_types
                .get(&(self.current_metadata_scope, expr_id, seg_idx - 1))
                .and_then(Self::tir_union_members)
                && self.lower_union_iface_field_access(base_local, &members, seg, &target_place)
            {
                if is_last {
                    return;
                }
                current_place = target_place;
                current_ty = target_ty;
                continue;
            }

            // Dynamic map key fallback
            let key_local = self.builder.temp(Ty::String {
                attr: TyAttr::default(),
            });
            self.builder.assign(
                Place::local(key_local),
                Rvalue::Use(Operand::Constant(Constant::String(seg.to_string()))),
            );
            current_place = Place::Index {
                base: Box::new(current_place),
                index: key_local,
                kind: IndexKind::Map,
            };
            break;
        }

        self.builder
            .assign(dest, Rvalue::Use(Operand::Copy(current_place)));
    }

    /// Look up the MIR type of a named field on a class, for chained field access.
    ///
    /// `class_type_args` are the type-args carried on the receiver's
    /// `Ty::Class(tn, class_type_args, _)` (e.g. `[User]` for `Box<User>`).
    /// They are substituted into the declared field type so a generic-typed
    /// position (`item: T` in `Container<T>`) resolves to the concrete
    /// receiver-side binding rather than `Ty::Void`.
    ///
    /// Returns `Ty::Null` if the field is not found or the type cannot be
    /// resolved.  Called by `lower_multi_segment_path_as_field_chain` to
    /// track the type through a chain of field projections (`a.b.c` needs
    /// the type of `b` to find `c`).
    fn class_field_ty(&self, class_tn: &TypeName, field_name: &Name, class_type_args: &[Ty]) -> Ty {
        use baml_compiler2_hir::{contributions::Definition, package::package_items};
        use baml_compiler2_tir::lower_type_expr::lower_type_expr_in_ns;
        let db = self.db;

        let pkg_name = class_tn.package();
        let pkg_id = baml_compiler2_hir::package::PackageId::new(db, pkg_name.clone());
        let pkg_items_ref = package_items(db, pkg_id);

        let namespace: Vec<Name> = class_tn.namespace().clone();

        let Some(Definition::Class(class_loc)) =
            pkg_items_ref.lookup_type(&namespace, class_tn.name())
        else {
            return Ty::Null {
                attr: TyAttr::default(),
            };
        };

        let item_tree = baml_compiler2_ppir::file_item_tree(db, class_loc.file(db));
        let class_data = &item_tree[class_loc.id(db)];

        let field = class_data.fields.iter().find(|f| &f.name == field_name);
        let Some(field) = field else {
            return Ty::Null {
                attr: TyAttr::default(),
            };
        };
        let Some(ref te) = field.type_expr else {
            return Ty::Null {
                attr: TyAttr::default(),
            };
        };

        let pkg_ns =
            baml_compiler2_hir::file_package::file_package(db, class_loc.file(db)).namespace_path;
        let mut diags = Vec::new();
        let tir_ty = lower_type_expr_in_ns(
            db,
            &te.expr,
            pkg_items_ref,
            &pkg_ns,
            &class_data.generic_params,
            &mut diags,
        );
        // Build a TyTemplate with `TypeArgRef(N)` for each class-level
        // generic param, then substitute `class_type_args` so a field
        // declared as `T` resolves to the concrete receiver-side binding.
        let template = tir2_to_template(&tir_ty, self.resolved_aliases, &class_data.generic_params);
        template.substitute(class_type_args)
    }

    fn lower_item_ref(&mut self, expr_id: AstExprId, def: Definition<'db>, dest: Place) {
        let item = def_to_item_ref(self.db, def);
        // Check if this expression's type is EnumVariant
        if let Some(Tir2Ty::EnumVariant(_qtn, variant, _)) = self
            .expr_types
            .get(&self.expr_metadata_key(expr_id))
            .cloned()
            .as_ref()
        {
            let variant_name = variant.clone();
            // Convert the Free item ref to an EnumType variant
            let enum_ref = match item {
                ItemRef::Free {
                    package,
                    namespace,
                    name,
                } => ItemRef::EnumType {
                    package,
                    namespace,
                    name,
                },
                other => other,
            };
            self.builder.assign(
                dest,
                Rvalue::Use(Operand::Constant(Constant::EnumVariant {
                    enum_ref,
                    variant: variant_name,
                })),
            );
            return;
        }
        // Otherwise treat as function/constructor reference
        self.builder.assign(
            dest,
            Rvalue::Use(Operand::Constant(Constant::Function(item))),
        );
    }
}

// ─── 3.4: Operator mapping and binary/unary lowering ─────────────────────────

impl LoweringContext<'_> {
    fn convert_binop(op: AstBinaryOp) -> Option<BinOp> {
        match op {
            AstBinaryOp::Add => Some(BinOp::Add),
            AstBinaryOp::Sub => Some(BinOp::Sub),
            AstBinaryOp::Mul => Some(BinOp::Mul),
            AstBinaryOp::Div => Some(BinOp::Div),
            AstBinaryOp::Mod => Some(BinOp::Mod),
            AstBinaryOp::Eq => Some(BinOp::Eq),
            AstBinaryOp::Ne => Some(BinOp::Ne),
            AstBinaryOp::Lt => Some(BinOp::Lt),
            AstBinaryOp::Le => Some(BinOp::Le),
            AstBinaryOp::Gt => Some(BinOp::Gt),
            AstBinaryOp::Ge => Some(BinOp::Ge),
            AstBinaryOp::BitAnd => Some(BinOp::BitAnd),
            AstBinaryOp::BitOr => Some(BinOp::BitOr),
            AstBinaryOp::BitXor => Some(BinOp::BitXor),
            AstBinaryOp::Shl => Some(BinOp::Shl),
            AstBinaryOp::Shr => Some(BinOp::Shr),
            // Short-circuit operators handled separately
            AstBinaryOp::And | AstBinaryOp::Or => None,
            // Null coalescing desugars to control flow, not a binary op
            AstBinaryOp::NullCoalesce => None,
        }
    }

    fn lower_binary(
        &mut self,
        expr_id: AstExprId,
        op: AstBinaryOp,
        lhs: AstExprId,
        rhs: AstExprId,
        dest: Place,
    ) {
        match op {
            AstBinaryOp::And => {
                return self.lower_short_circuit(expr_id, lhs, rhs, dest, true);
            }
            AstBinaryOp::Or => {
                return self.lower_short_circuit(expr_id, lhs, rhs, dest, false);
            }
            AstBinaryOp::NullCoalesce => {
                return self.lower_null_coalesce(expr_id, lhs, rhs, dest);
            }
            _ => {}
        }

        // Check if TIR already folded this expression to a literal constant
        if self.opt >= crate::OptLevel::Two {
            if let Ty::Literal(ref lit, _, _) = self.expr_ty(expr_id) {
                let constant = Self::lower_literal(lit);
                self.builder
                    .assign(dest, Rvalue::Use(Operand::Constant(constant)));
                return;
            }
        }

        // Mixed `int OP bigint` (or `bigint OP int`) operators resolve the
        // `int` operand to a small local `BigInt` in the VM (the specialized
        // `*Bigint`/`CmpBigint` opcodes accept a lone `int` operand), without
        // allocating a heap bigint. `int` is not a subtype of `bigint` and
        // there is no implicit move coercion — only these operators and the FFI
        // boundary convert — so lower both operands naturally.
        let left = self.lower_to_operand(lhs);
        let right = self.lower_to_operand(rhs);
        if let Some(mir_op) = Self::convert_binop(op) {
            self.builder.assign(
                dest,
                Rvalue::BinaryOp {
                    op: mir_op,
                    left,
                    right,
                },
            );
        } else {
            // Fallback — shouldn't happen for well-typed code
            self.emit_panic_call("unsupported binary op", expr_id);
        }
    }

    fn lower_short_circuit(
        &mut self,
        _expr_id: AstExprId,
        lhs: AstExprId,
        rhs: AstExprId,
        dest: Place,
        is_and: bool,
    ) {
        // `ShortCircuit`'s destination must be a `Place::Local`: the emitter
        // materializes it with `emit_store_place` on the short-circuit edge,
        // which does not handle Field/Index projections. Normalize through a
        // temp and assign through at the join — mirrors `lower_await`.
        let (sc_dest, projection_dest) = match dest {
            Place::Local(_) => (dest, None),
            projection => {
                let tmp = self.builder.temp(Ty::Null {
                    attr: TyAttr::default(),
                });
                (Place::Local(tmp), Some(projection))
            }
        };

        let lhs_op = self.lower_to_operand(lhs);

        let bb_rhs = self.builder.create_block();
        let bb_join = self.builder.create_block();

        // ShortCircuit terminator: JumpIfFalse (peek) keeps lhs on TOS when
        // short-circuiting. The rhs block evaluates and leaves its result on
        // TOS. At join, dest is on TOS when the destination local is
        // stack-carried (PhiLike); otherwise the emitter stores to its slot
        // on both edges and the join reads the slot.
        self.builder
            .short_circuit(lhs_op, is_and, sc_dest.clone(), bb_rhs, bb_join);

        self.builder.set_current_block(bb_rhs);
        self.lower_expr(rhs, sc_dest.clone());
        if !self.builder.is_current_terminated() {
            self.builder.goto(bb_join);
        }

        self.builder.set_current_block(bb_join);
        if let Some(projection) = projection_dest {
            self.builder
                .assign(projection, Rvalue::Use(Operand::Copy(sc_dest)));
        }
    }

    /// Lower `a ?? b` — evaluate `a`, if null then evaluate `b`, otherwise use `a`.
    fn lower_null_coalesce(
        &mut self,
        _expr_id: AstExprId,
        lhs: AstExprId,
        rhs: AstExprId,
        dest: Place,
    ) {
        // Evaluate LHS and store in dest
        let lhs_op = self.lower_to_operand(lhs);
        self.builder
            .assign(dest.clone(), Rvalue::Use(lhs_op.clone()));

        // Test: lhs == null
        let is_null = Rvalue::BinaryOp {
            op: BinOp::Eq,
            left: lhs_op,
            right: Operand::Constant(Constant::Null),
        };
        let test_local = self.builder.temp(Ty::Bool {
            attr: TyAttr::default(),
        });
        self.builder.assign(Place::local(test_local), is_null);

        let bb_rhs = self.builder.create_block();
        let bb_join = self.builder.create_block();

        // If null → evaluate RHS, otherwise keep LHS
        self.builder
            .branch(Operand::Copy(Place::Local(test_local)), bb_rhs, bb_join);

        self.builder.set_current_block(bb_rhs);
        self.lower_expr(rhs, dest);
        if !self.builder.is_current_terminated() {
            self.builder.goto(bb_join);
        }

        self.builder.set_current_block(bb_join);
    }

    /// Lower `OptionalChain { expr }` — set up shared null exit for the entire chain.
    fn lower_optional_chain(&mut self, _expr_id: AstExprId, inner: AstExprId, dest: Place) {
        let bb_null = self.builder.create_block();
        let bb_join = self.builder.create_block();

        // Push shared null exit
        self.chain_null_exits.push(bb_null);

        // Lower inner expression — Optional* nodes will jump to bb_null on null
        self.lower_expr(inner, dest.clone());

        self.chain_null_exits.pop();

        // Non-null path: goto join
        if !self.builder.is_current_terminated() {
            self.builder.goto(bb_join);
        }

        // Null path: assign null, goto join
        self.builder.set_current_block(bb_null);
        self.builder
            .assign(dest, Rvalue::Use(Operand::Constant(Constant::Null)));
        self.builder.goto(bb_join);

        self.builder.set_current_block(bb_join);
    }

    /// Lower an assignment whose target is wrapped in `OptionalChain`.
    /// Sets up null guards, then emits the assignment only on the non-null path.
    fn lower_assign_optional_chain(&mut self, inner_target: AstExprId, value: AstExprId) {
        let bb_null = self.builder.create_block();
        let bb_join = self.builder.create_block();

        // Push shared null exit — Optional* nodes inside will jump here on null
        self.chain_null_exits.push(bb_null);

        // Lower target as lvalue (this will trigger null checks at each ?. node)
        let place = self.lower_lvalue(inner_target);

        // Lower value and assign.
        self.lower_expr(value, place);

        self.chain_null_exits.pop();

        // Non-null path: goto join
        if !self.builder.is_current_terminated() {
            self.builder.goto(bb_join);
        }

        // Null path: skip assignment, goto join
        self.builder.set_current_block(bb_null);
        self.builder.goto(bb_join);

        self.builder.set_current_block(bb_join);
    }

    /// Lower a compound assignment (+=, etc.) whose target is wrapped in `OptionalChain`.
    fn lower_assign_op_optional_chain(
        &mut self,
        inner_target: AstExprId,
        op: AstAssignOp,
        value: AstExprId,
    ) {
        let bb_null = self.builder.create_block();
        let bb_join = self.builder.create_block();

        self.chain_null_exits.push(bb_null);

        let place = self.lower_lvalue(inner_target);
        let current = Operand::Copy(place.clone());
        // Mixed `bigint OP= int` does NOT widen the int rhs: the specialized
        // `*Bigint` opcodes accept a lone `int` operand and resolve it in the
        // VM without allocating a heap bigint (mirrors the plain `AssignOp`
        // path). Lower the value naturally.
        let rhs = self.lower_to_operand(value);
        let mir_op = Self::convert_assign_op(op);
        self.builder.assign(
            place,
            Rvalue::BinaryOp {
                op: mir_op,
                left: current,
                right: rhs,
            },
        );

        self.chain_null_exits.pop();

        if !self.builder.is_current_terminated() {
            self.builder.goto(bb_join);
        }

        self.builder.set_current_block(bb_null);
        self.builder.goto(bb_join);

        self.builder.set_current_block(bb_join);
    }

    /// Lower `obj?.member` — null-check obj, then access member or produce null.
    fn lower_optional_member_access(
        &mut self,
        expr_id: AstExprId,
        base: AstExprId,
        field: &Name,
        dest: Place,
    ) {
        let base_op = self.lower_to_operand(base);

        // Test: base == null
        let is_null = Rvalue::BinaryOp {
            op: BinOp::Eq,
            left: base_op,
            right: Operand::Constant(Constant::Null),
        };
        let test_local = self.builder.temp(Ty::Bool {
            attr: TyAttr::default(),
        });
        self.builder.assign(Place::local(test_local), is_null);

        let bb_access = self.builder.create_block();

        if let Some(&bb_null) = self.chain_null_exits.last() {
            // Inside an OptionalChain — jump to shared null exit
            self.builder
                .branch(Operand::Copy(Place::Local(test_local)), bb_null, bb_access);

            self.builder.set_current_block(bb_access);
            self.lower_member_access(expr_id, base, field, dest);
            // Don't create our own join — the OptionalChain handler does that
        } else {
            // Standalone (no wrapping OptionalChain) — fall back to own null/join blocks
            let bb_null = self.builder.create_block();
            let bb_join = self.builder.create_block();

            self.builder
                .branch(Operand::Copy(Place::Local(test_local)), bb_null, bb_access);

            self.builder.set_current_block(bb_access);
            self.lower_member_access(expr_id, base, field, dest.clone());
            if !self.builder.is_current_terminated() {
                self.builder.goto(bb_join);
            }

            self.builder.set_current_block(bb_null);
            self.builder
                .assign(dest, Rvalue::Use(Operand::Constant(Constant::Null)));
            self.builder.goto(bb_join);

            self.builder.set_current_block(bb_join);
        }
    }

    /// Lower `obj?.[index]` — null-check obj, then index or produce null.
    fn lower_optional_index(
        &mut self,
        expr_id: AstExprId,
        base: AstExprId,
        index: AstExprId,
        dest: Place,
    ) {
        let base_op = self.lower_to_operand(base);

        let is_null = Rvalue::BinaryOp {
            op: BinOp::Eq,
            left: base_op,
            right: Operand::Constant(Constant::Null),
        };
        let test_local = self.builder.temp(Ty::Bool {
            attr: TyAttr::default(),
        });
        self.builder.assign(Place::local(test_local), is_null);

        let bb_access = self.builder.create_block();

        if let Some(&bb_null) = self.chain_null_exits.last() {
            self.builder
                .branch(Operand::Copy(Place::Local(test_local)), bb_null, bb_access);

            self.builder.set_current_block(bb_access);
            self.lower_index(expr_id, base, index, dest);
        } else {
            let bb_null = self.builder.create_block();
            let bb_join = self.builder.create_block();

            self.builder
                .branch(Operand::Copy(Place::Local(test_local)), bb_null, bb_access);

            self.builder.set_current_block(bb_access);
            self.lower_index(expr_id, base, index, dest.clone());
            if !self.builder.is_current_terminated() {
                self.builder.goto(bb_join);
            }

            self.builder.set_current_block(bb_null);
            self.builder
                .assign(dest, Rvalue::Use(Operand::Constant(Constant::Null)));
            self.builder.goto(bb_join);

            self.builder.set_current_block(bb_join);
        }
    }

    /// Lower `func?.(args)` — null-check callee, then call or produce null.
    fn lower_optional_call(
        &mut self,
        expr_id: AstExprId,
        callee: AstExprId,
        args: &[AstExprId],
        dest: Place,
    ) {
        let callee_op = self.lower_to_operand(callee);

        let is_null = Rvalue::BinaryOp {
            op: BinOp::Eq,
            left: callee_op,
            right: Operand::Constant(Constant::Null),
        };
        let test_local = self.builder.temp(Ty::Bool {
            attr: TyAttr::default(),
        });
        self.builder.assign(Place::local(test_local), is_null);

        let bb_call = self.builder.create_block();

        if let Some(&bb_null) = self.chain_null_exits.last() {
            self.builder
                .branch(Operand::Copy(Place::Local(test_local)), bb_null, bb_call);

            self.builder.set_current_block(bb_call);
            self.lower_call(expr_id, callee, args, dest);
        } else {
            let bb_null = self.builder.create_block();
            let bb_join = self.builder.create_block();

            self.builder
                .branch(Operand::Copy(Place::Local(test_local)), bb_null, bb_call);

            self.builder.set_current_block(bb_call);
            self.lower_call(expr_id, callee, args, dest.clone());
            if !self.builder.is_current_terminated() {
                self.builder.goto(bb_join);
            }

            self.builder.set_current_block(bb_null);
            self.builder
                .assign(dest, Rvalue::Use(Operand::Constant(Constant::Null)));
            self.builder.goto(bb_join);

            self.builder.set_current_block(bb_join);
        }
    }

    fn lower_unary(&mut self, expr_id: AstExprId, op: AstUnaryOp, expr: AstExprId, dest: Place) {
        // Check if TIR already folded this expression to a literal constant
        if self.opt >= crate::OptLevel::Two {
            if let Ty::Literal(ref lit, _, _) = self.expr_ty(expr_id) {
                let constant = Self::lower_literal(lit);
                self.builder
                    .assign(dest, Rvalue::Use(Operand::Constant(constant)));
                return;
            }
        }
        let operand = self.lower_to_operand(expr);
        let mir_op = match op {
            AstUnaryOp::Not => crate::UnaryOp::Not,
            AstUnaryOp::Neg => crate::UnaryOp::Neg,
        };
        self.builder.assign(
            dest,
            Rvalue::UnaryOp {
                op: mir_op,
                operand,
            },
        );
    }
}

// ─── 3.5: Call lowering with builtin detection ────────────────────────────────

impl LoweringContext<'_> {
    fn lower_call_arg_operands(&mut self, expr_id: AstExprId, args: &[AstExprId]) -> Vec<Operand> {
        let Some(plan) = self
            .call_plans
            .get(&self.expr_metadata_key(expr_id))
            .cloned()
        else {
            // No call plan: lower each arg in order (the type checker would
            // have already flagged any mismatch).
            return args.iter().map(|&a| self.lower_to_operand(a)).collect();
        };

        // Pre-lower each provided arg in source order (the order `args` appear
        // in the call expression). This preserves the original evaluation
        // order, which matters for side effects.
        let provided_args: Vec<_> = plan.provided_args().collect();
        let mut lowered_args: FxHashMap<AstExprId, Operand> = FxHashMap::default();
        for &arg in args {
            if provided_args.contains(&arg) {
                lowered_args.insert(arg, self.lower_to_operand(arg));
            }
        }

        plan.bindings
            .into_iter()
            .map(|binding| match binding {
                baml_compiler2_tir::inference::ParamBinding::Provided { arg, .. } => lowered_args
                    .remove(&arg)
                    .expect("call plan referenced an argument outside the call expression"),
                baml_compiler2_tir::inference::ParamBinding::OmittedDefault { .. } => {
                    Operand::Constant(Constant::OmittedArg)
                }
            })
            .collect()
    }

    fn lower_call(
        &mut self,
        expr_id: AstExprId,
        callee: AstExprId,
        args: &[AstExprId],
        dest: Place,
    ) {
        // Check if callee is a member access (potential watch method call)
        let callee_expr = self.body.exprs[callee].clone();
        if let AstExpr::MemberAccess { base, member } = &callee_expr {
            let member_name = member.clone();
            let base_id = *base;
            if member_name.as_str() == "options" || member_name.as_str() == "notify" {
                let args_owned = args.to_vec();
                self.lower_watch_method(expr_id, base_id, &member_name, &args_owned, dest);
                return;
            }
            // BEP-044: interface-typed receiver — dispatch by type tag over
            // the registered implementor set. Each arm emits a static call
            // to that implementor's method.
            if self.try_lower_interface_dispatch(expr_id, base_id, &member_name, args, &dest) {
                return;
            }
            // Receiver may be a union of concrete classes sharing the method
            // (e.g. `(if c { Dog {} } else { Cat {} }).speak()`).
            if self.try_lower_union_dispatch(expr_id, base_id, &member_name, args, &dest) {
                return;
            }
            // Receiver may be a union containing an interface member
            // (e.g. `Animal | Vehicle`), where every member declares the
            // method — dispatch on the runtime class across all implementors.
            if self.try_lower_union_iface_dispatch(expr_id, base_id, &member_name, args, &dest) {
                return;
            }
        }
        // BEP-044: `default.<method>(...)` inside an `implements I { ... }`
        // block emits a static call to `I`'s default function, with the
        // class's `self` forwarded as the receiver. No type-tag switch —
        // the override is being deliberately bypassed.
        if let AstExpr::Path(segments) = &callee_expr
            && segments.len() == 2
            && self.is_default_receiver_root(segments)
            && let Some(target_te) = self.implements_block_iface_target()
            && let baml_compiler2_ast::TypeExpr::Path { .. } = &target_te.expr
        {
            let current_pkg = baml_compiler2_hir::file_package::file_package(self.db, self.file);
            let pkg_id = PackageId::new(self.db, current_pkg.package.clone());
            let pkg_items = package_items(self.db, pkg_id);
            if let Some(iface_loc) = baml_compiler2_tir::interfaces::resolve_path_to_interface(
                self.db,
                &target_te.expr,
                pkg_items,
                &current_pkg.namespace_path,
            ) {
                let iface_pkg = baml_compiler2_hir::file_package::file_package(
                    self.db,
                    iface_loc.file(self.db),
                );
                let iface_tree = file_item_tree(self.db, iface_loc.file(self.db));
                let iface_name = iface_tree[iface_loc.id(self.db)].name.clone();
                let method_name = segments[1].clone();
                let item_ref = ItemRef::Method {
                    package: iface_pkg.package.clone(),
                    namespace: iface_pkg.namespace_path,
                    class: iface_name,
                    name: method_name,
                };
                let callee_op = Operand::Constant(Constant::Function(item_ref));
                let Some(&self_local) = self.locals.get(&Name::new("self")) else {
                    return;
                };
                // Seed the default method's frame with the interface's type
                // args, expressed over the enclosing class's generic params
                // (e.g. `implements Cont<T>` → `[T]`). The default body lowers
                // the interface's `T` to `TypeArgRef` (see
                // `enclosing_generic_params`), so without this an explicit
                // `default.<method>()` that reads `T` would resolve it to
                // `unknown` at runtime. Mirrors the interface-dispatch switch.
                let iface_type_arg_tys: Vec<Tir2Ty> = if let baml_compiler2_ast::TypeExpr::Path {
                    generic_args,
                    ..
                } = &target_te.expr
                {
                    let generic_params = self.enclosing_generic_params();
                    let mut diags = Vec::new();
                    generic_args
                        .iter()
                        .map(|arg| {
                            baml_compiler2_tir::lower_type_expr::lower_type_expr_in_ns(
                                self.db,
                                arg,
                                pkg_items,
                                &current_pkg.namespace_path,
                                &generic_params,
                                &mut diags,
                            )
                        })
                        .collect()
                } else {
                    vec![]
                };
                let frame_type_arg_ops = self.emit_frame_type_arg_ops(&iface_type_arg_tys);
                let ntypeargs = frame_type_arg_ops.len();
                let mut all_args = frame_type_arg_ops;
                all_args.push(Operand::Copy(Place::Local(self_local)));
                all_args.extend(self.lower_call_arg_operands(expr_id, args));
                let target = self.builder.create_block();
                let unwind = self.catch_context.as_ref().map(|c| c.unwind_target);
                self.builder
                    .call_with_type_args(callee_op, all_args, ntypeargs, dest, target, unwind);
                self.builder.set_current_block(target);
                return;
            }
        }
        // BEP-044: intercept Path forms whose final segment is a method
        // call on an interface-typed receiver.
        //
        //   `<local>.<method>()` (2 segments) — receiver inferred interface
        //   `<local>.<field>.<method>()` (3+ segments) — field chain whose
        //   prefix is interface-typed
        if let AstExpr::Path(segments) = &callee_expr {
            // Any path of length ≥ 2 may end in a method call whose
            // receiver is interface-typed. The receiver type is recorded
            // by TIR at the segment just before the method name (or, for
            // a 2-segment path, is the root local's declared type).
            //
            // The segment just before the method name may be a real field
            // access (`r.a.b.c.d.e.speak()`) whose static type is an interface.
            if segments.len() >= 2
                && let Some(&recv_root_local) = self.locals.get(&segments[0])
            {
                let method_name = segments.last().unwrap().clone();
                let prefix_idx = segments.len() - 2;
                let recv_seg_idx = if segments.len() == 2 { 0 } else { prefix_idx };
                let recv_tir_ty = self
                    .path_segment_types
                    .get(&(self.current_metadata_scope, callee, recv_seg_idx))
                    .cloned()
                    .or_else(|| {
                        if segments.len() == 2
                            && segments[0].as_str() == "self"
                            && self.generic_param_bounds.contains_key(&Name::new("Self"))
                        {
                            Some(Tir2Ty::TypeVar(
                                Name::new("Self"),
                                baml_compiler2_tir::ty::TyAttr::default(),
                            ))
                        } else {
                            None
                        }
                    })
                    // A lambda-parameter receiver (`(a: T) -> a.compare(b)`) has
                    // no `path_segment_types` entry; recover its declared type so
                    // a method on its (bounded) type variable dispatches.
                    .or_else(|| {
                        if segments.len() == 2 {
                            self.lambda_param_tir_types.get(&segments[0]).cloned()
                        } else {
                            None
                        }
                    });
                let iface_dispatch_opt: Option<InterfaceTypeView> = if segments.len() == 2 {
                    if let Some(target) = self
                        .source_param_interface_view_for_name_at(callee, &segments[0])
                        .or_else(|| {
                            recv_tir_ty
                                .as_ref()
                                .and_then(|ty| self.interface_dispatch_target_for_tir_ty(ty))
                        })
                    {
                        Some(target)
                    } else {
                        match self.builder.local_ty(recv_root_local) {
                            Ty::Class(n, _, _) if self.interface_implementors.contains_key(&n) => {
                                Some((n, Vec::new(), Vec::new()))
                            }
                            _ => None,
                        }
                    }
                } else {
                    recv_tir_ty
                        .as_ref()
                        .and_then(|ty| self.interface_dispatch_target_for_tir_ty(ty))
                }
                // BEP-044 wf3 #G7: concrete receiver whose method comes from a
                // blanket / out-of-body impl — find the providing interface via
                // the registry and dispatch through the normal switch.
                .or_else(|| {
                    recv_tir_ty
                        .as_ref()
                        .and_then(|ty| self.registry_dispatch_target_for_concrete(ty, &method_name))
                });
                if let Some((iface_tn, iface_type_args, iface_assoc)) = iface_dispatch_opt {
                    // Decide how many leading segments form the receiver
                    // value (the rest are type qualifiers).
                    let prefix_is_qualifier = segments.len() >= 3
                        && segments[prefix_idx].as_str() == iface_tn.name().as_str();
                    let receiver_segments_end = if prefix_is_qualifier {
                        prefix_idx
                    } else {
                        segments.len() - 1
                    };
                    let receiver_segments = &segments[..receiver_segments_end];
                    let recv_local = self.lower_path_receiver_to_local(
                        callee,
                        receiver_segments,
                        recv_root_local,
                    );
                    if self.emit_interface_dispatch_switch(
                        InterfaceDispatchCall {
                            expr_id,
                            recv_local,
                            recv_tir_ty: recv_tir_ty.as_ref(),
                            iface_tn: &iface_tn,
                            iface_type_args: &iface_type_args,
                            iface_assoc: &iface_assoc,
                            method: &method_name,
                            args,
                        },
                        &dest,
                    ) {
                        return;
                    }
                }
                // Parallel to the interface case: the receiver may instead be a
                // union of concrete classes (a local or field chain bound to a
                // `match`/`if` whose arms are different classes). Same receiver
                // type slot, same field-chain lowering.
                else if let Some(members) = self
                    .path_segment_types
                    .get(&(self.current_metadata_scope, callee, prefix_idx))
                    .and_then(Self::tir_union_members)
                {
                    let receiver_segments = &segments[..segments.len() - 1];
                    let recv_local = self.lower_path_receiver_to_local(
                        callee,
                        receiver_segments,
                        recv_root_local,
                    );
                    if self.emit_union_class_dispatch(
                        recv_local,
                        &members,
                        &method_name,
                        expr_id,
                        args,
                        &dest,
                    ) {
                        return;
                    }
                    // Union containing an interface member: dispatch on the
                    // runtime class across all implementors.
                    if let Some(candidates) =
                        self.union_iface_method_candidates(&members, &method_name)
                        && self.emit_method_candidate_switch(
                            recv_local,
                            &candidates,
                            expr_id,
                            args,
                            &dest,
                            None,
                        )
                    {
                        return;
                    }
                }
            }
        }

        // Check if callee is a method call (MemberAccess or multi-segment Path with a
        // MemberResolution::BoundMethod/UnboundMethod/Free). Field and Variant resolutions are not callable.
        // If the base is a real value (not a package namespace), prepend it as self.
        let mut receiver_base_for_class_type_args: Option<AstExprId> = None;
        let mut receiver_path_tir_ty: Option<Tir2Ty> = None;
        let (callee_operand, arg_operands) = if let AstExpr::MemberAccess { base, .. } =
            &callee_expr
        {
            if self
                .resolutions
                .get(&self.expr_metadata_key(callee))
                .is_some_and(|r| {
                    use baml_compiler2_tir::inference::MemberResolution;
                    matches!(
                        r,
                        MemberResolution::BoundMethod { .. }
                            | MemberResolution::UnboundMethod { .. }
                            | MemberResolution::Free { .. }
                            | MemberResolution::InterfaceDefaultMethod { .. }
                    )
                })
            {
                // Check if base is a value receiver or a bare type/package path.
                // Type-name bases like `Label<int>.method` can have concrete
                // TIR types (`Interface`, `Class`) but are not runtime values.
                let base_is_value = match &self.body.exprs[*base] {
                    AstExpr::Path(segments) if !segments.is_empty() => {
                        self.locals.contains_key(&segments[0])
                            || self
                                .capture_index_for_name_at(*base, &segments[0])
                                .is_some()
                    }
                    _ => self
                        .expr_types
                        .get(&self.expr_metadata_key(*base))
                        .map(|ty| !matches!(ty, Tir2Ty::Unknown { .. }))
                        .unwrap_or(false),
                };
                // Check if the resolved method expects a `self` receiver.
                // Static methods (e.g. StreamCache.new) have no `self` param
                // and must not get the class reference prepended as an argument.
                let method_takes_self = {
                    use baml_compiler2_tir::inference::MemberResolution;
                    self.resolutions
                        .get(&self.expr_metadata_key(callee))
                        .is_some_and(|r| match r {
                            MemberResolution::BoundMethod { func_loc, .. }
                            | MemberResolution::UnboundMethod { func_loc, .. }
                            | MemberResolution::Free { func_loc }
                            | MemberResolution::InterfaceDefaultMethod { func_loc, .. } => {
                                let sig =
                                    baml_compiler2_ppir::function_signature(self.db, *func_loc);
                                sig.params
                                    .first()
                                    .is_some_and(|param| param.name.as_str() == "self")
                            }
                            _ => false,
                        })
                };
                if base_is_value && method_takes_self {
                    // Instance method call: arr.length() — prepend receiver as self.
                    // For immediate calls, emit the callee as a plain function constant
                    // (not MakeBoundMethod) since the receiver is passed explicitly as self.
                    let receiver_op = self.lower_to_operand(*base);
                    receiver_base_for_class_type_args = Some(*base);
                    let callee_op = {
                        let resolution = self
                            .resolutions
                            .get(&self.expr_metadata_key(callee))
                            .cloned();
                        match resolution
                            .as_ref()
                            .and_then(|r| resolution_to_item_ref(self.db, r))
                        {
                            Some(item) => Operand::Constant(Constant::Function(item)),
                            None => self.lower_to_operand(callee),
                        }
                    };
                    let mut all_args = vec![receiver_op];
                    all_args.extend(self.lower_call_arg_operands(expr_id, args));
                    (callee_op, all_args)
                } else {
                    // Non-self method or package function reference:
                    // e.g. Factory<int>.create(42), baml.Array.length(array).
                    // Resolve the callee as a plain function constant using
                    // resolution_to_item_ref to avoid lower_member_access emitting
                    // MakeBoundMethod (which would try to load the base type as a
                    // runtime value).
                    let callee_op = {
                        let resolution = self
                            .resolutions
                            .get(&self.expr_metadata_key(callee))
                            .cloned();
                        match resolution
                            .as_ref()
                            .and_then(|r| resolution_to_item_ref(self.db, r))
                        {
                            Some(item) => Operand::Constant(Constant::Function(item)),
                            None => self.lower_to_operand(callee),
                        }
                    };
                    (callee_op, self.lower_call_arg_operands(expr_id, args))
                }
            } else {
                let callee_op = self.lower_to_operand(callee);
                (callee_op, self.lower_call_arg_operands(expr_id, args))
            }
        } else if let AstExpr::Path(segments) = &callee_expr {
            // Check path_member_resolutions first (local-rooted paths like `self.method()`
            // or `obj.field.method()`). The last resolution determines if the final segment
            // is a method call (e.g. for `user.profile.items.slice`, resolutions are
            // [Field{profile}, Field{items}, Method{slice}] — last() is Method).
            let is_local_method = segments.len() >= 2
                && self
                    .path_member_resolutions
                    .get(&self.expr_metadata_key(callee))
                    .and_then(|resolutions| resolutions.last())
                    .is_some_and(|r| {
                        use baml_compiler2_tir::inference::MemberResolution;
                        matches!(
                            r,
                            MemberResolution::BoundMethod { .. }
                                | MemberResolution::UnboundMethod { .. }
                                | MemberResolution::InterfaceDefaultMethod { .. }
                        )
                    });
            // Also check flat resolutions (package-path method call, kept for compatibility).
            let is_pkg_method = !is_local_method
                && segments.len() >= 2
                && self
                    .resolutions
                    .get(&self.expr_metadata_key(callee))
                    .is_some_and(|r| {
                        use baml_compiler2_tir::inference::MemberResolution;
                        matches!(
                            r,
                            MemberResolution::BoundMethod { .. }
                                | MemberResolution::UnboundMethod { .. }
                                | MemberResolution::InterfaceDefaultMethod { .. }
                        )
                    });

            if is_local_method {
                // Multi-segment path callee with a local-rooted Method resolution.
                // The last segment is the method; segments[0..n-1] form the receiver.
                // e.g. `self.method()` → receiver=self, `user.profile.items.slice()` → receiver=user.profile.items.
                //
                // For immediate calls we emit the callee as a plain function constant
                // (not MakeBoundMethod) since the receiver is passed explicitly as self.
                let receiver_segments = &segments[..segments.len() - 1];
                let method_resolution = self
                    .path_member_resolutions
                    .get(&self.expr_metadata_key(callee))
                    .and_then(|resolutions| resolutions.last())
                    .cloned();
                let callee_op = match method_resolution
                    .as_ref()
                    .and_then(|r| resolution_to_item_ref(self.db, r))
                {
                    Some(item) => Operand::Constant(Constant::Function(item)),
                    None => self.lower_to_operand(callee),
                };
                let method_takes_self = method_resolution.as_ref().is_some_and(|r| {
                    use baml_compiler2_tir::inference::MemberResolution;
                    match r {
                        MemberResolution::BoundMethod { func_loc, .. }
                        | MemberResolution::UnboundMethod { func_loc, .. }
                        | MemberResolution::InterfaceDefaultMethod { func_loc, .. } => {
                            let sig = baml_compiler2_ppir::function_signature(self.db, *func_loc);
                            sig.params
                                .first()
                                .is_some_and(|param| param.name.as_str() == "self")
                        }
                        _ => false,
                    }
                });
                if !method_takes_self {
                    (callee_op, self.lower_call_arg_operands(expr_id, args))
                } else {
                    let receiver_op = if receiver_segments.len() == 1 {
                        // Simple local variable receiver (e.g. `self`).
                        if let Some(&recv_local) = self.locals.get(&receiver_segments[0]) {
                            Operand::Copy(Place::Local(recv_local))
                        } else if let Some(cap_idx) =
                            self.capture_index_for_name_at(callee, &receiver_segments[0])
                        {
                            Operand::Copy(Place::Capture(cap_idx))
                        } else {
                            Operand::Constant(Constant::Null)
                        }
                    } else {
                        // Multi-segment receiver (e.g. `user.profile.items`): lower as field chain.
                        let recv_ty = self.expr_ty(callee); // approximation; actual type not critical here
                        let recv_local = self.builder.temp(recv_ty);
                        self.lower_multi_segment_path_as_field_chain(
                            callee,
                            receiver_segments,
                            Place::local(recv_local),
                        );
                        Operand::Copy(Place::local(recv_local))
                    };
                    let prefix_idx = segments.len() - 2;
                    receiver_path_tir_ty = self
                        .path_segment_types
                        .get(&(self.current_metadata_scope, callee, prefix_idx))
                        .cloned();
                    let mut all_args = vec![receiver_op];
                    all_args.extend(self.lower_call_arg_operands(expr_id, args));
                    (callee_op, all_args)
                }
            } else if is_pkg_method {
                // Package-path method call (via flat resolutions): same treatment.
                // For immediate calls, emit the callee as a plain function constant
                // (not MakeBoundMethod) since the receiver is passed explicitly as self.
                let flat_resolution = self
                    .resolutions
                    .get(&self.expr_metadata_key(callee))
                    .cloned();
                let callee_op = match flat_resolution
                    .as_ref()
                    .and_then(|r| resolution_to_item_ref(self.db, r))
                {
                    Some(item) => Operand::Constant(Constant::Function(item)),
                    None => self.lower_to_operand(callee),
                };
                let first_seg = &segments[0];
                let receiver_op = if let Some(&receiver_local) = self.locals.get(first_seg) {
                    Some(Operand::Copy(Place::Local(receiver_local)))
                } else {
                    self.capture_index_for_name_at(callee, first_seg)
                        .map(|cap_idx| Operand::Copy(Place::Capture(cap_idx)))
                };
                if let Some(receiver_op) = receiver_op {
                    let prefix_idx = segments.len() - 2;
                    receiver_path_tir_ty = self
                        .path_segment_types
                        .get(&(self.current_metadata_scope, callee, prefix_idx))
                        .cloned();
                    let mut all_args = vec![receiver_op];
                    all_args.extend(self.lower_call_arg_operands(expr_id, args));
                    (callee_op, all_args)
                } else {
                    (callee_op, self.lower_call_arg_operands(expr_id, args))
                }
            } else {
                let callee_op = self.lower_to_operand(callee);
                (callee_op, self.lower_call_arg_operands(expr_id, args))
            }
        } else {
            let callee_op = self.lower_to_operand(callee);
            (callee_op, self.lower_call_arg_operands(expr_id, args))
        };

        let target = self.builder.create_block();
        let unwind = self.catch_context.as_ref().map(|c| c.unwind_target);

        // Check if callee is `reflect.type_of<T>()` — a value-producing intrinsic.
        // Unlike void intrinsics (log.*, baml.events.send), this emits an assignment
        // of `Rvalue::LoadType(template)` to `dest` rather than a StatementKind::Intrinsic.
        if let Some(template) = self.check_type_of_intrinsic(callee, expr_id) {
            self.builder.assign(dest, Rvalue::LoadType(template));
            self.builder.goto(target);
            self.builder.set_current_block(target);
            return;
        }

        // Check if callee is `.length()` on a container — emit Rvalue::Len instead of Call.
        if let Operand::Constant(Constant::Function(ref item)) = callee_operand {
            let name = item.to_string();
            if name == "baml.Array.length"
                || name == "baml.Map.length"
                || name == "baml.string.length"
                || name == "baml.Uint8Array.length"
            {
                if let Some(receiver_operand) = arg_operands.first() {
                    let place = match receiver_operand {
                        Operand::Copy(p) | Operand::Move(p) => p.clone(),
                        Operand::Constant(_) => {
                            let tmp = self.builder.temp(baml_type::Ty::unknown());
                            self.builder
                                .assign(Place::Local(tmp), Rvalue::Use(receiver_operand.clone()));
                            Place::Local(tmp)
                        }
                    };
                    self.builder.assign(dest, Rvalue::Len(place));
                    self.builder.goto(target);
                    self.builder.set_current_block(target);
                    return;
                }
            }
        }

        // Check if callee is a compiler intrinsic (log.*, baml.events.send).
        // Intrinsics are void side effects — emit as a statement, not a call.
        if let Some(op) = self.check_intrinsic(callee) {
            self.builder.push_statement(
                StatementKind::Intrinsic {
                    op,
                    args: arg_operands,
                },
                None,
            );
            self.builder.goto(target);
            self.builder.set_current_block(target);
            return;
        }

        // ── Emit LoadType temps for call type arguments ──────────────────────
        // When the call carries type args, either explicit (`describe<User>()`)
        // or inferred by TIR, materialise each as a `type` value on the stack
        // before the regular value args.
        // The VM pops these `ntypeargs` Object::Type values into the new frame's
        // `type_args` vec so that inner `reflect.type_of<T>()` calls can
        // substitute them at runtime.
        // Check if callee resolves to a builtin IO function (sys-op). Sys-op
        // glue reads only its declared value args plus any synthetic trailing
        // `type` operands needed for generic params that are not already
        // represented by ordinary `type` value params.
        let sys_op_type_arg_count = self.sys_op_synthetic_type_arg_count(callee);
        let is_sys_op = sys_op_type_arg_count.is_some();
        let call_type_arg_operands =
            self.lower_call_type_args(expr_id, true, sys_op_type_arg_count);

        // ── Prepend receiver's class-level type args ─────────────────────────
        // For `b.describe()` where `b: Box<int>`, the method `describe` is compiled
        // as a direct call `describe(b)` (not via MakeBoundMethod). The VM's
        // BoundMethod path for seeding frame.type_args is bypassed, so we instead
        // emit LoadType for each class-level type arg and prepend them before
        // the method's own type args. This preserves De Bruijn ordering:
        //   frame.type_args = [class_T, class_U, ..., fn_A, fn_B, ...]
        // matching `enclosing_generic_params()` = class_params ++ fn_params.
        //
        // There are two receiver paths:
        //   1. MemberAccess callee (`base.method()`): receiver type from `expr_types[recv_base_id]`.
        //   2. Path callee (`b.describe()` compiled as Path(["b","describe"])): receiver type
        //      from `path_root_types[callee_expr_id]` (TIR records root segment type there).
        let receiver_tir_ty: Option<Tir2Ty> =
            if let Some(recv_base_id) = receiver_base_for_class_type_args {
                self.expr_types
                    .get(&self.expr_metadata_key(recv_base_id))
                    .cloned()
            } else {
                receiver_path_tir_ty
            };
        let receiver_class_type_args: Vec<Tir2Ty> =
            match (&callee_operand, receiver_tir_ty.as_ref()) {
                (_, Some(Tir2Ty::Class(_, class_type_args, _))) => class_type_args.clone(),
                (
                    Operand::Constant(Constant::Function(ItemRef::Method {
                        package,
                        namespace,
                        class,
                        ..
                    })),
                    Some(Tir2Ty::List(inner, _) | Tir2Ty::EvolvingList(inner, _)),
                ) if package.as_str() == "baml"
                    && namespace.is_empty()
                    && class.as_str() == "Array" =>
                {
                    vec![inner.as_ref().clone()]
                }
                (
                    Operand::Constant(Constant::Function(ItemRef::Method {
                        package,
                        namespace,
                        class,
                        ..
                    })),
                    Some(Tir2Ty::Map { key, value, .. } | Tir2Ty::EvolvingMap(key, value, _)),
                ) if package.as_str() == "baml"
                    && namespace.is_empty()
                    && class.as_str() == "Map" =>
                {
                    vec![key.as_ref().clone(), value.as_ref().clone()]
                }
                _ => Vec::new(),
            };
        let receiver_class_type_arg_operands: Vec<Operand> = if !receiver_class_type_args.is_empty()
        {
            let generic_params = self.enclosing_generic_params();
            receiver_class_type_args
                .iter()
                .map(|ty_arg| {
                    let template = self.ty_to_template(ty_arg, &generic_params);
                    let temp = self.builder.temp(Ty::type_type());
                    self.builder
                        .assign(Place::local(temp), Rvalue::LoadType(template));
                    Operand::Copy(Place::local(temp))
                })
                .collect()
        } else {
            vec![]
        };

        let type_arg_operands: Vec<Operand> = if !receiver_class_type_arg_operands.is_empty() {
            let mut combined = receiver_class_type_arg_operands;
            combined.extend(call_type_arg_operands);
            combined
        } else {
            call_type_arg_operands
        };
        let ntypeargs = type_arg_operands.len();

        // Prepend type-arg operands before the value-arg operands.
        // (For regular BAML calls, type args are leading so the callee's frame
        // can pop them into `frame.type_args` before reading value args.)
        let all_arg_operands_for_call = if ntypeargs > 0 {
            let mut combined = type_arg_operands.clone();
            combined.extend(arg_operands.iter().cloned());
            combined
        } else {
            arg_operands.clone()
        };

        // BEP-034 `baml.future.__await_any(futures)` lowers to a dedicated
        // `Terminator::AwaitAny` suspend point (like `await`), not a call.
        if self.check_await_any(callee) {
            // The single value arg is the array of futures. (`__await_any` has
            // two type params T,E used only for type checking; the runtime
            // terminator just needs the array operand.)
            let futures_operand = arg_operands
                .into_iter()
                .next()
                .expect("__await_any takes exactly one (array) argument");
            match &dest {
                Place::Local(l) => {
                    self.builder
                        .await_any(futures_operand, Place::Local(*l), target, unwind);
                }
                _ => {
                    // Projection/capture destination: await into a temp, then
                    // assign across (mirrors the regular-call path below).
                    let call_ty = self.expr_ty(expr_id);
                    let tmp = self.builder.temp(call_ty);
                    self.builder
                        .await_any(futures_operand, Place::local(tmp), target, unwind);
                    self.builder.set_current_block(target);
                    let after = self.builder.create_block();
                    self.builder
                        .assign(dest, Rvalue::Use(Operand::Copy(Place::local(tmp))));
                    self.builder.goto(after);
                    self.builder.set_current_block(after);
                    return;
                }
            }
            self.builder.set_current_block(target);
            return;
        }

        if is_sys_op {
            // BEP-034 phase D′: sys-ops now lower to a single
            // `Terminator::SysOp` that runs the op inline in the
            // engine and binds the return value directly into `dest`
            // — no intermediate `Future` heap object, no separate
            // `Await` terminator, no `FutureManager` entry.
            //
            // The bytecode emit just becomes:
            //     <args ...>
            //     SYS_OP g
            //     <store dest>
            let dest_local = match dest {
                Place::Local(l) => l,
                _ => self.builder.temp(Ty::Null {
                    attr: TyAttr::default(),
                }),
            };
            // For generic IO builtins (`$rust_io_function` with type params),
            // the compiler may inject synthetic trailing value-arg slots for
            // runtime `baml_type::Ty` descriptors. The Rust glue reads them
            // positionally after the regular value args, so append them here
            // instead of prepending them like regular BAML frame type args.
            let sys_op_arg_operands = if ntypeargs > 0 {
                let mut combined = arg_operands;
                combined.extend(type_arg_operands);
                combined
            } else {
                arg_operands
            };
            self.builder.sys_op(
                callee_operand,
                sys_op_arg_operands,
                Place::Local(dest_local),
                target,
                unwind,
            );
        } else {
            // Call destinations must be Place::Local in MIR. If `dest` is a
            // projection (Field/Index) or a capture, call into a temp local
            // first, then assign from the temp to the real destination.
            match &dest {
                Place::Local(_) => {
                    self.builder.call_with_type_args(
                        callee_operand,
                        all_arg_operands_for_call,
                        ntypeargs,
                        dest,
                        target,
                        unwind,
                    );
                }
                _ => {
                    let call_ty = self.expr_ty(expr_id);
                    let tmp = self.builder.temp(call_ty);
                    self.builder.call_with_type_args(
                        callee_operand,
                        all_arg_operands_for_call,
                        ntypeargs,
                        Place::local(tmp),
                        target,
                        unwind,
                    );
                    self.builder.set_current_block(target);
                    let after = self.builder.create_block();
                    self.builder
                        .assign(dest, Rvalue::Use(Operand::Copy(Place::local(tmp))));
                    self.builder.goto(after);
                    self.builder.set_current_block(after);
                    return;
                }
            }
        }

        self.builder.set_current_block(target);
    }

    /// True when the callee is the BEP-034 `baml.future.__await_any`
    fn check_await_any(&self, callee: AstExprId) -> bool {
        matches!(
            self.callee_builtin_kind(callee),
            Some(baml_compiler2_ast::BuiltinKind::AwaitAny)
        )
    }
    fn callee_builtin_kind(&self, callee: AstExprId) -> Option<baml_compiler2_ast::BuiltinKind> {
        // ── Path callee (single- or multi-segment) ─────────────────────────────
        if let AstExpr::Path(segments) = &self.body.exprs[callee] {
            let func_loc = if segments.len() == 1 {
                let span_start = self
                    .source_map
                    .as_ref()
                    .map(|sm| sm.expr_span(callee).start())
                    .unwrap_or_default();
                let resolved = resolve_name_at_in_scope(
                    self.db,
                    self.file,
                    span_start,
                    &segments[0],
                    self.scope_func_name.as_ref(),
                );
                match resolved {
                    ResolvedName::Builtin(Definition::Function(fl)) => Some(fl),
                    ResolvedName::Item(Definition::Function(fl)) => Some(fl),
                    _ => None,
                }
            } else {
                // Multi-segment: check path_member_resolutions first (local-rooted paths
                // like `file.read_string`), then fall back to flat resolutions (package paths).
                // The last resolution in path_member_resolutions is the final-segment resolution.
                use baml_compiler2_tir::inference::MemberResolution;
                let from_pmr = self
                    .path_member_resolutions
                    .get(&self.expr_metadata_key(callee))
                    .and_then(|resolutions| resolutions.last())
                    .and_then(|res| match res {
                        MemberResolution::Free { func_loc } => Some(*func_loc),
                        MemberResolution::BoundMethod { func_loc, .. }
                        | MemberResolution::UnboundMethod { func_loc, .. }
                        | MemberResolution::InterfaceDefaultMethod { func_loc, .. } => {
                            Some(*func_loc)
                        }
                        MemberResolution::Field { .. } | MemberResolution::Variant { .. } => None,
                    });
                if from_pmr.is_some() {
                    from_pmr
                } else {
                    self.resolutions
                        .get(&self.expr_metadata_key(callee))
                        .and_then(|res| match res {
                            MemberResolution::Free { func_loc } => Some(*func_loc),
                            MemberResolution::BoundMethod { func_loc, .. }
                            | MemberResolution::UnboundMethod { func_loc, .. }
                            | MemberResolution::InterfaceDefaultMethod { func_loc, .. } => {
                                Some(*func_loc)
                            }
                            MemberResolution::Field { .. } | MemberResolution::Variant { .. } => {
                                None
                            }
                        })
                }
            };
            if let Some(fl) = func_loc {
                let body = baml_compiler2_ppir::function_body(self.db, fl);
                if let FunctionBody::Builtin(kind) = body.as_ref() {
                    return Some(*kind);
                }
            }
        }

        // ── NEW: MemberAccess callee (e.g. f.read, sock.recv) ──────────────────
        if let AstExpr::MemberAccess { .. } = &self.body.exprs[callee] {
            use baml_compiler2_tir::inference::MemberResolution;
            if let Some(resolution) = self.resolutions.get(&self.expr_metadata_key(callee)) {
                let func_loc = match resolution {
                    MemberResolution::BoundMethod { func_loc, .. }
                    | MemberResolution::UnboundMethod { func_loc, .. }
                    | MemberResolution::InterfaceDefaultMethod { func_loc, .. } => Some(*func_loc),
                    MemberResolution::Free { func_loc } => Some(*func_loc),
                    MemberResolution::Field { .. } | MemberResolution::Variant { .. } => None,
                };
                if let Some(fl) = func_loc {
                    let body = baml_compiler2_ppir::function_body(self.db, fl);
                    if let FunctionBody::Builtin(kind) = body.as_ref() {
                        return Some(*kind);
                    }
                }
            }
        }

        None
    }

    fn sys_op_synthetic_type_arg_count(&self, callee: AstExprId) -> Option<usize> {
        use baml_compiler2_ast::BuiltinKind;

        // ── Path callee (single- or multi-segment) ─────────────────────────────
        if let AstExpr::Path(segments) = &self.body.exprs[callee] {
            let func_loc = if segments.len() == 1 {
                let span_start = self
                    .source_map
                    .as_ref()
                    .map(|sm| sm.expr_span(callee).start())
                    .unwrap_or_default();
                let resolved = resolve_name_at_in_scope(
                    self.db,
                    self.file,
                    span_start,
                    &segments[0],
                    self.scope_func_name.as_ref(),
                );
                match resolved {
                    ResolvedName::Builtin(Definition::Function(fl)) => Some(fl),
                    ResolvedName::Item(Definition::Function(fl)) => Some(fl),
                    _ => None,
                }
            } else {
                // Multi-segment: check path_member_resolutions first (local-rooted paths
                // like `file.read_string`), then fall back to flat resolutions (package paths).
                // The last resolution in path_member_resolutions is the final-segment resolution.
                use baml_compiler2_tir::inference::MemberResolution;
                let from_pmr = self
                    .path_member_resolutions
                    .get(&self.expr_metadata_key(callee))
                    .and_then(|resolutions| resolutions.last())
                    .and_then(|res| match res {
                        MemberResolution::Free { func_loc } => Some(*func_loc),
                        MemberResolution::BoundMethod { func_loc, .. }
                        | MemberResolution::UnboundMethod { func_loc, .. }
                        | MemberResolution::InterfaceDefaultMethod { func_loc, .. } => {
                            Some(*func_loc)
                        }
                        MemberResolution::Field { .. } | MemberResolution::Variant { .. } => None,
                    });
                if from_pmr.is_some() {
                    from_pmr
                } else {
                    self.resolutions
                        .get(&self.expr_metadata_key(callee))
                        .and_then(|res| match res {
                            MemberResolution::Free { func_loc } => Some(*func_loc),
                            MemberResolution::BoundMethod { func_loc, .. }
                            | MemberResolution::UnboundMethod { func_loc, .. }
                            | MemberResolution::InterfaceDefaultMethod { func_loc, .. } => {
                                Some(*func_loc)
                            }
                            MemberResolution::Field { .. } | MemberResolution::Variant { .. } => {
                                None
                            }
                        })
                }
            };
            if let Some(fl) = func_loc {
                let body = baml_compiler2_ppir::function_body(self.db, fl);
                if let FunctionBody::Builtin(BuiltinKind::Io) = body.as_ref() {
                    return Some(self.synthetic_type_arg_count_for_sys_op(fl));
                }
            }
        }

        // ── NEW: MemberAccess callee (e.g. f.read, sock.recv) ──────────────────
        if let AstExpr::MemberAccess { .. } = &self.body.exprs[callee] {
            use baml_compiler2_tir::inference::MemberResolution;
            if let Some(resolution) = self.resolutions.get(&self.expr_metadata_key(callee)) {
                let func_loc = match resolution {
                    MemberResolution::BoundMethod { func_loc, .. }
                    | MemberResolution::UnboundMethod { func_loc, .. }
                    | MemberResolution::InterfaceDefaultMethod { func_loc, .. } => Some(*func_loc),
                    MemberResolution::Free { func_loc } => Some(*func_loc),
                    MemberResolution::Field { .. } | MemberResolution::Variant { .. } => None,
                };
                if let Some(fl) = func_loc {
                    let body = baml_compiler2_ppir::function_body(self.db, fl);
                    if let FunctionBody::Builtin(BuiltinKind::Io) = body.as_ref() {
                        return Some(self.synthetic_type_arg_count_for_sys_op(fl));
                    }
                }
            }
        }

        None
    }
    fn synthetic_type_arg_count_for_sys_op(
        &self,
        func_loc: baml_compiler2_hir::loc::FunctionLoc<'_>,
    ) -> usize {
        let item_tree = baml_compiler2_ppir::file_item_tree(self.db, func_loc.file(self.db));
        let func = &item_tree[func_loc.id(self.db)];
        let declared_type_value_params = func
            .params
            .iter()
            .filter(|param| {
                matches!(
                    param.type_expr.as_ref().map(|ty| &ty.expr),
                    Some(baml_compiler2_ast::TypeExpr::Type { .. })
                )
            })
            .count();
        func.generic_params
            .len()
            .saturating_sub(declared_type_value_params)
    }

    /// Check if the callee resolves to a `$compiler_intrinsic` function and return the
    /// corresponding `IntrinsicOp`. Follows the same resolution pattern as
    /// `sys_op_synthetic_type_arg_count`.
    fn check_intrinsic(&self, callee: AstExprId) -> Option<IntrinsicOp> {
        use baml_compiler2_ast::BuiltinKind;

        // ── Path callee (single- or multi-segment) ─────────────────────────────
        if let AstExpr::Path(segments) = &self.body.exprs[callee] {
            let func_loc = if segments.len() == 1 {
                let span_start = self
                    .source_map
                    .as_ref()
                    .map(|sm| sm.expr_span(callee).start())
                    .unwrap_or_default();
                let resolved = resolve_name_at_in_scope(
                    self.db,
                    self.file,
                    span_start,
                    &segments[0],
                    self.scope_func_name.as_ref(),
                );
                match resolved {
                    ResolvedName::Builtin(Definition::Function(fl)) => Some(fl),
                    ResolvedName::Item(Definition::Function(fl)) => Some(fl),
                    _ => None,
                }
            } else {
                use baml_compiler2_tir::inference::MemberResolution;
                let from_pmr = self
                    .path_member_resolutions
                    .get(&self.expr_metadata_key(callee))
                    .and_then(|resolutions| resolutions.last())
                    .and_then(|res| match res {
                        MemberResolution::Free { func_loc } => Some(*func_loc),
                        MemberResolution::BoundMethod { func_loc, .. }
                        | MemberResolution::UnboundMethod { func_loc, .. }
                        | MemberResolution::InterfaceDefaultMethod { func_loc, .. } => {
                            Some(*func_loc)
                        }
                        MemberResolution::Field { .. } | MemberResolution::Variant { .. } => None,
                    });
                if from_pmr.is_some() {
                    from_pmr
                } else {
                    self.resolutions
                        .get(&self.expr_metadata_key(callee))
                        .and_then(|res| match res {
                            MemberResolution::Free { func_loc } => Some(*func_loc),
                            MemberResolution::BoundMethod { func_loc, .. }
                            | MemberResolution::UnboundMethod { func_loc, .. }
                            | MemberResolution::InterfaceDefaultMethod { func_loc, .. } => {
                                Some(*func_loc)
                            }
                            MemberResolution::Field { .. } | MemberResolution::Variant { .. } => {
                                None
                            }
                        })
                }
            };
            if let Some(fl) = func_loc {
                let body = baml_compiler2_ppir::function_body(self.db, fl);
                if let FunctionBody::Builtin(BuiltinKind::Intrinsic) = body.as_ref() {
                    let item_ref = def_to_item_ref(self.db, Definition::Function(fl));
                    return match item_ref.to_string().as_str() {
                        "log.info" => Some(IntrinsicOp::Log(LogLevel::Info)),
                        "log.debug" => Some(IntrinsicOp::Log(LogLevel::Debug)),
                        "log.warn" => Some(IntrinsicOp::Log(LogLevel::Warn)),
                        "log.error" => Some(IntrinsicOp::Log(LogLevel::Error)),
                        _ => None,
                    };
                }
            }
        }

        None
    }
}

// ─── 3.6: reflect.type_of intrinsic ─────────────────────────────────────────

impl LoweringContext<'_> {
    /// Detect a `reflect.type_of<T>()` call and, if found, resolve the type
    /// argument and return the corresponding `TyTemplate`.
    ///
    /// Returns `Some(template)` when:
    /// - The callee is the `baml.reflect.type_of` `$compiler_intrinsic`.
    /// - The call carries exactly one type argument.
    /// - The type argument resolves to a concrete `Ty` (no `TypeVar` leaves).
    ///
    /// Returns `None` when the callee is not `type_of` **or** when the type
    /// argument contains a `TypeVar` (generic-parameter reference).  The latter
    /// case is deferred to template lowering, which produces
    /// `TyTemplate::TypeArgRef` leaves; attempting it here would emit a broken
    /// `LoadType` instruction.
    fn check_type_of_intrinsic(
        &self,
        callee: AstExprId,
        call_expr_id: AstExprId,
    ) -> Option<TyTemplate> {
        use baml_compiler2_ast::BuiltinKind;

        // ── 1. Check the callee resolves to `baml.reflect.type_of` ──────────
        let func_loc = if let AstExpr::Path(segments) = &self.body.exprs[callee] {
            if segments.len() == 1 {
                let span_start = self
                    .source_map
                    .as_ref()
                    .map(|sm| sm.expr_span(callee).start())
                    .unwrap_or_default();
                let resolved = baml_compiler2_tir::resolve::resolve_name_at_in_scope(
                    self.db,
                    self.file,
                    span_start,
                    &segments[0],
                    self.scope_func_name.as_ref(),
                );
                match resolved {
                    baml_compiler2_tir::resolve::ResolvedName::Builtin(
                        baml_compiler2_hir::contributions::Definition::Function(fl),
                    ) => Some(fl),
                    baml_compiler2_tir::resolve::ResolvedName::Item(
                        baml_compiler2_hir::contributions::Definition::Function(fl),
                    ) => Some(fl),
                    _ => None,
                }
            } else {
                use baml_compiler2_tir::inference::MemberResolution;
                let from_pmr = self
                    .path_member_resolutions
                    .get(&self.expr_metadata_key(callee))
                    .and_then(|resolutions| resolutions.last())
                    .and_then(|res| match res {
                        MemberResolution::Free { func_loc } => Some(*func_loc),
                        MemberResolution::BoundMethod { func_loc, .. }
                        | MemberResolution::UnboundMethod { func_loc, .. }
                        | MemberResolution::InterfaceDefaultMethod { func_loc, .. } => {
                            Some(*func_loc)
                        }
                        MemberResolution::Field { .. } | MemberResolution::Variant { .. } => None,
                    });
                if from_pmr.is_some() {
                    from_pmr
                } else {
                    self.resolutions
                        .get(&self.expr_metadata_key(callee))
                        .and_then(|res| match res {
                            MemberResolution::Free { func_loc } => Some(*func_loc),
                            MemberResolution::BoundMethod { func_loc, .. }
                            | MemberResolution::UnboundMethod { func_loc, .. }
                            | MemberResolution::InterfaceDefaultMethod { func_loc, .. } => {
                                Some(*func_loc)
                            }
                            MemberResolution::Field { .. } | MemberResolution::Variant { .. } => {
                                None
                            }
                        })
                }
            }
        } else {
            None
        }?;

        let body = baml_compiler2_ppir::function_body(self.db, func_loc);
        if !matches!(
            body.as_ref(),
            baml_compiler2_hir::body::FunctionBody::Builtin(BuiltinKind::Intrinsic)
        ) {
            return None;
        }
        let item_ref = def_to_item_ref(
            self.db,
            baml_compiler2_hir::contributions::Definition::Function(func_loc),
        );
        if item_ref.to_string().as_str() != "reflect.type_of" {
            return None;
        }

        // ── 2. Extract the single type argument ─────────────────────────────
        let type_args = if let AstExpr::Call { type_args, .. } = &self.body.exprs[call_expr_id] {
            type_args.clone()
        } else {
            return None;
        };
        let type_arg = type_args.into_iter().next()?;

        // Include the enclosing class + function generic params so that `T`
        // in `reflect.type_of<T>()` resolves to `Tir2Ty::TypeVar("T")` rather
        // than an unresolved-type error — both for free generic functions and
        // for methods on generic classes.  The order (class params first,
        // then function params) mirrors TIR's `enclosing_class_generic_params
        // ++ user_generic_params` convention used in `callable.rs`.
        let generic_params = self.enclosing_generic_params();

        // ── 4. Build TyTemplate — TypeVar → TypeArgRef(N) ─────────────────────
        let template = self.type_expr_to_template(&type_arg, &generic_params);
        Some(template)
    }

    fn type_expr_to_template(
        &self,
        type_arg: &AstTypeExpr,
        generic_params: &[baml_base::Name],
    ) -> TyTemplate {
        if let Some(template) = Self::direct_frame_type_arg_template(type_arg, generic_params) {
            return template;
        }

        let pkg_info = file_package(self.db, self.file);
        let pkg_id = PackageId::new(self.db, pkg_info.package);
        let pkg_items = package_items(self.db, pkg_id);
        let mut diags = Vec::new();
        let tir_ty = baml_compiler2_tir::lower_type_expr::lower_type_expr_in_ns(
            self.db,
            type_arg,
            pkg_items,
            &pkg_info.namespace_path,
            generic_params,
            &mut diags,
        );
        self.ty_to_template(&tir_ty, generic_params)
    }

    fn direct_frame_type_arg_template(
        type_arg: &AstTypeExpr,
        generic_params: &[baml_base::Name],
    ) -> Option<TyTemplate> {
        let AstTypeExpr::Path {
            segments,
            generic_args,
            associated_type_bindings,
            ..
        } = type_arg
        else {
            return None;
        };
        if segments.len() != 1 || !generic_args.is_empty() || !associated_type_bindings.is_empty() {
            return None;
        }
        generic_params
            .iter()
            .position(|param| param == &segments[0])
            .map(|idx| TyTemplate::TypeArgRef(u32::try_from(idx).expect("type arg index fits")))
    }

    /// Recursively convert a `Tir2Ty` to a `TyTemplate`.
    ///
    /// `Tir2Ty::TypeVar("T")` whose name appears at position `N` in
    /// `generic_params` maps to `TyTemplate::TypeArgRef(N)`.  All other types
    /// recurse structurally and bottom out at `TyTemplate::Concrete(...)`.
    fn ty_to_template(&self, ty: &Tir2Ty, generic_params: &[baml_base::Name]) -> TyTemplate {
        // Delegate to the free `tir2_to_template` so the two routines can never
        // drift apart again (C1). They were previously byte-for-byte twins; a
        // missing `Tir2Ty::Interface` arm in both voided generic interface args
        // to `Box<void>` (BEP-044 wf3 #6/#7).
        tir2_to_template(ty, self.resolved_aliases, generic_params)
    }

    /// Return the list of generic parameter names in scope for the
    /// **enclosing** function being lowered.  Empty for top-level expressions
    /// that have no enclosing generic function.
    ///
    /// When the enclosing function is a method on a generic class, the
    /// class-level params come first, followed by the function-level params
    /// — matching TIR's `enclosing_class_generic_params ++ generic_params`
    /// convention (see `baml_compiler2_tir::callable`).  This keeps MIR's
    /// view of in-scope generics consistent with how TIR types the body.
    ///
    /// Runtime lowering is responsible for seeding this frame layout: direct
    /// method calls prepend receiver class args, and interface dispatch seeds
    /// either static guard args or the matched receiver instance's class args.
    fn enclosing_generic_params(&self) -> Vec<baml_base::Name> {
        let Some(fl) = self.func_loc else {
            return Vec::new();
        };
        let item_tree = file_item_tree(self.db, fl.file(self.db));
        let func_id = fl.id(self.db);
        if let Some(imp) = item_tree
            .implements_for
            .iter()
            .find(|imp| imp.methods.contains(&func_id))
        {
            let mut params = imp.generic_params.clone();
            params.extend(item_tree[func_id].generic_params.iter().cloned());
            return params;
        }
        // BEP-044: interface default methods are lowered as standalone
        // functions, but their bodies reference the *interface's* generic
        // params (e.g. a default `map(self)` building `Map<T, U>`). Mirror the
        // class-method convention — interface params first, then fn params — so
        // `TypeVar(T)` lowers to `TypeArgRef(N)` against the frame type args the
        // interface-dispatch switch seeds (see `emit_method_candidate_switch`).
        if let Some(iface_data) = item_tree
            .interfaces
            .values()
            .find(|iface_data| iface_data.default_methods.contains(&func_id))
        {
            let mut params = iface_data.generic_params.clone();
            params.extend(
                iface_data
                    .associated_types
                    .iter()
                    .map(|assoc| assoc.name.clone()),
            );
            params.extend(item_tree[func_id].generic_params.iter().cloned());
            return params;
        }
        let mut params: Vec<baml_base::Name> = item_tree
            .classes
            .values()
            .find(|class_data| class_data.methods.contains(&func_id))
            .map(|class_data| class_data.generic_params.clone())
            .unwrap_or_default();
        params.extend(item_tree[func_id].generic_params.iter().cloned());
        // Inside a (possibly nested) generic lambda body, the lambda's own
        // type params follow the enclosing function's, matching the runtime
        // frame.type_args layout. Empty outside any lambda.
        params.extend(self.lambda_generic_params.iter().cloned());
        params
    }

    /// Emit `LoadType` temps for a list of type args resolved at an interface
    /// dispatch site, returning one `Operand` per arg (in order). Used by
    /// `emit_method_candidate_switch` to seed the callee frame's `type_args`.
    /// `TypeVar`s are lowered against the *caller's* `enclosing_generic_params`
    /// so they substitute against the caller's `frame.type_args` at runtime
    /// (mirroring the receiver-class-type-args path for direct method calls).
    fn emit_frame_type_arg_ops(&mut self, tys: &[Tir2Ty]) -> Vec<Operand> {
        if tys.is_empty() {
            return Vec::new();
        }
        let generic_params = self.enclosing_generic_params();
        tys.iter()
            .map(|ty| {
                let template = self.ty_to_template(ty, &generic_params);
                let temp = self.builder.temp(Ty::type_type());
                self.builder
                    .assign(Place::local(temp), Rvalue::LoadType(template));
                Operand::Copy(Place::local(temp))
            })
            .collect()
    }

    fn lower_call_type_args(
        &mut self,
        call_expr_id: AstExprId,
        include_inferred: bool,
        max_count: Option<usize>,
    ) -> Vec<Operand> {
        if max_count == Some(0) {
            return Vec::new();
        }
        let ast_type_args: Vec<AstTypeExpr> =
            if let AstExpr::Call { type_args, .. } = &self.body.exprs[call_expr_id] {
                type_args.clone()
            } else {
                Vec::new()
            };
        if !ast_type_args.is_empty() {
            let ast_type_args = match max_count {
                Some(max_count) => ast_type_args.into_iter().take(max_count).collect(),
                None => ast_type_args,
            };
            return self.lower_explicit_type_args(&ast_type_args);
        }
        if !include_inferred {
            return Vec::new();
        }

        let mut inferred_type_args = self
            .call_plans
            .get(&self.expr_metadata_key(call_expr_id))
            .map(|plan| plan.type_args.clone())
            .unwrap_or_default();
        let caller_generic_params = self.enclosing_generic_params();
        for ty in &mut inferred_type_args {
            if baml_compiler2_tir::generics::contains_typevar_where(ty, &|name| {
                !caller_generic_params.iter().any(|param| param == name)
            }) {
                *ty = Tir2Ty::BuiltinUnknown {
                    attr: TyAttr::default(),
                };
            }
        }
        if let Some(max_count) = max_count {
            inferred_type_args.truncate(max_count);
        }
        if inferred_type_args
            .iter()
            .all(|ty| matches!(ty, Tir2Ty::BuiltinUnknown { .. } | Tir2Ty::Unknown { .. }))
        {
            return Vec::new();
        }
        self.emit_frame_type_arg_ops(&inferred_type_args)
    }

    fn call_has_explicit_type_args(&self, call_expr_id: AstExprId) -> bool {
        matches!(
            &self.body.exprs[call_expr_id],
            AstExpr::Call { type_args, .. } if !type_args.is_empty()
        )
    }

    /// Emit `LoadType` rvalue assignments for the explicit type arguments of a
    /// generic call and return the resulting operands plus the count.
    ///
    /// For each `TypeExpr` in `ast_type_args`:
    /// 1. Lowers it to `Tir2Ty` (respecting the enclosing generic params so
    ///    that `T` resolves to `Tir2Ty::TypeVar("T")` rather than an error).
    /// 2. Converts it to a `TyTemplate` via `ty_to_template` (`TypeVar` → `TypeArgRef(N)`).
    /// 3. Assigns `Rvalue::LoadType(template)` to a fresh `type`-typed temp.
    /// 4. Appends that temp as an `Operand::Copy` to the returned vec.
    ///
    /// Returns `(type_arg_operands, ntypeargs)` — the number equals
    /// `ast_type_args.len()`.  Returns an empty vec when there are no type args.
    fn lower_explicit_type_args(&mut self, ast_type_args: &[AstTypeExpr]) -> Vec<Operand> {
        if ast_type_args.is_empty() {
            return vec![];
        }

        let generic_params = self.enclosing_generic_params();
        let type_ty = baml_type::Ty::type_type();

        let mut operands = Vec::with_capacity(ast_type_args.len());
        for type_arg in ast_type_args {
            let template = self.type_expr_to_template(type_arg, &generic_params);
            let temp = self.builder.temp(type_ty.clone());
            self.builder
                .assign(Place::local(temp), Rvalue::LoadType(template));
            operands.push(Operand::Copy(Place::local(temp)));
        }
        operands
    }

    /// Lower `foo<int>` (a `GenericApply` value). If the base resolves to a
    /// function `ItemRef` and all type args are fully concrete, emit a pooled,
    /// interned `Constant::GenericFunction` (pointer-stable; seeds
    /// `frame.type_args` when called). Otherwise fall back to lowering the base
    /// value with type args erased — for exotic bases (bound methods, lambdas)
    /// or param-dependent args (`foo<T>` inside a generic function).
    fn lower_generic_apply(&mut self, base: AstExprId, type_args: &[AstTypeExpr], dest: Place) {
        let Some(item) = self.try_resolve_generic_apply_base(base) else {
            // Non-`ItemRef` base (a local/captured generic function value):
            // there is no function global to pool, so specialize the *runtime
            // value* — evaluate it and wrap it in a closure carrying the
            // (frame-resolved) type args — instead of silently erasing them.
            let value = self.lower_to_operand(base);
            let type_arg_templates = self.generic_apply_type_arg_templates(type_args);
            self.builder.assign(
                dest,
                Rvalue::MakeGenericFunctionFromValue {
                    value,
                    type_arg_templates,
                },
            );
            return;
        };
        let templates = self.generic_apply_type_arg_templates(type_args);
        if templates.iter().all(TyTemplate::is_fully_concrete) {
            // Concrete args → pooled, interned compile-time constant
            // (pointer-stable identity).
            let concrete: Vec<Ty> = templates.iter().map(|t| t.substitute(&[])).collect();
            self.builder.assign(
                dest,
                Rvalue::Use(Operand::Constant(Constant::GenericFunction {
                    item,
                    type_args: concrete,
                })),
            );
        } else {
            // A type arg depends on an enclosing generic param (`foo<T>` inside
            // a generic fn) → build the value at runtime, resolving the
            // templates against the current frame's type_args.
            self.builder.assign(
                dest,
                Rvalue::MakeGenericFunction {
                    item,
                    type_arg_templates: templates,
                },
            );
        }
    }

    /// Resolve a `GenericApply` base to the underlying function `ItemRef` (free
    /// function or static/interface method). `None` for bound methods, lambdas,
    /// or anything that is not a function path.
    fn try_resolve_generic_apply_base(&self, base: AstExprId) -> Option<ItemRef> {
        use baml_compiler2_tir::inference::MemberResolution;
        let is_fn = |r: &MemberResolution<'_>| {
            matches!(
                r,
                MemberResolution::Free { .. }
                    | MemberResolution::UnboundMethod { .. }
                    | MemberResolution::InterfaceDefaultMethod { .. }
            )
        };
        let key = self.expr_metadata_key(base);
        // Multi-segment paths: static methods, qualified free fns (e.g. baml.json.from_string).
        if let Some(item) = self
            .path_member_resolutions
            .get(&key)
            .and_then(|rs| rs.last())
            .filter(|r| is_fn(r))
            .and_then(|r| resolution_to_item_ref(self.db, r))
        {
            return Some(item);
        }
        // Flat / package resolutions.
        if let Some(item) = self
            .resolutions
            .get(&key)
            .filter(|r| is_fn(r))
            .and_then(|r| resolution_to_item_ref(self.db, r))
        {
            return Some(item);
        }
        // Single-name free function / builtin.
        if let AstExpr::Path(segments) = &self.body.exprs[base]
            && segments.len() == 1
        {
            let span_start = self
                .source_map
                .as_ref()
                .map(|sm| sm.expr_span(base).start())
                .unwrap_or_default();
            match resolve_name_at_in_scope(
                self.db,
                self.file,
                span_start,
                &segments[0],
                self.scope_func_name.as_ref(),
            ) {
                ResolvedName::Item(def @ Definition::Function(_))
                | ResolvedName::Builtin(def @ Definition::Function(_)) => {
                    return Some(def_to_item_ref(self.db, def));
                }
                _ => {}
            }
        }
        None
    }

    /// Resolve `GenericApply` AST type args to `TyTemplate`s. A template is
    /// `is_fully_concrete()` unless the arg references an enclosing generic
    /// param (then it carries a `TypeArgRef`, resolved at runtime).
    fn generic_apply_type_arg_templates(&self, type_args: &[AstTypeExpr]) -> Vec<TyTemplate> {
        let generic_params = self.enclosing_generic_params();
        type_args
            .iter()
            .map(|type_arg| self.type_expr_to_template(type_arg, &generic_params))
            .collect()
    }
}

// ─── 3.7: Helper methods ─────────────────────────────────────────────────────

impl<'db> LoweringContext<'db> {
    fn lower_to_operand(&mut self, expr_id: AstExprId) -> Operand {
        let ty = self.expr_ty(expr_id);
        let temp = self.builder.temp(ty);
        self.lower_expr(expr_id, Place::local(temp));
        Operand::Copy(Place::Local(temp))
    }

    fn emit_panic_call(&mut self, message: &str, _expr_id: AstExprId) {
        // Emit a call to baml.sys.panic with the error message
        let callee = Operand::Constant(Constant::Function(ItemRef::Free {
            package: Name::new("baml"),
            namespace: vec![Name::new("sys")],
            name: Name::new("panic"),
        }));
        let msg = Operand::Constant(Constant::String(message.to_string()));
        let temp = self.builder.temp(Ty::Null {
            attr: TyAttr::default(),
        });
        let unreachable_block = self.builder.create_block();
        self.builder.call(
            callee,
            vec![msg],
            Place::local(temp),
            unreachable_block,
            None,
        );
        self.builder.set_current_block(unreachable_block);
        self.builder.unreachable();
        // Start a new block for any code after this (dead code)
        let dead = self.builder.create_block();
        self.builder.set_current_block(dead);
    }

    fn lower_if(
        &mut self,
        _expr_id: AstExprId,
        condition: AstExprId,
        then_branch: AstExprId,
        else_branch: Option<AstExprId>,
        dest: Place,
    ) {
        let cond_op = self.lower_to_operand(condition);
        let bb_then = self.builder.create_block();
        let bb_else = self.builder.create_block();
        let bb_join = self.builder.create_block();

        self.builder.branch(cond_op, bb_then, bb_else);

        self.builder.set_current_block(bb_then);
        self.lower_expr(then_branch, dest.clone());
        if !self.builder.is_current_terminated() {
            self.builder.goto(bb_join);
        }

        self.builder.set_current_block(bb_else);
        if let Some(else_expr) = else_branch {
            self.lower_expr(else_expr, dest);
        } else {
            self.builder
                .assign(dest, Rvalue::Use(Operand::Constant(Constant::Null)));
        }
        if !self.builder.is_current_terminated() {
            self.builder.goto(bb_join);
        }

        self.builder.set_current_block(bb_join);
    }

    /// MIR lowering for `if let PATTERN = SCRUTINEE { THEN } else { ELSE }`.
    ///
    /// Same shape as a two-arm match (`PATTERN => then, _ => else`), but we
    /// emit it inline rather than synthesizing a match. The pattern test
    /// jumps to the then-block on success (where we bind names from the
    /// pattern before lowering the body) and to the else-block on failure.
    fn lower_if_let(
        &mut self,
        _expr_id: AstExprId,
        pattern: AstPatId,
        scrutinee: AstExprId,
        then_branch: AstExprId,
        else_branch: Option<AstExprId>,
        dest: Place,
    ) {
        let scrutinee_local = self.try_resolve_to_local(scrutinee).unwrap_or_else(|| {
            let op = self.lower_to_operand(scrutinee);
            let ty = self.expr_ty(scrutinee);
            self.operand_to_local(op, ty)
        });

        let bb_then = self.builder.create_block();
        let bb_else = self.builder.create_block();
        let bb_join = self.builder.create_block();

        self.lower_pattern_test(scrutinee_local, pattern, bb_then, bb_else);

        // Then-branch: bind pattern locals, lower body, restore on exit.
        self.builder.set_current_block(bb_then);
        let saved_locals = self.locals.clone();
        let watched_depth = self.watched_locals_stack.len();
        self.bind_pattern(scrutinee_local, pattern);
        self.lower_expr(then_branch, dest.clone());
        if !self.builder.is_current_terminated() {
            self.emit_unwatch_to_depth(watched_depth);
            self.builder.goto(bb_join);
        }
        self.restore_locals_after_scope(saved_locals, watched_depth);

        // Else-branch: no bindings from the pattern, just lower the else
        // (or write Null if absent — same as plain `if` with no else).
        self.builder.set_current_block(bb_else);
        if let Some(else_expr) = else_branch {
            self.lower_expr(else_expr, dest);
        } else {
            self.builder
                .assign(dest, Rvalue::Use(Operand::Constant(Constant::Null)));
        }
        if !self.builder.is_current_terminated() {
            self.builder.goto(bb_join);
        }

        self.builder.set_current_block(bb_join);
    }

    fn lower_object(
        &mut self,
        expr_id: AstExprId,
        type_name: Option<&TypePath>,
        type_args: &[AstTypeExpr],
        fields: &[(Name, AstExprId)],
        spreads: &[baml_compiler2_ast::SpreadField],
        dest: Place,
    ) {
        // Prefer the explicitly written type name. If absent (e.g., when the
        // type is a qualified path like `baml.errors.DevOther`), fall back to
        // the TIR-inferred type to get the short class name.
        //
        // We also extract a `TypeName` for looking up fields in `class_fields`,
        // which is keyed by `TypeName`.
        let ty = self.expr_ty(expr_id);
        let type_name_key: Option<TypeName> = match &ty {
            Ty::Class(tn, _, _) => Some(tn.clone()),
            _ => None,
        };
        // Prefer the TIR-resolved fully-qualified name (`<package>.<ns>.<name>`)
        // because that matches the bytecode emitter's FQN registry. The parser
        // stores qualified paths verbatim from source (e.g. `root.http.Response`
        // for user types), but the emitter registers user types under the `user.`
        // prefix — so the source-verbatim form would miss the lookup. Falling
        // back to the parser name only when TIR has no type info handles
        // synthetic Object exprs from `lower_cst.rs` that already use registry-
        // matching dotted forms like "baml.llm.Client".
        let class_name = if let Some(tn) = &type_name_key {
            tn.render_dotted(false)
        } else {
            type_name.map(ToString::to_string).unwrap_or_default()
        };
        let field_slot_count = |field_name_to_idx: &IndexMap<String, usize>| {
            field_name_to_idx
                .values()
                .copied()
                .max()
                .map(|idx| idx + 1)
                .unwrap_or(0)
        };

        if spreads.is_empty() {
            // Lower fields in class-definition order, filling unspecified slots
            // with Null. Source order in the literal does not match definition
            // order, so a partial literal like `ScanOptions { absolute: true }`
            // would otherwise put `absolute` into whichever slot happens to be
            // first. The TIR Object handler resolves the type via its qualified
            // path, so `class_fields.get(tn)` always finds the definition for
            // any user-written class literal.
            let field_operands: Vec<Operand> = if let Some(field_name_to_idx) = type_name_key
                .as_ref()
                .and_then(|tn| self.class_fields.get(tn))
                .cloned()
            {
                let mut result: Vec<Operand> = (0..field_slot_count(&field_name_to_idx))
                    .map(|_| Operand::Constant(Constant::Null))
                    .collect();
                for (name, expr) in fields {
                    if let Some(&idx) = field_name_to_idx.get(&name.to_string()) {
                        result[idx] = self.lower_to_operand(*expr);
                    }
                }
                result
            } else {
                // Synthetic Object exprs without TIR type info (e.g. compiler
                // sugar for retry policies) fall back to source order. These
                // construction sites build full, ordered literals so the order
                // matches the class definition.
                fields
                    .iter()
                    .map(|(_, e)| self.lower_to_operand(*e))
                    .collect()
            };
            let type_arg_templates = self.object_class_type_arg_templates(expr_id, type_args);
            self.builder.assign(
                dest,
                Rvalue::Aggregate {
                    kind: AggregateKind::Class {
                        name: class_name,
                        type_arg_templates,
                    },
                    fields: field_operands,
                },
            );
        } else {
            // Lower spread base(s) and explicit fields eagerly (in source
            // order), then assemble the aggregate respecting override semantics:
            // later source entries override earlier ones for the same class field.

            enum Entry {
                Spread(Local),
                Named(String, Operand),
            }

            let field_count = type_name_key
                .as_ref()
                .and_then(|tn| self.class_fields.get(tn))
                .map(field_slot_count)
                .unwrap_or(0);

            // Lower all spread expressions into locals.
            let spread_locals: Vec<(usize, Local)> = spreads
                .iter()
                .map(|s| {
                    let op = self.lower_to_operand(s.expr);
                    let ty = self.expr_ty(s.expr);
                    (s.position, self.operand_to_local(op, ty))
                })
                .collect();

            // Lower all explicit field expressions into operands.
            // Named fields occupy source positions 0.. excluding spread positions.
            // Assign each named field its source position by counting up and
            // skipping positions occupied by spreads.
            let spread_positions: HashSet<usize> = spreads.iter().map(|s| s.position).collect();
            let explicit_with_pos: Vec<(usize, String, Operand)> = {
                let mut pos = 0usize;
                fields
                    .iter()
                    .map(|(name, e)| {
                        while spread_positions.contains(&pos) {
                            pos += 1;
                        }
                        let cur = pos;
                        pos += 1;
                        (cur, name.to_string(), self.lower_to_operand(*e))
                    })
                    .collect()
            };

            // Build per-class-field operand array. Process all entries in source
            // position order; later entries overwrite earlier ones.
            let field_name_to_idx: &IndexMap<String, usize> = match type_name_key
                .as_ref()
                .and_then(|tn| self.class_fields.get(tn))
            {
                Some(m) => m,
                None => {
                    // Unknown class — just emit named fields in order.
                    let field_operands: Vec<Operand> = fields
                        .iter()
                        .map(|(_, e)| self.lower_to_operand(*e))
                        .collect();
                    let type_arg_templates =
                        self.object_class_type_arg_templates(expr_id, type_args);
                    self.builder.assign(
                        dest,
                        Rvalue::Aggregate {
                            kind: AggregateKind::Class {
                                name: class_name,
                                type_arg_templates,
                            },
                            fields: field_operands,
                        },
                    );
                    return;
                }
            };

            // Merge all entries into a single sorted list by source position.
            let mut entries: Vec<(usize, Entry)> = Vec::new();
            for (pos, local) in &spread_locals {
                entries.push((*pos, Entry::Spread(*local)));
            }
            for (pos, name, op) in explicit_with_pos {
                entries.push((pos, Entry::Named(name, op)));
            }
            entries.sort_by_key(|(pos, _)| *pos);

            // Initialize all fields to null, then apply entries in order.
            let mut result: Vec<Operand> = (0..field_count)
                .map(|_| Operand::Constant(Constant::Null))
                .collect();

            for (_, entry) in &entries {
                match entry {
                    Entry::Spread(local) => {
                        // A spread fills every field from the base object.
                        for (idx, slot) in result.iter_mut().enumerate().take(field_count) {
                            *slot = Operand::Copy(Place::Field {
                                base: Box::new(Place::Local(*local)),
                                field: idx,
                            });
                        }
                    }
                    Entry::Named(name, op) => {
                        if let Some(&idx) = field_name_to_idx.get(name) {
                            result[idx] = op.clone();
                        }
                    }
                }
            }

            let type_arg_templates = self.object_class_type_arg_templates(expr_id, type_args);
            self.builder.assign(
                dest,
                Rvalue::Aggregate {
                    kind: AggregateKind::Class {
                        name: class_name,
                        type_arg_templates,
                    },
                    fields: result,
                },
            );
        }
    }

    fn lower_member_access(
        &mut self,
        expr_id: AstExprId,
        base: AstExprId,
        field: &Name,
        dest: Place,
    ) {
        // Check if TIR resolved this to a method or free function — if so, emit a function constant
        // (unbound) or MakeBoundMethod (bound). Field and Variant resolutions fall through to the
        // existing lowering paths below.
        if let Some(resolution) = self
            .resolutions
            .get(&self.expr_metadata_key(expr_id))
            .cloned()
        {
            use baml_compiler2_tir::inference::MemberResolution;
            match &resolution {
                MemberResolution::BoundMethod { .. } => {
                    // Bound method reference: lower receiver and emit MakeBoundMethod.
                    let item = resolution_to_item_ref(self.db, &resolution);
                    if let Some(item) = item {
                        let receiver_op = self.lower_to_operand(base);
                        self.builder.assign(
                            dest,
                            Rvalue::MakeBoundMethod {
                                item_ref: item,
                                receiver: receiver_op,
                            },
                        );
                        return;
                    }
                }
                MemberResolution::UnboundMethod { .. }
                | MemberResolution::Free { .. }
                | MemberResolution::InterfaceDefaultMethod { .. } => {
                    // Unbound method or free function reference: emit a plain function constant.
                    let item = resolution_to_item_ref(self.db, &resolution);
                    if let Some(item) = item {
                        self.builder.assign(
                            dest,
                            Rvalue::Use(Operand::Constant(Constant::Function(item))),
                        );
                        return;
                    }
                }
                MemberResolution::Field { .. } | MemberResolution::Variant { .. } => {
                    // Fall through — handled by the existing field/enum-variant lowering below
                }
            }
        }

        // An interface method referenced as a *value* on a generic- or
        // interface-typed receiver (`let f = x.eq`): there is no single concrete
        // method to bind statically, so bind the implementor's method by the
        // receiver's runtime type (captured now — its type is fixed at this
        // point). Resolve candidates *before* lowering the receiver so a field
        // access (no method candidates) falls through to the field path below
        // without evaluating the receiver expression twice.
        if let Some((iface_tn, iface_type_args, iface_assoc)) =
            self.interface_dispatch_target_for_expr(base).or_else(|| {
                self.expr_types
                    .get(&self.expr_metadata_key(base))
                    .and_then(|ty| self.registry_dispatch_target_for_concrete(ty, field))
            })
        {
            let recv_tir_ty = self.dispatch_receiver_static_tir_ty(base);
            let resolved = self.interface_method_candidates_for(
                &iface_tn,
                &iface_type_args,
                &iface_assoc,
                field,
                recv_tir_ty.as_ref(),
            );
            if !resolved.is_empty() {
                let recv_op = self.lower_to_operand(base);
                let recv_local = self.builder.temp(self.expr_ty(base));
                self.builder
                    .assign(Place::local(recv_local), Rvalue::Use(recv_op));
                self.emit_bound_method_candidate_switch(recv_local, &resolved, &dest);
                return;
            }
        }

        // Check if TIR resolved this to an enum variant (e.g. baml.HttpMethod.Get via package path)
        if let Some(Tir2Ty::EnumVariant(qtn, variant, _)) = self
            .expr_types
            .get(&self.expr_metadata_key(expr_id))
            .cloned()
            .as_ref()
        {
            let enum_ref = ItemRef::EnumType {
                package: qtn.package().clone(),
                namespace: qtn.namespace().clone(),
                name: qtn.name().clone(),
            };
            self.builder.assign(
                dest,
                Rvalue::Use(Operand::Constant(Constant::EnumVariant {
                    enum_ref,
                    variant: variant.clone(),
                })),
            );
            return;
        }

        // Check if this is a package path intermediate (e.g. `baml.HttpMethod` in
        // `baml.HttpMethod.Get`). TIR marks these as Ty::Unknown. Emit null placeholder.
        // CRITICAL: only treat the expression as a namespace intermediate if the BASE
        // is also Unknown (i.e. `baml` in `baml.HttpMethod`). If the base has a
        // concrete type, this is a real field access whose field type happens to be
        // Unknown (unresolved type annotation). In that case, fall through to emit
        // the field projection.
        if let Some(Tir2Ty::Unknown { .. }) = self.expr_types.get(&self.expr_metadata_key(expr_id))
        {
            let base_is_also_unknown = self
                .expr_types
                .get(&self.expr_metadata_key(base))
                .map(|ty| matches!(ty, Tir2Ty::Unknown { .. }))
                .unwrap_or(true);
            if base_is_also_unknown {
                self.builder
                    .assign(dest, Rvalue::Use(Operand::Constant(Constant::Null)));
                return;
            }
            // Base is a real value (non-Unknown type) — fall through to field projection
        }

        // Regular field access
        let base_ty = self.expr_ty(base);
        let base_op = self.lower_to_operand(base);
        let field_str = field.to_string();

        // Unwrap Optional — when called from lower_optional_member_access,
        // the base type is T? but we've already null-checked, so use the inner type.
        let unwrapped_ty = base_ty.strip_null();

        // Look up field index from class_fields
        let field_idx = if let Ty::Class(tn, _, _) = &unwrapped_ty {
            self.class_fields
                .get(tn)
                .and_then(|fields| fields.get(&field_str))
                .copied()
        } else {
            None
        };

        let base_local = self.operand_to_local(base_op, base_ty);

        if let Some(idx) = field_idx {
            self.builder.assign(
                dest,
                Rvalue::Use(Operand::Copy(Place::Field {
                    base: Box::new(Place::Local(base_local)),
                    field: idx,
                })),
            );
        } else {
            let handled_interface_field = self
                .interface_receiver_for_field_access(base, &unwrapped_ty)
                .is_some_and(|(iface_tn, iface_type_args, iface_assoc)| {
                    self.try_lower_interface_field_access(
                        base_local,
                        &iface_tn,
                        &iface_type_args,
                        &iface_assoc,
                        field,
                        &dest,
                    )
                });
            let handled_union_field = handled_interface_field
                || self.lower_union_class_field_access(
                    expr_id,
                    base_local,
                    &unwrapped_ty,
                    field,
                    &dest,
                )
                || self
                    .expr_types
                    .get(&self.expr_metadata_key(base))
                    .and_then(Self::tir_union_members)
                    .is_some_and(|members| {
                        self.lower_union_iface_field_access(base_local, &members, field, &dest)
                    });
            if handled_union_field {
                return;
            }
            if let Ty::Class(tn, _, _) = &unwrapped_ty {
                self.emit_panic_call(
                    &format!(
                        "internal compiler error: MIR failed to resolve field access \
                         .{} against class definition '{}' (module_path: {:?}). \
                         This class should be in class_fields but isn't.",
                        field_str,
                        tn.name(),
                        tn.module_path(),
                    ),
                    expr_id,
                );
                return;
            }
            // Dynamic map access — only valid for map types, unknown, etc.
            let key_local = self.builder.temp(Ty::String {
                attr: TyAttr::default(),
            });
            self.builder.assign(
                Place::local(key_local),
                Rvalue::Use(Operand::Constant(Constant::String(field_str))),
            );
            self.builder.assign(
                dest,
                Rvalue::Use(Operand::Copy(Place::Index {
                    base: Box::new(Place::Local(base_local)),
                    index: key_local,
                    kind: IndexKind::Map,
                })),
            );
        }
    }

    fn interface_receiver_for_field_access(
        &self,
        base: AstExprId,
        unwrapped_ty: &Ty,
    ) -> Option<InterfaceTypeView> {
        if let Some(target) = self.interface_dispatch_target_for_expr(base) {
            return Some(target);
        }

        match unwrapped_ty {
            Ty::Class(tn, _, _) if self.interface_implementors.contains_key(tn) => {
                Some((tn.clone(), Vec::new(), Vec::new()))
            }
            Ty::Interface(tn, _, _, _) if self.interface_implementors.contains_key(tn) => {
                Some((tn.clone(), Vec::new(), Vec::new()))
            }
            _ => None,
        }
    }

    fn interface_receiver_for_path_prefix(
        &self,
        expr_id: AstExprId,
        prefix_idx: usize,
        current_ty: &Ty,
    ) -> Option<InterfaceTypeView> {
        if let Some(target) = self
            .path_segment_types
            .get(&(self.current_metadata_scope, expr_id, prefix_idx))
            .and_then(|ty| self.interface_dispatch_target_for_tir_ty(ty))
        {
            return Some(target);
        }
        if prefix_idx == 0
            && let Some(target) = self
                .path_root_types
                .get(&self.expr_metadata_key(expr_id))
                .and_then(|ty| self.interface_dispatch_target_for_tir_ty(ty))
        {
            return Some(target);
        }

        match current_ty {
            Ty::Class(tn, _, _) if self.interface_implementors.contains_key(tn) => {
                Some((tn.clone(), Vec::new(), Vec::new()))
            }
            Ty::Interface(tn, _, _, _) if self.interface_implementors.contains_key(tn) => {
                Some((tn.clone(), Vec::new(), Vec::new()))
            }
            _ => None,
        }
    }

    fn class_receiver_for_path_prefix(
        &self,
        expr_id: AstExprId,
        prefix_idx: usize,
        current_ty: &Ty,
    ) -> Option<(TypeName, Vec<Ty>)> {
        let tir_prefix_ty = if prefix_idx == 0 {
            self.path_root_types.get(&self.expr_metadata_key(expr_id))
        } else {
            self.path_segment_types
                .get(&(self.current_metadata_scope, expr_id, prefix_idx))
        };
        if let Some(target) = tir_prefix_ty.and_then(|ty| self.class_dispatch_target_for_tir_ty(ty))
        {
            return Some(target);
        }

        match current_ty {
            Ty::Class(tn, type_args, _) => Some((tn.clone(), type_args.clone())),
            _ => None,
        }
    }

    fn lower_union_class_field_access(
        &mut self,
        _expr_id: AstExprId,
        base_local: Local,
        base_ty: &Ty,
        field: &Name,
        dest: &Place,
    ) -> bool {
        let Some(candidates) = self.class_union_field_candidates(base_ty, field) else {
            return false;
        };

        let bb_entry = self.builder.current_block();
        let bb_join = self.builder.create_block();
        let bb_otherwise = self.builder.create_block();

        let tag_local = self.builder.temp(Ty::Int {
            attr: TyAttr::default(),
        });
        self.builder.assign(
            Place::local(tag_local),
            Rvalue::TypeTag(Place::local(base_local)),
        );

        let mut arms = Vec::with_capacity(candidates.len());
        let mut arm_names = Vec::with_capacity(candidates.len());
        for (tag, class_name, field_idx) in candidates {
            let bb_body = self.builder.create_block();
            arms.push((tag, bb_body));
            arm_names.push((tag, class_name.name().to_string()));

            self.builder.set_current_block(bb_body);
            self.builder.assign(
                dest.clone(),
                Rvalue::Use(Operand::Copy(Place::Field {
                    base: Box::new(Place::Local(base_local)),
                    field: field_idx,
                })),
            );
            self.builder.goto(bb_join);
        }

        self.builder.set_current_block(bb_otherwise);
        self.builder.unreachable();

        self.builder.set_current_block(bb_entry);
        self.builder.switch(
            Operand::Copy(Place::Local(tag_local)),
            arms,
            bb_otherwise,
            true,
            arm_names,
        );
        self.builder.set_current_block(bb_join);
        true
    }

    fn class_union_field_candidates(
        &self,
        ty: &Ty,
        field: &Name,
    ) -> Option<Vec<(i64, TypeName, usize)>> {
        // Collect candidate (class_name) entries to search for the field on.
        // For `Ty::Union`, every member must be `Ty::Class`. For `Ty::Class`
        // whose name is actually a BEP-044 interface, use the registered
        // implementor set.
        let class_names: Vec<TypeName> = match ty {
            Ty::Union(members, _) => members
                .iter()
                .filter_map(|m| match m {
                    Ty::Class(n, _, _) => Some(n.clone()),
                    _ => None,
                })
                .collect(),
            Ty::Class(class_name, _, _) => self.interface_implementors.get(class_name)?.clone(),
            _ => return None,
        };
        if class_names.is_empty() {
            return None;
        }

        let mut candidates = Vec::new();
        for class_name in &class_names {
            let field_idx = self
                .class_fields
                .get(class_name)
                .and_then(|fields| fields.get(field.as_str()))
                .copied()?;
            let tag = self.class_type_tags.get(class_name).copied()?;
            if !candidates
                .iter()
                .any(|(existing_tag, _, _)| *existing_tag == tag)
            {
                candidates.push((tag, class_name.clone(), field_idx));
            }
        }

        (!candidates.is_empty()).then_some(candidates)
    }

    fn lower_index(&mut self, _expr_id: AstExprId, base: AstExprId, index: AstExprId, dest: Place) {
        let base_ty = self.expr_ty(base);
        let base_op = self.lower_to_operand(base);
        let index_op = self.lower_to_operand(index);
        let index_ty = self.expr_ty(index);

        let base_local = self.operand_to_local(base_op, base_ty.clone());
        let index_local = self.operand_to_local(index_op, index_ty);

        // Unwrap Optional — when called from lower_optional_index,
        // the base type is T? but we've already null-checked.
        let unwrapped_ty = base_ty.strip_null();

        let kind = if matches!(&unwrapped_ty, Ty::List(..) | Ty::Uint8Array { .. }) {
            IndexKind::Array
        } else {
            IndexKind::Map
        };

        self.builder.assign(
            dest,
            Rvalue::Use(Operand::Copy(Place::Index {
                base: Box::new(Place::Local(base_local)),
                index: index_local,
                kind,
            })),
        );
    }

    /// If the expression is a simple local variable reference (single-segment path
    /// resolving to a known local), return its Local directly without allocating
    /// a temp or emitting a copy.
    fn try_resolve_to_local(&self, expr_id: AstExprId) -> Option<Local> {
        let expr = &self.body.exprs[expr_id];
        if let AstExpr::Path(segments) = expr {
            if segments.len() == 1 {
                if let Some(&local) = self.locals.get(&segments[0]) {
                    return Some(local);
                }
            }
        }
        None
    }

    /// Convert an operand to a local, materializing a temp if necessary.
    fn operand_to_local(&mut self, op: Operand, ty: Ty) -> Local {
        match op {
            Operand::Copy(Place::Local(l)) | Operand::Move(Place::Local(l)) => l,
            _ => {
                let temp = self.builder.temp(ty);
                self.builder.assign(Place::local(temp), Rvalue::Use(op));
                temp
            }
        }
    }

    /// BEP-044: emit a type-tag switch over the implementor set when calling
    /// a method on an interface-typed receiver. Each arm invokes the
    /// concrete implementor's `<class>.<method>` as a static call.
    ///
    /// Returns `true` when dispatch was emitted. Returns `false` (without
    /// touching the builder) when the receiver isn't interface-typed or no
    /// implementors are registered — the regular call lowering then runs.
    fn try_lower_interface_dispatch(
        &mut self,
        expr_id: AstExprId,
        base: AstExprId,
        method: &Name,
        args: &[AstExprId],
        dest: &Place,
    ) -> bool {
        let dispatch_target = self.interface_dispatch_target_for_expr(base).or_else(|| {
            self.expr_types
                .get(&self.expr_metadata_key(base))
                .and_then(|ty| self.registry_dispatch_target_for_concrete(ty, method))
        });
        let Some((iface_tn, iface_type_args, iface_assoc)) = dispatch_target else {
            return false;
        };
        let recv_tir_ty = self.dispatch_receiver_static_tir_ty(base);
        // Lower receiver to a local we can copy from in every arm.
        let receiver_op = self.lower_to_operand(base);
        let receiver_ty = self.expr_ty(base);
        let recv_local = self.operand_to_local(receiver_op, receiver_ty);
        self.emit_interface_dispatch_switch(
            InterfaceDispatchCall {
                expr_id,
                recv_local,
                recv_tir_ty: recv_tir_ty.as_ref(),
                iface_tn: &iface_tn,
                iface_type_args: &iface_type_args,
                iface_assoc: &iface_assoc,
                method,
                args,
            },
            dest,
        )
    }

    /// Lower the receiver of a method-call path (`receiver_segments` — the path
    /// up to but excluding the method/qualifier) to a single local: a bare root
    /// local is used directly; a field chain is materialized into a temp. Shared
    /// by the interface- and union-receiver dispatch paths.
    fn lower_path_receiver_to_local(
        &mut self,
        callee: AstExprId,
        receiver_segments: &[Name],
        recv_root_local: Local,
    ) -> Local {
        if receiver_segments.len() <= 1 {
            return recv_root_local;
        }
        let recv_ty_idx = receiver_segments.len() - 1;
        let recv_ty = self
            .path_segment_types
            .get(&(self.current_metadata_scope, callee, recv_ty_idx))
            .cloned()
            .map(|t| self.convert_tir_ty_for_runtime(&t))
            .unwrap_or_else(|| Ty::BuiltinUnknown {
                attr: TyAttr::default(),
            });
        let local = self.builder.temp(recv_ty);
        self.lower_multi_segment_path_as_field_chain(
            callee,
            receiver_segments,
            Place::local(local),
        );
        local
    }

    /// Resolve `class.method` to a callable `ItemRef` by simple name.
    fn class_method_item_ref_by_name(&self, class_tn: &TypeName, method: &Name) -> Option<ItemRef> {
        let class_loc = self.resolve_class_loc_by_type_name(class_tn)?;
        let item_tree = file_item_tree(self.db, class_loc.file(self.db));
        let class_data = &item_tree[class_loc.id(self.db)];
        let func_id = class_data
            .methods
            .iter()
            .copied()
            .find(|&id| item_tree[id].name == *method)?;
        let func_loc =
            baml_compiler2_hir::loc::FunctionLoc::new(self.db, class_loc.file(self.db), func_id);
        Some(method_item_ref(self.db, class_loc, func_loc))
    }

    /// A method call whose receiver is a *union of concrete classes* (e.g. the
    /// `Dog | Cat` produced by `if`/`match` arms) — dispatch by runtime class.
    /// Each member must declare `method`; otherwise this isn't a uniform call we
    /// can lower and we fall through (the caller reports the real error).
    fn try_lower_union_dispatch(
        &mut self,
        expr_id: AstExprId,
        base: AstExprId,
        method: &Name,
        args: &[AstExprId],
        dest: &Place,
    ) -> bool {
        let Some(members) = self
            .expr_types
            .get(&self.expr_metadata_key(base))
            .and_then(Self::tir_union_members)
        else {
            return false;
        };
        // Lower the receiver once; copy it into every arm.
        let receiver_op = self.lower_to_operand(base);
        let receiver_ty = self.expr_ty(base);
        let recv_local = self.operand_to_local(receiver_op, receiver_ty);
        self.emit_union_class_dispatch(recv_local, &members, method, expr_id, args, dest)
    }

    /// A method call whose receiver is a union that contains at least one
    /// *interface* member (e.g. `Animal | Vehicle`, where every member declares
    /// `method`). BEP-044: a method present on every union member dispatches on
    /// the runtime class. Expand each interface member to its implementor
    /// candidates (and each class member to itself) and emit one class-tag
    /// switch. Falls through (returns false) if any member contributes no
    /// candidate, so the caller can report the real error.
    fn try_lower_union_iface_dispatch(
        &mut self,
        expr_id: AstExprId,
        base: AstExprId,
        method: &Name,
        args: &[AstExprId],
        dest: &Place,
    ) -> bool {
        let Some(members) = self
            .expr_types
            .get(&self.expr_metadata_key(base))
            .and_then(Self::tir_union_members)
        else {
            return false;
        };
        let Some(candidates) = self.union_iface_method_candidates(&members, method) else {
            return false;
        };
        let receiver_op = self.lower_to_operand(base);
        let receiver_ty = self.expr_ty(base);
        let recv_local = self.operand_to_local(receiver_op, receiver_ty);
        self.emit_method_candidate_switch(recv_local, &candidates, expr_id, args, dest, None)
    }

    /// Build the runtime-class dispatch candidates for calling `method` on a
    /// union receiver that contains at least one interface member. Returns
    /// `None` (caller falls through) for a pure class union (handled elsewhere)
    /// or when any member contributes no candidate.
    fn union_iface_method_candidates(
        &self,
        members: &[Tir2Ty],
        method: &Name,
    ) -> Option<Vec<InterfaceMethodCandidate>> {
        // This runs only after `emit_union_class_dispatch` (the direct-method
        // class-union path) has declined, so it also covers a *pure class* union
        // whose members satisfy `method` through an inherited interface default
        // (`class Dog { implements Greeter {} }`), not just unions that name an
        // interface member directly.
        let mut candidates: Vec<InterfaceMethodCandidate> = Vec::new();
        for member in members {
            match member {
                Tir2Ty::Class(qtn, _, _) => {
                    let class_tn = qtn.clone();
                    let member_candidates = self.class_member_method_candidates(&class_tn, method);
                    if member_candidates.is_empty() {
                        return None;
                    }
                    candidates.extend(member_candidates);
                }
                Tir2Ty::Interface(..) => {
                    let (iface_tn, iface_type_args, iface_assoc) =
                        self.interface_dispatch_target_for_tir_ty(member)?;
                    let member_candidates = self.interface_method_candidates_for(
                        &iface_tn,
                        &iface_type_args,
                        &iface_assoc,
                        method,
                        None,
                    );
                    if member_candidates.is_empty() {
                        return None;
                    }
                    candidates.extend(member_candidates);
                }
                _ => return None,
            }
        }
        Some(candidates)
    }

    /// Dispatch candidates for calling `method` on a concrete-class union member.
    /// A direct own/override method dispatches on the class tag; otherwise the
    /// method may be supplied by an interface the class implements — an inherited
    /// `implements I {}` default, a field-link, or an out-of-body
    /// `implements I for C` — so fall back to the same implementor resolution
    /// used for interface arms. Without this, a `Dog | Cat` union whose members
    /// satisfy a method only through an inherited default resolved to nothing and
    /// the call dispatched as a map read (VM `expected map, got instance`).
    fn class_member_method_candidates(
        &self,
        class_tn: &TypeName,
        method: &Name,
    ) -> Vec<InterfaceMethodCandidate> {
        if let Some(item_ref) = self.class_method_item_ref_by_name(class_tn, method) {
            return vec![InterfaceMethodCandidate {
                guard: InterfaceDispatchGuard::Type(Ty::Class(
                    class_tn.clone(),
                    Vec::new(),
                    TyAttr::default(),
                )),
                item_ref,
                // Union-member direct dispatch: frame type args not threaded
                // here. (Unchanged from prior behavior.)
                frame_seed: CalleeFrameSeed::Static(Vec::new()),
            }];
        }
        let Some(class_loc) = self.resolve_class_loc_by_type_name(class_tn) else {
            return Vec::new();
        };
        let class_tree = file_item_tree(self.db, class_loc.file(self.db));
        let class_data = &class_tree[class_loc.id(self.db)];
        let mut out = Vec::new();
        for impl_block in &class_data.implements {
            if let Some((iface_tn, iface_args, iface_assoc)) = self.resolve_implements_target_view(
                &impl_block.target,
                &impl_block.associated_type_bindings,
                class_loc,
            ) {
                out.extend(self.resolve_implementor_method_candidates(
                    class_tn,
                    &iface_tn,
                    &iface_args,
                    &iface_assoc,
                    method,
                    None,
                ));
            }
        }
        out
    }

    /// Field-read candidates for a concrete-class union member whose `field` is
    /// supplied by an interface view (`implements Named { name as full }`) rather
    /// than a class-owned slot. Mirrors [`class_member_method_candidates`].
    fn class_member_field_candidates(
        &self,
        class_tn: &TypeName,
        field: &Name,
    ) -> Vec<InterfaceFieldCandidate> {
        let Some(class_loc) = self.resolve_class_loc_by_type_name(class_tn) else {
            return Vec::new();
        };
        let class_tree = file_item_tree(self.db, class_loc.file(self.db));
        let class_data = &class_tree[class_loc.id(self.db)];
        let mut out = Vec::new();
        for impl_block in &class_data.implements {
            if let Some((iface_tn, iface_args, iface_assoc)) = self.resolve_implements_target_view(
                &impl_block.target,
                &impl_block.associated_type_bindings,
                class_loc,
            ) {
                out.extend(self.resolve_implementor_interface_field_candidates(
                    class_tn,
                    &iface_tn,
                    &iface_args,
                    &iface_assoc,
                    field,
                ));
            }
        }
        out
    }

    /// Emit a class-tag dispatch switch for a method call whose receiver
    /// (`recv_local`) has the union type `members`. Returns false (lowering
    /// nothing) unless every member is a class declaring `method`.
    fn emit_union_class_dispatch(
        &mut self,
        recv_local: Local,
        members: &[Tir2Ty],
        method: &Name,
        expr_id: AstExprId,
        args: &[AstExprId],
        dest: &Place,
    ) -> bool {
        let mut arms: Vec<(TypeName, ItemRef)> = Vec::new();
        for member in members {
            let Tir2Ty::Class(qtn, _, _) = member else {
                return false;
            };
            let class_tn = qtn.clone();
            let Some(item_ref) = self.class_method_item_ref_by_name(&class_tn, method) else {
                return false;
            };
            arms.push((class_tn, item_ref));
        }
        if arms.is_empty() {
            return false;
        }

        let arg_ops = self.lower_call_arg_operands(expr_id, args);
        let unwind = self.catch_context.as_ref().map(|c| c.unwind_target);

        let bb_join = self.builder.create_block();
        let bb_otherwise = self.builder.create_block();
        let mut next_check = self.builder.current_block();
        for (idx, (class_tn, item_ref)) in arms.iter().enumerate() {
            let bb_body = self.builder.create_block();
            let bb_next = if idx + 1 == arms.len() {
                bb_otherwise
            } else {
                self.builder.create_block()
            };
            self.builder.set_current_block(next_check);
            self.emit_is_type_branch(
                recv_local,
                Ty::Class(class_tn.clone(), Vec::new(), TyAttr::default()),
                bb_body,
                bb_next,
            );
            self.builder.set_current_block(bb_body);
            let callee_op = Operand::Constant(Constant::Function(item_ref.clone()));
            let mut all_args = vec![Operand::Copy(Place::Local(recv_local))];
            all_args.extend(arg_ops.iter().cloned());
            self.builder
                .call(callee_op, all_args, dest.clone(), bb_join, unwind);
            next_check = bb_next;
        }
        self.builder.set_current_block(bb_otherwise);
        self.builder.unreachable();
        self.builder.set_current_block(bb_join);
        true
    }

    fn emit_interface_dispatch_switch(
        &mut self,
        call: InterfaceDispatchCall<'_>,
        dest: &Place,
    ) -> bool {
        let InterfaceDispatchCall {
            expr_id,
            recv_local,
            recv_tir_ty,
            iface_tn,
            iface_type_args,
            iface_assoc,
            method,
            args,
        } = call;
        let resolved = self.interface_method_candidates_for(
            iface_tn,
            iface_type_args,
            iface_assoc,
            method,
            recv_tir_ty,
        );
        if resolved.is_empty() {
            return false;
        }
        self.emit_method_candidate_switch(
            recv_local,
            &resolved,
            expr_id,
            args,
            dest,
            Some(InterfaceDefaultCallContext {
                iface_tn,
                iface_type_args,
                iface_assoc,
                method,
            }),
        )
    }

    /// Resolve every concrete-class dispatch candidate for calling `method`
    /// through interface `iface_tn` (with `iface_type_args`): each in-body
    /// implementor's method (or the interface default it inherits) plus every
    /// out-of-body / primitive type implementor. Shared by the single-interface
    /// receiver path and the union-receiver path.
    fn interface_method_candidates_for(
        &self,
        iface_tn: &TypeName,
        iface_type_args: &[Tir2Ty],
        iface_assoc: &[(Name, Tir2Ty)],
        method: &Name,
        recv_tir_ty: Option<&Tir2Ty>,
    ) -> Vec<InterfaceMethodCandidate> {
        let mut class_impls = self
            .interface_implementors
            .get(iface_tn)
            .cloned()
            .unwrap_or_default();
        if let Some(Tir2Ty::Class(qtn, _, _)) = recv_tir_ty {
            let recv_tn = qtn.clone();
            if !class_impls.iter().any(|tn| tn == &recv_tn) {
                class_impls.push(recv_tn);
            }
        }
        let type_impls = self
            .interface_type_implementors
            .get(iface_tn)
            .cloned()
            .unwrap_or_default();
        // Resolve the call target for every implementor. If the implementor
        // doesn't directly declare the method, fall back to the interface
        // whose default it inherits.
        let mut resolved: Vec<InterfaceMethodCandidate> = class_impls
            .iter()
            .flat_map(|impl_tn| {
                self.resolve_implementor_method_candidates(
                    impl_tn,
                    iface_tn,
                    iface_type_args,
                    iface_assoc,
                    method,
                    recv_tir_ty,
                )
            })
            .collect();
        for implementor in &type_impls {
            let Some((item_ref, frame_type_args)) = self.resolve_type_implementor_method(
                &implementor.tir_ty,
                iface_tn,
                iface_type_args,
                iface_assoc,
                method,
            ) else {
                continue;
            };
            resolved.push(InterfaceMethodCandidate {
                guard: InterfaceDispatchGuard::Type(implementor.runtime_ty.clone()),
                item_ref,
                frame_seed: CalleeFrameSeed::Static(frame_type_args),
            });
        }
        if let Some(recv_tir_ty) = recv_tir_ty
            && !matches!(
                recv_tir_ty,
                Tir2Ty::Class(..) | Tir2Ty::Interface(..) | Tir2Ty::TypeVar(..)
            )
            && let Some((item_ref, frame_type_args)) = self.resolve_type_implementor_method(
                recv_tir_ty,
                iface_tn,
                iface_type_args,
                iface_assoc,
                method,
            )
        {
            // A concrete non-class receiver (e.g. `int[]`, `map<string, T>`)
            // statically pins exactly one implementor of `method` — there is
            // nothing to discriminate at runtime. Return that single candidate
            // so dispatch lowers to an unconditional call: this is both an
            // optimization and a correctness fix, since a container receiver's
            // `Type` guard (`int[]`) has no representable `IsType` form and
            // would otherwise always fail and hit the `unreachable` arm.
            let runtime_ty = self.convert_tir_ty_for_runtime(recv_tir_ty);
            return vec![InterfaceMethodCandidate {
                guard: InterfaceDispatchGuard::Type(runtime_ty),
                item_ref,
                frame_seed: CalleeFrameSeed::Static(frame_type_args),
            }];
        }
        resolved
    }

    /// Emit a runtime class-tag switch that calls the matching
    /// `InterfaceMethodCandidate` for `recv_local`. Returns false (emitting
    /// nothing) when there are no candidates.
    fn emit_method_candidate_switch(
        &mut self,
        recv_local: Local,
        resolved: &[InterfaceMethodCandidate],
        expr_id: AstExprId,
        args: &[AstExprId],
        dest: &Place,
        interface_default_context: Option<InterfaceDefaultCallContext<'_>>,
    ) -> bool {
        if resolved.is_empty() {
            return false;
        }

        // Lower args once; same operands used in every arm.
        let arg_ops = self.lower_call_arg_operands(expr_id, args);
        let type_arg_ops = self.lower_call_type_args(expr_id, true, None);
        let has_explicit_type_args = self.call_has_explicit_type_args(expr_id);
        let ntypeargs = type_arg_ops.len();
        let unwind = self.catch_context.as_ref().map(|c| c.unwind_target);

        let bb_entry = self.builder.current_block();
        let bb_join = self.builder.create_block();
        let bb_otherwise = self.builder.create_block();

        // A single candidate means the static receiver type admits exactly one
        // implementor, so it provably matches at runtime and the guard is
        // redundant. Skipping it also handles concrete container receivers
        // (e.g. `int[]` dispatched through a blanket `implements ... for T[]`),
        // whose `IsType` guard has no representable form and would otherwise
        // always fail and fall to the `unreachable` arm.
        let single_candidate = resolved.len() == 1;

        let mut next_check = bb_entry;
        for (idx, candidate) in resolved.iter().enumerate() {
            let bb_body = self.builder.create_block();
            let bb_next = if idx + 1 == resolved.len() {
                bb_otherwise
            } else {
                self.builder.create_block()
            };

            self.builder.set_current_block(next_check);
            if single_candidate {
                self.builder.goto(bb_body);
            } else {
                self.emit_interface_dispatch_guard_branch(
                    recv_local,
                    &candidate.guard,
                    bb_body,
                    bb_next,
                );
            }
            self.builder.set_current_block(bb_body);
            // Seed the callee frame's `type_args` so a dispatched method that
            // reads its enclosing `T` at runtime — e.g. building `Other<T>{}` or
            // `reflect.type_of<T>()` — resolves it correctly. Without this,
            // inferred generic instances carry `unknown` class type args and
            // downstream interface dispatch falls to the wrong implementor.
            match &candidate.frame_seed {
                CalleeFrameSeed::FromReceiverInstance => {
                    // Bind the receiver so the VM seeds `frame.type_args` from
                    // the instance's `class_type_args` (class-param order) and
                    // then the method call's type args — handling `Any`/partial
                    // guards that a static seed can't name.
                    let bm = self.builder.temp(Ty::unknown());
                    self.builder.assign(
                        Place::local(bm),
                        Rvalue::MakeBoundMethod {
                            item_ref: candidate.item_ref.clone(),
                            receiver: Operand::Copy(Place::Local(recv_local)),
                        },
                    );
                    // A `BoundMethod` callee inserts the receiver itself, so the
                    // the operands carry only the call type args + values.
                    let mut all_args = type_arg_ops.clone();
                    all_args.extend(arg_ops.iter().cloned());
                    self.builder.call_with_type_args(
                        Operand::Copy(Place::local(bm)),
                        all_args,
                        ntypeargs,
                        dest.clone(),
                        bb_join,
                        unwind,
                    );
                }
                CalleeFrameSeed::Static(tys) => {
                    let callee_op =
                        Operand::Constant(Constant::Function(candidate.item_ref.clone()));
                    let call_type_arg_ops = if let Some(context) = interface_default_context
                        && Self::item_ref_is_interface_method(
                            &candidate.item_ref,
                            context.iface_tn,
                            context.method,
                        ) {
                        let mut owner_tys = context.iface_type_args.to_vec();
                        owner_tys.extend(self.interface_assoc_frame_tys(
                            context.iface_tn,
                            context.iface_type_args,
                            context.iface_assoc,
                        ));
                        let mut owner_ops = self.emit_frame_type_arg_ops(&owner_tys);
                        let owner_arg_count = owner_tys.len();
                        let method_arg_count = self
                            .interface_method_generic_count(context.iface_tn, context.method)
                            .unwrap_or(0);
                        if has_explicit_type_args
                            || (method_arg_count > 0 && type_arg_ops.len() == method_arg_count)
                        {
                            owner_ops.extend(type_arg_ops.iter().cloned());
                        } else if type_arg_ops.len() == owner_arg_count + method_arg_count {
                            owner_ops.extend(type_arg_ops[owner_arg_count..].iter().cloned());
                        } else {
                            owner_ops.extend(type_arg_ops.iter().cloned());
                        }
                        owner_ops
                    } else {
                        type_arg_ops.clone()
                    };
                    // De Bruijn order: class/iface params first, then method
                    // call type args. Lowered per-arm because the class/iface
                    // seed can vary by candidate.
                    let frame_type_arg_ops = self.emit_frame_type_arg_ops(tys);
                    let arm_ntypeargs = frame_type_arg_ops.len() + call_type_arg_ops.len();
                    let mut all_args = frame_type_arg_ops;
                    all_args.extend(call_type_arg_ops);
                    all_args.push(Operand::Copy(Place::Local(recv_local)));
                    all_args.extend(arg_ops.iter().cloned());
                    self.builder.call_with_type_args(
                        callee_op,
                        all_args,
                        arm_ntypeargs,
                        dest.clone(),
                        bb_join,
                        unwind,
                    );
                }
            }
            next_check = bb_next;
        }

        self.builder.set_current_block(bb_otherwise);
        self.builder.unreachable();

        self.builder.set_current_block(bb_join);
        true
    }

    fn interface_assoc_frame_tys(
        &self,
        iface_tn: &TypeName,
        iface_type_args: &[Tir2Ty],
        iface_assoc: &[(Name, Tir2Ty)],
    ) -> Vec<Tir2Ty> {
        let Some((iface_loc, iface_names)) = self.interface_associated_type_names(iface_tn) else {
            return Vec::new();
        };
        let pkg_info =
            baml_compiler2_hir::file_package::file_package(self.db, iface_loc.file(self.db));
        let pkg_items = self.resolve_class_pkg_items_by_name(&pkg_info.package);
        let completed_assoc =
            baml_compiler2_tir::interfaces::interface_closure_locs_with_args_and_assoc(
                self.db,
                iface_loc,
                iface_type_args,
                iface_assoc,
                pkg_items,
                &pkg_info.namespace_path,
            )
            .into_iter()
            .next()
            .map(|(_, _, assoc)| assoc)
            .unwrap_or_else(|| iface_assoc.to_vec());

        iface_names
            .into_iter()
            .map(|name| {
                completed_assoc
                    .iter()
                    .find(|(assoc_name, _)| assoc_name == &name)
                    .map(|(_, ty)| ty.clone())
                    .unwrap_or_else(|| Tir2Ty::BuiltinUnknown {
                        attr: TyAttr::default(),
                    })
            })
            .collect()
    }

    fn interface_associated_type_names(
        &self,
        iface_tn: &TypeName,
    ) -> Option<(baml_compiler2_hir::loc::InterfaceLoc<'db>, Vec<Name>)> {
        let iface_pkg_name = iface_tn.package();
        let iface_pkg_items = self.resolve_class_pkg_items_by_name(iface_pkg_name);
        let iface_ns: Vec<Name> = iface_tn.namespace().clone();
        let Definition::Interface(iface_loc) =
            iface_pkg_items.lookup_type(&iface_ns, iface_tn.name())?
        else {
            return None;
        };
        let iface_tree = baml_compiler2_hir::file_item_tree(self.db, iface_loc.file(self.db));
        let iface_data = iface_tree.interfaces.get(&iface_loc.id(self.db))?;
        Some((
            iface_loc,
            iface_data
                .associated_types
                .iter()
                .map(|assoc| assoc.name.clone())
                .collect(),
        ))
    }

    fn item_ref_is_interface_method(
        item_ref: &ItemRef,
        iface_tn: &TypeName,
        method: &Name,
    ) -> bool {
        let ItemRef::Method {
            package,
            namespace,
            class,
            name,
        } = item_ref
        else {
            return false;
        };
        name == method
            && class == iface_tn.name()
            && iface_tn.package() == package
            && iface_tn.namespace().iter().eq(namespace.iter())
    }

    fn interface_method_generic_count(&self, iface_tn: &TypeName, method: &Name) -> Option<usize> {
        let iface_pkg_name = iface_tn.package();
        let iface_pkg_items = self.resolve_class_pkg_items_by_name(iface_pkg_name);
        let iface_ns: Vec<Name> = iface_tn.namespace().clone();
        let Definition::Interface(iface_loc) =
            iface_pkg_items.lookup_type(&iface_ns, iface_tn.name())?
        else {
            return None;
        };
        let iface_tree = baml_compiler2_hir::file_item_tree(self.db, iface_loc.file(self.db));
        let iface_data = iface_tree.interfaces.get(&iface_loc.id(self.db))?;
        iface_data
            .default_methods
            .iter()
            .find_map(|fn_id| {
                let func = &iface_tree[*fn_id];
                (func.name == *method).then_some(func.generic_params.len())
            })
            .or_else(|| {
                iface_data
                    .required_methods
                    .iter()
                    .find_map(|sig| (sig.name == *method).then_some(sig.generic_params.len()))
            })
    }

    /// Per-arm analogue of [`Self::emit_method_candidate_switch`] that *binds* the
    /// matching implementor's method as a bound-method value rather than calling
    /// it. For `let f = x.eq` where `x`'s static type is a generic `T extends I`
    /// (or a bare interface) there is no single concrete method to bind at compile
    /// time, so this switches on the receiver's runtime type and binds the right
    /// concrete method. The receiver is captured at bind time (its runtime type is
    /// fixed then), so a later `f(y)` calls the correct concrete method — exactly
    /// as `x.eq(y)` would dispatch directly.
    ///
    /// `resolved` must be non-empty: callers resolve candidates first (via
    /// [`Self::interface_method_candidates_for`]) so they can tell a method from a
    /// field — and avoid lowering the receiver — *before* committing to this path.
    fn emit_bound_method_candidate_switch(
        &mut self,
        recv_local: Local,
        resolved: &[InterfaceMethodCandidate],
        dest: &Place,
    ) {
        let bb_entry = self.builder.current_block();
        let bb_join = self.builder.create_block();
        let bb_otherwise = self.builder.create_block();

        // See `emit_method_candidate_switch`: a single candidate provably
        // matches, so the guard is redundant (and unrepresentable for some
        // concrete container receiver types).
        let single_candidate = resolved.len() == 1;

        let mut next_check = bb_entry;
        for (idx, candidate) in resolved.iter().enumerate() {
            let bb_body = self.builder.create_block();
            let bb_next = if idx + 1 == resolved.len() {
                bb_otherwise
            } else {
                self.builder.create_block()
            };

            self.builder.set_current_block(next_check);
            if single_candidate {
                self.builder.goto(bb_body);
            } else {
                self.emit_interface_dispatch_guard_branch(
                    recv_local,
                    &candidate.guard,
                    bb_body,
                    bb_next,
                );
            }
            self.builder.set_current_block(bb_body);
            self.builder.assign(
                dest.clone(),
                Rvalue::MakeBoundMethod {
                    item_ref: candidate.item_ref.clone(),
                    receiver: Operand::Copy(Place::Local(recv_local)),
                },
            );
            self.builder.goto(bb_join);
            next_check = bb_next;
        }

        self.builder.set_current_block(bb_otherwise);
        self.builder.unreachable();

        self.builder.set_current_block(bb_join);
    }

    fn try_lower_interface_field_access(
        &mut self,
        recv_local: Local,
        iface_tn: &TypeName,
        iface_type_args: &[Tir2Ty],
        iface_assoc: &[(Name, Tir2Ty)],
        field: &Name,
        dest: &Place,
    ) -> bool {
        let Some(impls) = self.interface_implementors.get(iface_tn).cloned() else {
            return false;
        };
        let resolved: Vec<InterfaceFieldCandidate> = impls
            .iter()
            .flat_map(|impl_tn| {
                self.resolve_implementor_interface_field_candidates(
                    impl_tn,
                    iface_tn,
                    iface_type_args,
                    iface_assoc,
                    field,
                )
            })
            .collect();
        self.emit_interface_field_candidate_switch(recv_local, &resolved, dest)
    }

    /// A field read whose receiver is a union with at least one interface member
    /// (e.g. `(Dog | Named).name`). Each member contributes the concrete classes
    /// it can be at runtime — a class member is itself; an interface member is
    /// every implementor (reading the linked field view) — and we dispatch on
    /// the runtime class. Returns false (caller falls through) if any member
    /// contributes no candidate.
    fn lower_union_iface_field_access(
        &mut self,
        recv_local: Local,
        members: &[Tir2Ty],
        field: &Name,
        dest: &Place,
    ) -> bool {
        // Runs only after `lower_union_class_field_access` declines, so it also
        // covers a pure class union whose `field` is supplied by an interface
        // view rather than a class-owned slot.
        let mut resolved: Vec<InterfaceFieldCandidate> = Vec::new();
        for member in members {
            match member {
                Tir2Ty::Class(qtn, _, _) => {
                    let class_tn = qtn.clone();
                    if let Some(field_idx) = self
                        .class_fields
                        .get(&class_tn)
                        .and_then(|fields| fields.get(field.as_str()))
                        .copied()
                    {
                        resolved.push(InterfaceFieldCandidate {
                            impl_tn: class_tn,
                            guard: InterfaceClassGuard::Any,
                            field_idx,
                        });
                    } else {
                        // The field may be supplied by an interface view
                        // (`implements Named { name as full }`) rather than a
                        // class-owned slot. Resolve it through the class's
                        // implemented interfaces, mirroring the method path.
                        let member_candidates =
                            self.class_member_field_candidates(&class_tn, field);
                        if member_candidates.is_empty() {
                            return false;
                        }
                        resolved.extend(member_candidates);
                    }
                }
                Tir2Ty::Interface(..) => {
                    let Some((iface_tn, iface_type_args, iface_assoc)) =
                        self.interface_dispatch_target_for_tir_ty(member)
                    else {
                        return false;
                    };
                    let Some(impls) = self.interface_implementors.get(&iface_tn).cloned() else {
                        return false;
                    };
                    let member_candidates: Vec<InterfaceFieldCandidate> = impls
                        .iter()
                        .flat_map(|impl_tn| {
                            self.resolve_implementor_interface_field_candidates(
                                impl_tn,
                                &iface_tn,
                                &iface_type_args,
                                &iface_assoc,
                                field,
                            )
                        })
                        .collect();
                    if member_candidates.is_empty() {
                        return false;
                    }
                    resolved.extend(member_candidates);
                }
                _ => return false,
            }
        }
        self.emit_interface_field_candidate_switch(recv_local, &resolved, dest)
    }

    /// Emit a runtime class-tag switch that reads `field` from `recv_local`
    /// using whichever `InterfaceFieldCandidate` matches. Returns false
    /// (emitting nothing) when there are no candidates.
    fn emit_interface_field_candidate_switch(
        &mut self,
        recv_local: Local,
        resolved: &[InterfaceFieldCandidate],
        dest: &Place,
    ) -> bool {
        if resolved.is_empty() {
            return false;
        }

        let bb_entry = self.builder.current_block();
        let bb_join = self.builder.create_block();
        let bb_otherwise = self.builder.create_block();
        let mut next_check = bb_entry;

        for (idx, candidate) in resolved.iter().enumerate() {
            let bb_body = self.builder.create_block();
            let bb_next = if idx + 1 == resolved.len() {
                bb_otherwise
            } else {
                self.builder.create_block()
            };

            self.builder.set_current_block(next_check);
            self.emit_interface_class_guard_branch(
                recv_local,
                &candidate.impl_tn,
                &candidate.guard,
                bb_body,
                bb_next,
            );
            self.builder.set_current_block(bb_body);
            self.builder.assign(
                dest.clone(),
                Rvalue::Use(Operand::Copy(Place::Field {
                    base: Box::new(Place::Local(recv_local)),
                    field: candidate.field_idx,
                })),
            );
            self.builder.goto(bb_join);
            next_check = bb_next;
        }

        self.builder.set_current_block(bb_otherwise);
        self.builder.unreachable();

        self.builder.set_current_block(bb_join);
        true
    }

    /// Resolve candidate functions for `impl_tn` when dispatching `method` on
    /// `iface_tn`. Generic class implementors can satisfy multiple
    /// instantiations of the same interface, so each candidate carries a class
    /// guard that may include concrete class type args.
    fn resolve_implementor_method_candidates(
        &self,
        impl_tn: &TypeName,
        iface_tn: &TypeName,
        iface_type_args: &[Tir2Ty],
        iface_assoc: &[(Name, Tir2Ty)],
        method: &Name,
        recv_tir_ty: Option<&Tir2Ty>,
    ) -> Vec<InterfaceMethodCandidate> {
        let Some(class_loc) = self.resolve_class_loc_by_type_name(impl_tn) else {
            return Vec::new();
        };
        let class_tree = file_item_tree(self.db, class_loc.file(self.db));
        let class_data = &class_tree[class_loc.id(self.db)];
        let Some(requested_views) =
            self.interface_closure_type_name_views(iface_tn, iface_type_args, iface_assoc)
        else {
            return Vec::new();
        };
        let class_owned_method_ids: Vec<_> = class_data
            .methods
            .iter()
            .copied()
            .filter(|method_id| {
                class_tree[*method_id].name == *method
                    && !class_tree.method_to_iface_target.contains_key(method_id)
            })
            .collect();

        let mut out = Vec::new();
        for &method_id in &class_data.methods {
            if class_tree[method_id].name != *method {
                continue;
            }
            let Some(target) = class_tree.method_to_iface_target.get(&method_id) else {
                continue;
            };
            let method_assoc_bindings = class_tree
                .method_to_iface_associated_type_bindings
                .get(&method_id)
                .map(Vec::as_slice)
                .unwrap_or(&[]);
            // BEP-044: this override only satisfies requests resolving to the
            // interface that owns `method` within the block's closure — so a
            // `B::foo` override never leaks into a request for `A::foo` when
            // `B requires A` and both declare `foo`.
            let provider_view =
                self.method_provider_view(target, method_assoc_bindings, class_loc, method);
            for (requested_idx, matched_iface_tn, guard) in self
                .implements_target_matches_requested_views(
                    target,
                    method_assoc_bindings,
                    class_loc,
                    &requested_views,
                    &class_data.generic_params,
                )
            {
                if let Some(provider) = &provider_view
                    && matched_iface_tn != *provider
                {
                    continue;
                }
                let func_loc = baml_compiler2_hir::loc::FunctionLoc::new(
                    self.db,
                    class_loc.file(self.db),
                    method_id,
                );
                // A class-owned override reads class-level `T` from its frame
                // type args (class-param order): statically when the guard pins
                // them, otherwise from the matched runtime instance.
                let frame_seed =
                    class_owned_frame_seed(&guard, !class_data.generic_params.is_empty());
                out.push((
                    requested_idx,
                    InterfaceMethodCandidate {
                        guard: InterfaceDispatchGuard::Class {
                            impl_tn: impl_tn.clone(),
                            guard,
                        },
                        item_ref: method_item_ref(self.db, class_loc, func_loc),
                        frame_seed,
                    },
                ));
            }
        }

        for impl_block in &class_data.implements {
            let provider_view = self.method_provider_view(
                &impl_block.target,
                &impl_block.associated_type_bindings,
                class_loc,
                method,
            );
            for (requested_idx, matched_iface_tn, guard) in self
                .implements_target_matches_requested_views(
                    &impl_block.target,
                    &impl_block.associated_type_bindings,
                    class_loc,
                    &requested_views,
                    &class_data.generic_params,
                )
            {
                if let Some(provider) = &provider_view
                    && matched_iface_tn != *provider
                {
                    continue;
                }
                if !class_owned_method_ids.is_empty()
                    && !self.class_method_candidate_already_resolved(
                        &out,
                        requested_idx,
                        impl_tn,
                        &guard,
                    )
                {
                    for &method_id in &class_owned_method_ids {
                        let func_loc = baml_compiler2_hir::loc::FunctionLoc::new(
                            self.db,
                            class_loc.file(self.db),
                            method_id,
                        );
                        let frame_seed =
                            class_owned_frame_seed(&guard, !class_data.generic_params.is_empty());
                        out.push((
                            requested_idx,
                            InterfaceMethodCandidate {
                                guard: InterfaceDispatchGuard::Class {
                                    impl_tn: impl_tn.clone(),
                                    guard: guard.clone(),
                                },
                                item_ref: method_item_ref(self.db, class_loc, func_loc),
                                frame_seed,
                            },
                        ));
                    }
                    continue;
                }
                let Some(item_ref) =
                    self.interface_default_method_item_ref(&matched_iface_tn, method)
                else {
                    continue;
                };
                out.push((
                    requested_idx,
                    InterfaceMethodCandidate {
                        guard: InterfaceDispatchGuard::Class {
                            impl_tn: impl_tn.clone(),
                            guard,
                        },
                        item_ref,
                        frame_seed: CalleeFrameSeed::Static(Vec::new()),
                    },
                ));
            }
        }

        // Out-of-body generic impl methods live in `item_tree.implements_for`
        // rather than the class body. Match them through the same TIR rule
        // machinery as subtype checking instead of reconstructing TIR from MIR
        // types or wildcarding interface args.
        if out.is_empty() {
            let Some(candidate_class_qtn) = self.resolve_qtn_by_type_name(impl_tn) else {
                return Vec::new();
            };
            let candidate_class_ty = Tir2Ty::Class(
                candidate_class_qtn.clone(),
                match recv_tir_ty {
                    Some(Tir2Ty::Class(qtn, args, _)) if qtn == &candidate_class_qtn => {
                        args.clone()
                    }
                    _ => Vec::new(),
                },
                baml_compiler2_tir::ty::TyAttr::default(),
            );
            'blanket_search: for file in compiler2_all_files(self.db) {
                let file_pkg_info = file_package(self.db, file);
                let file_pkg_items = self.resolve_class_pkg_items_by_name(&file_pkg_info.package);
                let file_item_tree = file_item_tree(self.db, file);
                for imp in &file_item_tree.implements_for {
                    let mut diags = Vec::new();
                    let target_ty_tir = baml_compiler2_tir::lower_type_expr::lower_type_expr_in_ns(
                        self.db,
                        &imp.for_target.expr,
                        file_pkg_items,
                        &file_pkg_info.namespace_path,
                        &imp.generic_params,
                        &mut diags,
                    );
                    let Some(root_iface_loc) =
                        baml_compiler2_tir::interfaces::resolve_path_to_interface(
                            self.db,
                            &imp.interface_target.expr,
                            file_pkg_items,
                            &file_pkg_info.namespace_path,
                        )
                    else {
                        continue;
                    };
                    let iface_tree =
                        baml_compiler2_hir::file_item_tree(self.db, root_iface_loc.file(self.db));
                    let Some(root_iface_data) =
                        iface_tree.interfaces.get(&root_iface_loc.id(self.db))
                    else {
                        continue;
                    };
                    let root_iface_qtn = baml_compiler2_tir::lower_type_expr::qualify_def(
                        self.db,
                        Definition::Interface(root_iface_loc),
                        &root_iface_data.name,
                    );
                    let root_iface_args_tir: Vec<baml_compiler2_tir::ty::Ty> =
                        match &imp.interface_target.expr {
                            baml_compiler2_ast::TypeExpr::Path { generic_args, .. } => generic_args
                                .iter()
                                .map(|arg| {
                                    baml_compiler2_tir::lower_type_expr::lower_type_expr_in_ns(
                                        self.db,
                                        arg,
                                        file_pkg_items,
                                        &file_pkg_info.namespace_path,
                                        &imp.generic_params,
                                        &mut diags,
                                    )
                                })
                                .collect(),
                            _ => Vec::new(),
                        };
                    let root_iface_assoc_tir = lower_interface_target_associated_bindings(
                        self.db,
                        &imp.interface_target.expr,
                        &imp.associated_type_bindings,
                        file_pkg_items,
                        &file_pkg_info.namespace_path,
                        &imp.generic_params,
                        &mut diags,
                    );
                    let bounds = imp
                        .generic_param_bounds
                        .iter()
                        .map(|bound| {
                            bound.as_ref().map(|bound| {
                                baml_compiler2_tir::lower_type_expr::lower_type_expr_in_ns(
                                    self.db,
                                    bound,
                                    file_pkg_items,
                                    &file_pkg_info.namespace_path,
                                    &imp.generic_params,
                                    &mut diags,
                                )
                            })
                        })
                        .collect();
                    let rule = baml_compiler2_tir::interfaces::InterfaceImplRule {
                        generic_params: imp.generic_params.clone(),
                        generic_param_bounds: bounds,
                        for_ty_pattern: target_ty_tir,
                        interface_ty: Tir2Ty::Interface(
                            root_iface_qtn,
                            root_iface_args_tir,
                            root_iface_assoc_tir,
                            baml_compiler2_tir::ty::TyAttr::default(),
                        ),
                        origin: baml_compiler2_tir::interfaces::InterfaceImplOrigin::OutOfBody,
                    };
                    let candidate_ty =
                        if matches!(rule.for_ty_pattern, baml_compiler2_tir::ty::Ty::TypeVar(..))
                            || baml_compiler2_tir::interfaces::match_ty_pattern(
                                &rule.for_ty_pattern,
                                &candidate_class_ty,
                                &rule.generic_params,
                                &self.resolved_aliases.aliases,
                            )
                            .is_some()
                        {
                            Some(&candidate_class_ty)
                        } else {
                            None
                        };
                    let registry = baml_compiler2_tir::interfaces::package_implements_registry(
                        self.db,
                        PackageId::new(self.db, file_pkg_info.package.clone()),
                    );
                    for (requested_idx, (requested_tn, requested_args, requested_assoc)) in
                        requested_views.iter().enumerate()
                    {
                        let Some(requested_iface_qtn) = self.resolve_qtn_by_type_name(requested_tn)
                        else {
                            continue;
                        };
                        let requested_iface_ty = Tir2Ty::Interface(
                            requested_iface_qtn,
                            requested_args.clone(),
                            requested_assoc.clone(),
                            baml_compiler2_tir::ty::TyAttr::default(),
                        );
                        let instantiation_with_candidate = candidate_ty.and_then(|candidate_ty| {
                            registry.instantiate_rule_for_requested_interface(
                                &rule,
                                &requested_iface_ty,
                                Some(candidate_ty),
                                &self.resolved_aliases.aliases,
                                |actual, bound| {
                                    type_satisfies_bound(
                                        self.db,
                                        actual,
                                        bound,
                                        &self.resolved_aliases.aliases,
                                        &file_pkg_info.package,
                                        BLANKET_BOUND_DEPTH,
                                    )
                                },
                            )
                        });
                        let Some(instantiation) = instantiation_with_candidate.or_else(|| {
                            registry.instantiate_rule_for_requested_interface(
                                &rule,
                                &requested_iface_ty,
                                None,
                                &self.resolved_aliases.aliases,
                                |actual, bound| {
                                    type_satisfies_bound(
                                        self.db,
                                        actual,
                                        bound,
                                        &self.resolved_aliases.aliases,
                                        &file_pkg_info.package,
                                        BLANKET_BOUND_DEPTH,
                                    )
                                },
                            )
                        }) else {
                            continue;
                        };
                        let guard = match &instantiation.for_ty {
                            Tir2Ty::Class(qtn, args, _) if qtn == &candidate_class_qtn => {
                                if args
                                    .iter()
                                    .any(baml_compiler2_tir::generics::contains_typevar)
                                {
                                    InterfaceClassGuard::Any
                                } else {
                                    InterfaceClassGuard::Exact(
                                        args.iter().cloned().map(Some).collect(),
                                    )
                                }
                            }
                            Tir2Ty::TypeVar(..) => InterfaceClassGuard::Any,
                            _ => continue,
                        };

                        let (inst_iface_args, inst_iface_assoc): (&[Tir2Ty], &[(Name, Tir2Ty)]) =
                            match &instantiation.interface_ty {
                                Tir2Ty::Interface(_, args, assoc, _) => (args, assoc),
                                _ => (&[], &[]),
                            };
                        for (iface_loc, current_iface_args, current_iface_assoc) in
                            baml_compiler2_tir::interfaces::interface_closure_locs_with_args_and_assoc(
                                self.db,
                                root_iface_loc,
                                inst_iface_args,
                                inst_iface_assoc,
                                file_pkg_items,
                                &file_pkg_info.namespace_path,
                            )
                        {
                            let Some(current_iface_tn) =
                                interface_type_name_from_loc(self.db, iface_loc)
                            else {
                                continue;
                            };
                            if current_iface_tn != *requested_tn
                                || !self.interface_tir_type_args_match(
                                    &current_iface_args,
                                    requested_args,
                                )
                                || !self.interface_tir_assoc_match(
                                    &current_iface_assoc,
                                    requested_assoc,
                                )
                            {
                                continue;
                            }
                            let iface_tree = baml_compiler2_hir::file_item_tree(
                                self.db,
                                iface_loc.file(self.db),
                            );
                            let Some(iface_data) =
                                iface_tree.interfaces.get(&iface_loc.id(self.db))
                            else {
                                continue;
                            };
                            let iface_pkg = baml_compiler2_hir::file_package::file_package(
                                self.db,
                                iface_loc.file(self.db),
                            );
                            if let Some(method_id) = imp
                                .methods
                                .iter()
                                .find(|mid| file_item_tree[**mid].name == *method)
                            {
                                let func_loc = baml_compiler2_hir::loc::FunctionLoc::new(
                                    self.db, file, *method_id,
                                );
                                let frame_type_args = Self::impl_frame_type_args_for_request(
                                    &imp.generic_params,
                                    &instantiation,
                                    &rule.interface_ty,
                                    &requested_iface_ty,
                                );
                                out.push((
                                    requested_idx,
                                    InterfaceMethodCandidate {
                                        guard: InterfaceDispatchGuard::Class {
                                            impl_tn: impl_tn.clone(),
                                            guard,
                                        },
                                        item_ref: def_to_item_ref(
                                            self.db,
                                            baml_compiler2_hir::contributions::Definition::Function(
                                                func_loc,
                                            ),
                                        ),
                                        // Out-of-body override: its frame uses
                                        // `imp.generic_params` order, which can
                                        // differ from both class- and
                                        // interface-arg order.
                                        frame_seed: CalleeFrameSeed::Static(frame_type_args),
                                    },
                                ));
                                break 'blanket_search;
                            }
                            if !class_owned_method_ids.is_empty()
                                && !self.class_method_candidate_already_resolved(
                                    &out,
                                    requested_idx,
                                    impl_tn,
                                    &guard,
                                )
                            {
                                for &method_id in &class_owned_method_ids {
                                    let func_loc = baml_compiler2_hir::loc::FunctionLoc::new(
                                        self.db,
                                        class_loc.file(self.db),
                                        method_id,
                                    );
                                    let frame_seed = class_owned_frame_seed(
                                        &guard,
                                        !class_data.generic_params.is_empty(),
                                    );
                                    out.push((
                                        requested_idx,
                                        InterfaceMethodCandidate {
                                            guard: InterfaceDispatchGuard::Class {
                                                impl_tn: impl_tn.clone(),
                                                guard: guard.clone(),
                                            },
                                            item_ref: method_item_ref(self.db, class_loc, func_loc),
                                            frame_seed,
                                        },
                                    ));
                                }
                                break 'blanket_search;
                            }
                            // No override in blanket impl — check for interface default method
                            for &fn_id in &iface_data.default_methods {
                                if iface_tree[fn_id].name == *method {
                                    out.push((
                                        requested_idx,
                                        InterfaceMethodCandidate {
                                            guard: InterfaceDispatchGuard::Class {
                                                impl_tn: impl_tn.clone(),
                                                guard,
                                            },
                                            item_ref: ItemRef::Method {
                                                package: iface_pkg.package.clone(),
                                                namespace: iface_pkg.namespace_path,
                                                class: iface_data.name.clone(),
                                                name: method.clone(),
                                            },
                                            frame_seed: CalleeFrameSeed::Static(Vec::new()),
                                        },
                                    ));
                                    break 'blanket_search;
                                }
                            }
                        }
                    }
                }
            }
        }

        out.sort_by_key(|(requested_idx, _)| *requested_idx);
        out.into_iter().map(|(_, candidate)| candidate).collect()
    }

    fn interface_tir_type_args_match(
        &self,
        impl_iface_args: &[Tir2Ty],
        iface_type_args: &[Tir2Ty],
    ) -> bool {
        interface_tir_type_args_match_preserving_typevars(
            impl_iface_args,
            iface_type_args,
            &self.resolved_aliases.aliases,
        )
    }

    fn interface_tir_assoc_match(
        &self,
        impl_iface_assoc: &[(Name, Tir2Ty)],
        iface_assoc: &[(Name, Tir2Ty)],
    ) -> bool {
        iface_assoc.iter().all(|(requested_name, requested_ty)| {
            impl_iface_assoc
                .iter()
                .find(|(impl_name, _)| impl_name == requested_name)
                .is_some_and(|(_, impl_ty)| {
                    tir_type_satisfies_dispatch_request(
                        impl_ty,
                        requested_ty,
                        &self.resolved_aliases.aliases,
                    ) || self.tir_types_equivalent(impl_ty, requested_ty)
                })
        })
    }

    fn class_method_candidate_already_resolved(
        &self,
        resolved: &[(usize, InterfaceMethodCandidate)],
        requested_idx: usize,
        impl_tn: &TypeName,
        guard: &InterfaceClassGuard,
    ) -> bool {
        resolved.iter().any(|(idx, candidate)| {
            if *idx != requested_idx {
                return false;
            }
            let InterfaceDispatchGuard::Class {
                impl_tn: candidate_impl_tn,
                guard: candidate_guard,
            } = &candidate.guard
            else {
                return false;
            };
            candidate_impl_tn == impl_tn
                && self.interface_class_guards_equivalent(candidate_guard, guard)
        })
    }

    fn interface_class_guards_equivalent(
        &self,
        a: &InterfaceClassGuard,
        b: &InterfaceClassGuard,
    ) -> bool {
        match (a, b) {
            (InterfaceClassGuard::Any, InterfaceClassGuard::Any) => true,
            (InterfaceClassGuard::Exact(a), InterfaceClassGuard::Exact(b))
                if a.len() == b.len() =>
            {
                a.iter().zip(b).all(|(a, b)| match (a, b) {
                    (None, None) => true,
                    (Some(a), Some(b)) => self.tir_types_equivalent(a, b),
                    _ => false,
                })
            }
            _ => false,
        }
    }

    fn impl_frame_type_args_for_request(
        generic_params: &[Name],
        instantiation: &baml_compiler2_tir::interfaces::InterfaceImplInstantiation,
        rule_iface_ty: &Tir2Ty,
        requested_iface_ty: &Tir2Ty,
    ) -> Vec<Tir2Ty> {
        generic_params
            .iter()
            .map(|param| {
                instantiation
                    .bindings
                    .get(param)
                    .filter(|ty| !Self::is_unresolved_impl_binding_for_param(param, ty))
                    .cloned()
                    .or_else(|| {
                        Self::requested_iface_binding_for_impl_param(
                            param,
                            rule_iface_ty,
                            requested_iface_ty,
                        )
                    })
                    .unwrap_or_else(|| Tir2Ty::BuiltinUnknown {
                        attr: baml_compiler2_tir::ty::TyAttr::default(),
                    })
            })
            .collect()
    }

    fn is_unresolved_impl_binding_for_param(param: &Name, ty: &Tir2Ty) -> bool {
        matches!(ty, Tir2Ty::Unknown { .. } | Tir2Ty::BuiltinUnknown { .. })
            || matches!(ty, Tir2Ty::TypeVar(name, _) if name == param)
    }

    fn requested_iface_binding_for_impl_param(
        param: &Name,
        rule_iface_ty: &Tir2Ty,
        requested_iface_ty: &Tir2Ty,
    ) -> Option<Tir2Ty> {
        let (
            Tir2Ty::Interface(_, rule_args, rule_assoc, _),
            Tir2Ty::Interface(_, requested_args, requested_assoc, _),
        ) = (rule_iface_ty, requested_iface_ty)
        else {
            return None;
        };

        rule_args
            .iter()
            .zip(requested_args.iter())
            .find_map(|(rule_arg, requested_arg)| {
                Self::is_direct_impl_param_reference(param, rule_arg).then(|| requested_arg.clone())
            })
            .or_else(|| {
                rule_assoc.iter().find_map(|(assoc_name, rule_ty)| {
                    if !Self::is_direct_impl_param_reference(param, rule_ty) {
                        return None;
                    }
                    requested_assoc
                        .iter()
                        .find(|(requested_name, _)| requested_name == assoc_name)
                        .map(|(_, requested_ty)| requested_ty.clone())
                })
            })
    }

    fn is_direct_impl_param_reference(param: &Name, ty: &Tir2Ty) -> bool {
        matches!(ty, Tir2Ty::TypeVar(name, _) if name == param)
    }

    fn tir_types_equivalent(&self, a: &Tir2Ty, b: &Tir2Ty) -> bool {
        let resolver = baml_compiler2_tir::associated_projection::AssociatedProjectionResolver::new(
            self.db,
            &self.resolved_aliases.aliases,
            &(),
        );
        let resolved_a = resolver.resolve_deep(a);
        let resolved_b = resolver.resolve_deep(b);
        resolver.types_equivalent(&resolved_a, &resolved_b)
    }

    fn interface_closure_type_name_views(
        &self,
        iface_tn: &TypeName,
        iface_type_args: &[Tir2Ty],
        iface_assoc: &[(Name, Tir2Ty)],
    ) -> Option<Vec<InterfaceTypeView>> {
        let iface_pkg_name = iface_tn.package();
        let iface_pkg_items = self.resolve_class_pkg_items_by_name(iface_pkg_name);
        let iface_ns: Vec<Name> = iface_tn.namespace().clone();
        let Definition::Interface(requested_root_loc) =
            iface_pkg_items.lookup_type(&iface_ns, iface_tn.name())?
        else {
            return None;
        };
        Some(
            baml_compiler2_tir::interfaces::interface_closure_locs_with_args_and_assoc(
                self.db,
                requested_root_loc,
                iface_type_args,
                iface_assoc,
                iface_pkg_items,
                &iface_ns,
            )
            .into_iter()
            .filter_map(|(loc, args, assoc)| {
                Some((interface_type_name_from_loc(self.db, loc)?, args, assoc))
            })
            .collect(),
        )
    }

    fn interface_dispatch_instantiation_request(
        actual_iface_ty: &Tir2Ty,
        requested_iface_ty: &Tir2Ty,
    ) -> Tir2Ty {
        let (
            Tir2Ty::Interface(actual_qtn, actual_args, actual_assoc, _),
            Tir2Ty::Interface(requested_qtn, requested_args, requested_assoc, requested_attr),
        ) = (actual_iface_ty, requested_iface_ty)
        else {
            return requested_iface_ty.clone();
        };
        if actual_qtn != requested_qtn {
            return requested_iface_ty.clone();
        }

        let args = requested_args
            .iter()
            .zip(actual_args.iter())
            .map(|(requested, actual)| rewrite_dispatch_request_ty(actual, requested))
            .collect();
        let assoc = requested_assoc
            .iter()
            .map(|(name, requested)| {
                let rewritten = actual_assoc
                    .iter()
                    .find(|(actual_name, _)| actual_name == name)
                    .map_or_else(
                        || requested.clone(),
                        |(_, actual)| rewrite_dispatch_request_ty(actual, requested),
                    );
                (name.clone(), rewritten)
            })
            .collect();

        Tir2Ty::Interface(requested_qtn.clone(), args, assoc, requested_attr.clone())
    }

    fn resolve_type_implementor_method(
        &self,
        impl_ty_tir: &Tir2Ty,
        iface_tn: &TypeName,
        iface_type_args: &[Tir2Ty],
        iface_assoc: &[(Name, Tir2Ty)],
        method: &Name,
    ) -> Option<(ItemRef, Vec<Tir2Ty>)> {
        let requested_views =
            self.interface_closure_type_name_views(iface_tn, iface_type_args, iface_assoc)?;

        for file in compiler2_all_files(self.db) {
            let pkg_info = file_package(self.db, file);
            let pkg_items = self.resolve_class_pkg_items_by_name(&pkg_info.package);
            let item_tree = file_item_tree(self.db, file);
            for imp in &item_tree.implements_for {
                let mut diags = Vec::new();
                let target_ty_tir = baml_compiler2_tir::lower_type_expr::lower_type_expr_in_ns(
                    self.db,
                    &imp.for_target.expr,
                    pkg_items,
                    &pkg_info.namespace_path,
                    &imp.generic_params,
                    &mut diags,
                );

                let target_matches_implementor = if imp.generic_params.is_empty() {
                    baml_compiler2_tir::normalize::is_same_normalized_type(
                        &target_ty_tir,
                        impl_ty_tir,
                        &self.resolved_aliases.aliases,
                    )
                } else {
                    baml_compiler2_tir::interfaces::match_ty_pattern(
                        &target_ty_tir,
                        impl_ty_tir,
                        &imp.generic_params,
                        &self.resolved_aliases.aliases,
                    )
                    .is_some()
                };
                if !target_matches_implementor {
                    continue;
                }

                let Some(root_iface_loc) =
                    baml_compiler2_tir::interfaces::resolve_path_to_interface(
                        self.db,
                        &imp.interface_target.expr,
                        pkg_items,
                        &pkg_info.namespace_path,
                    )
                else {
                    continue;
                };
                let root_iface_tree =
                    baml_compiler2_hir::file_item_tree(self.db, root_iface_loc.file(self.db));
                let Some(root_iface_data) =
                    root_iface_tree.interfaces.get(&root_iface_loc.id(self.db))
                else {
                    continue;
                };
                let root_iface_qtn = baml_compiler2_tir::lower_type_expr::qualify_def(
                    self.db,
                    Definition::Interface(root_iface_loc),
                    &root_iface_data.name,
                );
                let root_iface_args_tir = match &imp.interface_target.expr {
                    baml_compiler2_ast::TypeExpr::Path { generic_args, .. } => generic_args
                        .iter()
                        .map(|arg| {
                            baml_compiler2_tir::lower_type_expr::lower_type_expr_in_ns(
                                self.db,
                                arg,
                                pkg_items,
                                &pkg_info.namespace_path,
                                &imp.generic_params,
                                &mut diags,
                            )
                        })
                        .collect::<Vec<_>>(),
                    _ => Vec::new(),
                };
                let root_iface_assoc_tir = lower_interface_target_associated_bindings(
                    self.db,
                    &imp.interface_target.expr,
                    &imp.associated_type_bindings,
                    pkg_items,
                    &pkg_info.namespace_path,
                    &imp.generic_params,
                    &mut diags,
                );
                let bounds = imp
                    .generic_param_bounds
                    .iter()
                    .map(|bound| {
                        bound.as_ref().map(|bound| {
                            baml_compiler2_tir::lower_type_expr::lower_type_expr_in_ns(
                                self.db,
                                bound,
                                pkg_items,
                                &pkg_info.namespace_path,
                                &imp.generic_params,
                                &mut diags,
                            )
                        })
                    })
                    .collect();
                let rule = baml_compiler2_tir::interfaces::InterfaceImplRule {
                    generic_params: imp.generic_params.clone(),
                    generic_param_bounds: bounds,
                    for_ty_pattern: target_ty_tir,
                    interface_ty: Tir2Ty::Interface(
                        root_iface_qtn,
                        root_iface_args_tir,
                        root_iface_assoc_tir,
                        baml_compiler2_tir::ty::TyAttr::default(),
                    ),
                    origin: baml_compiler2_tir::interfaces::InterfaceImplOrigin::OutOfBody,
                };
                let registry = baml_compiler2_tir::interfaces::package_implements_registry(
                    self.db,
                    PackageId::new(self.db, pkg_info.package.clone()),
                );

                for (requested_tn, requested_args, requested_assoc) in &requested_views {
                    let Some(requested_iface_qtn) = self.resolve_qtn_by_type_name(requested_tn)
                    else {
                        continue;
                    };
                    let requested_iface_ty = Tir2Ty::Interface(
                        requested_iface_qtn,
                        requested_args.clone(),
                        requested_assoc.clone(),
                        baml_compiler2_tir::ty::TyAttr::default(),
                    );
                    let instantiation = registry
                        .instantiate_rule_for_requested_interface(
                            &rule,
                            &requested_iface_ty,
                            None,
                            &self.resolved_aliases.aliases,
                            |actual, bound| {
                                type_satisfies_bound(
                                    self.db,
                                    actual,
                                    bound,
                                    &self.resolved_aliases.aliases,
                                    &pkg_info.package,
                                    BLANKET_BOUND_DEPTH,
                                )
                            },
                        )
                        .or_else(|| {
                            let dispatch_iface_ty = Self::interface_dispatch_instantiation_request(
                                &rule.interface_ty,
                                &requested_iface_ty,
                            );
                            registry.instantiate_rule_for_requested_interface(
                                &rule,
                                &dispatch_iface_ty,
                                None,
                                &self.resolved_aliases.aliases,
                                |actual, bound| {
                                    type_satisfies_bound(
                                        self.db,
                                        actual,
                                        bound,
                                        &self.resolved_aliases.aliases,
                                        &pkg_info.package,
                                        BLANKET_BOUND_DEPTH,
                                    )
                                },
                            )
                        });
                    let Some(instantiation) = instantiation else {
                        continue;
                    };
                    let (inst_iface_args, inst_iface_assoc): (&[Tir2Ty], &[(Name, Tir2Ty)]) =
                        match &instantiation.interface_ty {
                            Tir2Ty::Interface(_, args, assoc, _) => (args, assoc),
                            _ => (&[], &[]),
                        };

                    for (current_iface_loc, current_iface_args, current_iface_assoc) in
                        baml_compiler2_tir::interfaces::interface_closure_locs_with_args_and_assoc(
                            self.db,
                            root_iface_loc,
                            inst_iface_args,
                            inst_iface_assoc,
                            pkg_items,
                            &pkg_info.namespace_path,
                        )
                    {
                        let Some(current_iface_tn) =
                            interface_type_name_from_loc(self.db, current_iface_loc)
                        else {
                            continue;
                        };
                        if current_iface_tn != *requested_tn
                            || !self
                                .interface_tir_type_args_match(&current_iface_args, requested_args)
                            || !self
                                .interface_tir_assoc_match(&current_iface_assoc, requested_assoc)
                        {
                            continue;
                        }
                        let iface_tree = baml_compiler2_hir::file_item_tree(
                            self.db,
                            current_iface_loc.file(self.db),
                        );
                        let Some(iface_data) =
                            iface_tree.interfaces.get(&current_iface_loc.id(self.db))
                        else {
                            continue;
                        };
                        let iface_pkg = baml_compiler2_hir::file_package::file_package(
                            self.db,
                            current_iface_loc.file(self.db),
                        );

                        if let Some(method_id) = imp
                            .methods
                            .iter()
                            .find(|method_id| item_tree[**method_id].name == *method)
                        {
                            let func_loc = baml_compiler2_hir::loc::FunctionLoc::new(
                                self.db, file, *method_id,
                            );
                            let frame_type_args = Self::impl_frame_type_args_for_request(
                                &imp.generic_params,
                                &instantiation,
                                &rule.interface_ty,
                                &requested_iface_ty,
                            );
                            return Some((
                                def_to_item_ref(self.db, Definition::Function(func_loc)),
                                frame_type_args,
                            ));
                        }

                        for &fn_id in &iface_data.default_methods {
                            if iface_tree[fn_id].name == *method {
                                return Some((
                                    ItemRef::Method {
                                        package: iface_pkg.package.clone(),
                                        namespace: iface_pkg.namespace_path,
                                        class: iface_data.name.clone(),
                                        name: method.clone(),
                                    },
                                    current_iface_args,
                                ));
                            }
                        }
                    }
                }
            }
        }
        None
    }

    fn resolve_class_loc_by_type_name(
        &self,
        class_tn: &TypeName,
    ) -> Option<baml_compiler2_hir::loc::ClassLoc<'db>> {
        let pkg_name = class_tn.package();
        let pkg_items = self.resolve_class_pkg_items_by_name(pkg_name);
        let ns: Vec<Name> = class_tn.namespace().clone();
        let Some(Definition::Class(class_loc)) = pkg_items.lookup_type(&ns, class_tn.name()) else {
            return None;
        };
        Some(class_loc)
    }

    fn resolve_qtn_by_type_name(&self, tn: &TypeName) -> Option<QualifiedTypeName> {
        let pkg_name = tn.package();
        let pkg_items = self.resolve_class_pkg_items_by_name(pkg_name);
        let ns: Vec<Name> = tn.namespace().clone();
        let def = pkg_items.lookup_type(&ns, tn.name())?;
        match def {
            Definition::Class(_)
            | Definition::Enum(_)
            | Definition::Interface(_)
            | Definition::TypeAlias(_) => Some(baml_compiler2_tir::lower_type_expr::qualify_def(
                self.db,
                def,
                tn.name(),
            )),
            _ => None,
        }
    }

    fn implements_target_matches_requested_views(
        &self,
        target: &baml_compiler2_ast::SpannedTypeExpr,
        associated_type_bindings: &[baml_compiler2_ast::AssociatedTypeBindingDef],
        class_loc: baml_compiler2_hir::loc::ClassLoc<'db>,
        requested_views: &[InterfaceTypeView],
        class_params: &[Name],
    ) -> Vec<(usize, TypeName, InterfaceClassGuard)> {
        let Some((target_tn, target_args, target_assoc)) =
            self.resolve_implements_target_view(target, associated_type_bindings, class_loc)
        else {
            return Vec::new();
        };
        let Some(target_views) =
            self.interface_closure_type_name_views(&target_tn, &target_args, &target_assoc)
        else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for (target_view_tn, target_view_args, target_view_assoc) in target_views {
            for (requested_idx, (requested_tn, requested_args, requested_assoc)) in
                requested_views.iter().enumerate()
            {
                if target_view_tn != *requested_tn {
                    continue;
                }
                let Some(guard) = interface_class_guard_for_args(
                    &target_view_args,
                    &target_view_assoc,
                    requested_args,
                    requested_assoc,
                    class_params,
                    &self.resolved_aliases.aliases,
                ) else {
                    continue;
                };
                out.push((requested_idx, requested_tn.clone(), guard));
            }
        }
        out
    }

    fn interface_default_method_item_ref(
        &self,
        iface_tn: &TypeName,
        method: &Name,
    ) -> Option<ItemRef> {
        let iface_pkg_name = iface_tn.package();
        let iface_pkg_items = self.resolve_class_pkg_items_by_name(iface_pkg_name);
        let iface_ns: Vec<Name> = iface_tn.namespace().clone();
        let Definition::Interface(iface_loc) =
            iface_pkg_items.lookup_type(&iface_ns, iface_tn.name())?
        else {
            return None;
        };
        let iface_tree = baml_compiler2_hir::file_item_tree(self.db, iface_loc.file(self.db));
        let iface_data = iface_tree.interfaces.get(&iface_loc.id(self.db))?;
        if !iface_data
            .default_methods
            .iter()
            .any(|fn_id| iface_tree[*fn_id].name == *method)
        {
            return None;
        }
        let iface_pkg =
            baml_compiler2_hir::file_package::file_package(self.db, iface_loc.file(self.db));
        Some(ItemRef::Method {
            package: iface_pkg.package.clone(),
            namespace: iface_pkg.namespace_path,
            class: iface_data.name.clone(),
            name: method.clone(),
        })
    }

    fn resolve_implements_target_view(
        &self,
        target: &baml_compiler2_ast::SpannedTypeExpr,
        associated_type_bindings: &[baml_compiler2_ast::AssociatedTypeBindingDef],
        class_loc: baml_compiler2_hir::loc::ClassLoc<'db>,
    ) -> Option<InterfaceTypeView> {
        let class_file = class_loc.file(self.db);
        let class_pkg = baml_compiler2_hir::file_package::file_package(self.db, class_file);
        let class_pkg_id = PackageId::new(self.db, class_pkg.package.clone());
        let class_pkg_items = package_items(self.db, class_pkg_id);
        let target_loc = baml_compiler2_tir::interfaces::resolve_path_to_interface(
            self.db,
            &target.expr,
            class_pkg_items,
            &class_pkg.namespace_path,
        )?;
        let target_tree = baml_compiler2_hir::file_item_tree(self.db, target_loc.file(self.db));
        let target_data = target_tree.interfaces.get(&target_loc.id(self.db))?;
        let target_qtn = baml_compiler2_tir::lower_type_expr::qualify_def(
            self.db,
            Definition::Interface(target_loc),
            &target_data.name,
        );
        let item_tree = file_item_tree(self.db, class_file);
        let class_data = &item_tree[class_loc.id(self.db)];
        let mut diags = Vec::new();
        let target_args = match &target.expr {
            baml_compiler2_ast::TypeExpr::Path { generic_args, .. } => generic_args
                .iter()
                .map(|arg| {
                    baml_compiler2_tir::lower_type_expr::lower_type_expr_in_ns(
                        self.db,
                        arg,
                        class_pkg_items,
                        &class_pkg.namespace_path,
                        &class_data.generic_params,
                        &mut diags,
                    )
                })
                .collect(),
            _ => Vec::new(),
        };
        let target_iface_pkg =
            baml_compiler2_hir::file_package::file_package(self.db, target_loc.file(self.db));
        let mut bindings =
            baml_compiler2_tir::generics::bind_type_vars(&target_data.generic_params, &target_args);
        for param in &class_data.generic_params {
            bindings.entry(param.clone()).or_insert_with(|| {
                Tir2Ty::TypeVar(param.clone(), baml_compiler2_tir::ty::TyAttr::default())
            });
        }
        let associated_bindings = target_data
            .associated_types
            .iter()
            .filter_map(|assoc| {
                if let Some(binding) = associated_type_bindings
                    .iter()
                    .find(|binding| binding.name == assoc.name)
                    && let Some(type_expr) = &binding.type_expr
                {
                    let ty = baml_compiler2_tir::generics::lower_type_expr_with_generics(
                        self.db,
                        &type_expr.expr,
                        class_pkg_items,
                        &class_pkg.namespace_path,
                        &bindings,
                        &mut diags,
                    );
                    bindings.insert(assoc.name.clone(), ty.clone());
                    return Some((assoc.name.clone(), ty));
                }
                assoc.default.as_ref().map(|default| {
                    let ty = baml_compiler2_tir::generics::lower_type_expr_with_generics(
                        self.db,
                        &default.expr,
                        class_pkg_items,
                        &target_iface_pkg.namespace_path,
                        &bindings,
                        &mut diags,
                    );
                    bindings.insert(assoc.name.clone(), ty.clone());
                    (assoc.name.clone(), ty)
                })
            })
            .collect();
        Some((target_qtn, target_args, associated_bindings))
    }

    /// True iff the interface named by `iface_tn` declares `field` directly in
    /// its own body (not via `requires`).
    fn interface_declares_field(&self, iface_tn: &TypeName, field: &Name) -> bool {
        let pkg_name = iface_tn.package();
        let pkg_items = self.resolve_class_pkg_items_by_name(pkg_name);
        let ns: Vec<Name> = iface_tn.namespace().clone();
        let Some(Definition::Interface(loc)) = pkg_items.lookup_type(&ns, iface_tn.name()) else {
            return false;
        };
        let tree = file_item_tree(self.db, loc.file(self.db));
        tree.interfaces
            .get(&loc.id(self.db))
            .is_some_and(|data| data.fields.iter().any(|f| &f.name == field))
    }

    /// True iff the interface named by `iface_tn` declares `method` directly
    /// (as a default or required method), not via `requires`.
    fn interface_declares_method(&self, iface_tn: &TypeName, method: &Name) -> bool {
        let pkg_name = iface_tn.package();
        let pkg_items = self.resolve_class_pkg_items_by_name(pkg_name);
        let ns: Vec<Name> = iface_tn.namespace().clone();
        let Some(Definition::Interface(loc)) = pkg_items.lookup_type(&ns, iface_tn.name()) else {
            return false;
        };
        let tree = file_item_tree(self.db, loc.file(self.db));
        tree.interfaces.get(&loc.id(self.db)).is_some_and(|data| {
            data.required_methods.iter().any(|s| s.name == *method)
                || data
                    .default_methods
                    .iter()
                    .any(|&fn_id| tree[fn_id].name == *method)
        })
    }

    /// The interface that "owns" `method` for an `implements <target>` block:
    /// the most-derived interface in the target's requires-closure (root-first)
    /// that declares `method`. A method override or default in that block only
    /// satisfies a request resolving to this view — so `implements B { foo }`
    /// (where `B requires A` and both declare `foo`) provides `B::foo`, never
    /// `A::foo`, even though `A` is reachable through `B`'s closure.
    fn method_provider_view(
        &self,
        target: &baml_compiler2_ast::SpannedTypeExpr,
        associated_type_bindings: &[baml_compiler2_ast::AssociatedTypeBindingDef],
        class_loc: baml_compiler2_hir::loc::ClassLoc<'db>,
        method: &Name,
    ) -> Option<TypeName> {
        let (target_tn, target_args, target_assoc) =
            self.resolve_implements_target_view(target, associated_type_bindings, class_loc)?;
        let views =
            self.interface_closure_type_name_views(&target_tn, &target_args, &target_assoc)?;
        views
            .into_iter()
            .find(|(tn, _, _)| self.interface_declares_method(tn, method))
            .map(|(tn, _, _)| tn)
    }

    /// Class-tag dispatch guards for every implementor that satisfies the
    /// *specific instantiation* `iface_tn<iface_type_args>`. Implementors of a
    /// different instantiation (e.g. `StrSlot: Slot<string>` when the request is
    /// `Slot<int>`) are excluded, because `interface_class_guard_for_args`
    /// returns `None` for a non-matching argument list. Used by the runtime
    /// `is`-type test so a generic-interface pattern respects its type argument.
    fn interface_implementor_class_guards(
        &self,
        iface_tn: &TypeName,
        iface_type_args: &[Tir2Ty],
        iface_assoc: &[(Name, Tir2Ty)],
    ) -> Vec<(TypeName, InterfaceClassGuard)> {
        let Some(impls) = self.interface_implementors.get(iface_tn).cloned() else {
            return Vec::new();
        };
        let Some(requested_views) =
            self.interface_closure_type_name_views(iface_tn, iface_type_args, iface_assoc)
        else {
            return Vec::new();
        };
        let mut out: Vec<(TypeName, InterfaceClassGuard)> = Vec::new();
        for impl_tn in &impls {
            let Some(class_loc) = self.resolve_class_loc_by_type_name(impl_tn) else {
                continue;
            };
            let item_tree = file_item_tree(self.db, class_loc.file(self.db));
            let class_data = &item_tree[class_loc.id(self.db)];
            for impl_block in &class_data.implements {
                let Some((target_tn, target_args, target_assoc)) = self
                    .resolve_implements_target_view(
                        &impl_block.target,
                        &impl_block.associated_type_bindings,
                        class_loc,
                    )
                else {
                    continue;
                };
                let Some(target_views) =
                    self.interface_closure_type_name_views(&target_tn, &target_args, &target_assoc)
                else {
                    continue;
                };
                for (target_view_tn, target_view_args, target_view_assoc) in target_views {
                    for (requested_tn, requested_args, requested_assoc) in &requested_views {
                        if target_view_tn != *requested_tn {
                            continue;
                        }
                        let Some(guard) = interface_class_guard_for_args(
                            &target_view_args,
                            &target_view_assoc,
                            requested_args,
                            requested_assoc,
                            &class_data.generic_params,
                            &self.resolved_aliases.aliases,
                        ) else {
                            continue;
                        };
                        // Push every matching guard — a generic class can satisfy
                        // the requested instantiation through more than one
                        // type-arg projection (`Pair<L, R>` implementing
                        // `Slot<L>` and `Slot<R>`), and dropping all but the first
                        // would make some runtime values fail the `is` test.
                        // Redundant identical branches are harmless (both succeed).
                        out.push((impl_tn.clone(), guard));
                    }
                }
            }
        }
        out
    }

    fn resolve_implementor_interface_field_candidates(
        &self,
        impl_tn: &TypeName,
        iface_tn: &TypeName,
        iface_type_args: &[Tir2Ty],
        iface_assoc: &[(Name, Tir2Ty)],
        field: &Name,
    ) -> Vec<InterfaceFieldCandidate> {
        let Some(class_loc) = self.resolve_class_loc_by_type_name(impl_tn) else {
            return Vec::new();
        };
        let item_tree = file_item_tree(self.db, class_loc.file(self.db));
        let class_data = &item_tree[class_loc.id(self.db)];
        let Some(requested_views) =
            self.interface_closure_type_name_views(iface_tn, iface_type_args, iface_assoc)
        else {
            return Vec::new();
        };
        // BEP-044: when the requested interface and a `requires` parent both
        // declare `field`, `.as<Requested>.field` must use the *most-derived*
        // declaration. The closure is root-first, so resolve the field against
        // the first view that declares it and ignore the others — otherwise an
        // inherited parent view (e.g. `A` behind `B requires A`) could win by
        // impl-block ordering and read the wrong class field.
        let owning_view_tn: Option<TypeName> = requested_views
            .iter()
            .find(|(tn, _, _)| self.interface_declares_field(tn, field))
            .map(|(tn, _, _)| tn.clone());
        let mut out = Vec::new();

        for impl_block in &class_data.implements {
            let Some((target_tn, target_args, target_assoc)) = self.resolve_implements_target_view(
                &impl_block.target,
                &impl_block.associated_type_bindings,
                class_loc,
            ) else {
                continue;
            };
            let Some(target_views) =
                self.interface_closure_type_name_views(&target_tn, &target_args, &target_assoc)
            else {
                continue;
            };

            for (target_view_tn, target_view_args, target_view_assoc) in target_views {
                for (requested_tn, requested_args, requested_assoc) in &requested_views {
                    if target_view_tn != *requested_tn {
                        continue;
                    }
                    // Restrict to the field's owning (most-derived) view.
                    if let Some(owning) = &owning_view_tn
                        && requested_tn != owning
                    {
                        continue;
                    }
                    let Some(guard) = interface_class_guard_for_args(
                        &target_view_args,
                        &target_view_assoc,
                        requested_args,
                        requested_assoc,
                        &class_data.generic_params,
                        &self.resolved_aliases.aliases,
                    ) else {
                        continue;
                    };
                    let class_field = impl_block
                        .field_links
                        .iter()
                        .find(|link| &link.interface_field == field)
                        .map(|link| link.class_field.clone())
                        .unwrap_or_else(|| field.clone());
                    if let Some(field_idx) = self
                        .class_fields
                        .get(impl_tn)
                        .and_then(|fields| fields.get(class_field.as_str()))
                        .copied()
                    {
                        out.push(InterfaceFieldCandidate {
                            impl_tn: impl_tn.clone(),
                            guard,
                            field_idx,
                        });
                    }
                }
            }
        }

        out
    }

    fn emit_interface_class_guard_branch(
        &mut self,
        recv_local: Local,
        impl_tn: &TypeName,
        guard: &InterfaceClassGuard,
        success: BlockId,
        failure: BlockId,
    ) {
        let ty_template = match guard {
            InterfaceClassGuard::Any => {
                TyTemplate::Concrete(Ty::Class(impl_tn.clone(), Vec::new(), TyAttr::default()))
            }
            InterfaceClassGuard::Exact(args) => {
                let generic_params = self.enclosing_generic_params();
                TyTemplate::Class(
                    impl_tn.clone(),
                    args.iter()
                        .map(|arg| match arg {
                            Some(arg) => tir2_to_dispatch_guard_template(
                                arg,
                                self.resolved_aliases,
                                &generic_params,
                            ),
                            None => TyTemplate::Wildcard,
                        })
                        .collect(),
                )
            }
        };
        let test_local = self.builder.temp(Ty::Bool {
            attr: TyAttr::default(),
        });
        self.builder.assign(
            Place::local(test_local),
            Rvalue::IsType {
                operand: Operand::Copy(Place::Local(recv_local)),
                ty_template,
            },
        );
        self.builder
            .branch(Operand::Copy(Place::Local(test_local)), success, failure);
    }

    fn emit_interface_dispatch_guard_branch(
        &mut self,
        recv_local: Local,
        guard: &InterfaceDispatchGuard,
        success: BlockId,
        failure: BlockId,
    ) {
        match guard {
            InterfaceDispatchGuard::Class { impl_tn, guard } => {
                self.emit_interface_class_guard_branch(
                    recv_local, impl_tn, guard, success, failure,
                );
            }
            InterfaceDispatchGuard::Type(ty) => {
                self.emit_is_type_branch(recv_local, ty.clone(), success, failure);
            }
        }
    }

    /// BEP-044: when the enclosing function is the override declared
    /// inside an `implements I { ... }` block, return `I`'s target type
    /// expression. `None` for free functions, top-level class methods,
    /// and interface default-method bodies.
    fn implements_block_iface_target(&self) -> Option<baml_compiler2_ast::SpannedTypeExpr> {
        let func_loc = self.func_loc?;
        let item_tree = file_item_tree(self.db, func_loc.file(self.db));
        item_tree
            .method_to_iface_target
            .get(&func_loc.id(self.db))
            .cloned()
    }

    fn resolve_class_pkg_items_by_name(
        &self,
        pkg_name: &Name,
    ) -> &'db baml_compiler2_hir::package::PackageItems<'db> {
        let pkg_id = PackageId::new(self.db, pkg_name.clone());
        package_items(self.db, pkg_id)
    }

    fn lower_watch_method(
        &mut self,
        _expr_id: AstExprId,
        base: AstExprId,
        method: &Name,
        args: &[AstExprId],
        dest: Place,
    ) {
        // Find the watched local from the base expression
        let base_op = self.lower_to_operand(base);
        let (Operand::Copy(Place::Local(local)) | Operand::Move(Place::Local(local))) = base_op
        else {
            // Not a direct local — fall back to regular call lowering
            // (shouldn't happen in well-formed code)
            self.builder
                .assign(dest, Rvalue::Use(Operand::Constant(Constant::Null)));
            return;
        };

        if method.as_str() == "options" {
            // $watch.options(filter) — emit WatchOptions statement
            if let Some(&filter_expr) = args.first() {
                let filter_op = self.lower_to_operand(filter_expr);
                self.builder.watch_options(local, filter_op);
            }
            self.builder
                .assign(dest, Rvalue::Use(Operand::Constant(Constant::Null)));
        } else if method.as_str() == "notify" {
            // $watch.notify() — emit WatchNotify statement
            self.builder.watch_notify(local);
            self.builder
                .assign(dest, Rvalue::Use(Operand::Constant(Constant::Null)));
        } else {
            self.builder
                .assign(dest, Rvalue::Use(Operand::Constant(Constant::Null)));
        }
    }
}

// ─── Statement lowering ───────────────────────────────────────────────────────

impl LoweringContext<'_> {
    fn lower_stmt(&mut self, stmt_id: AstStmtId) {
        let prev_span = self.builder.current_source_span;
        if let Some(span) = self.span_for_stmt(stmt_id) {
            self.builder.current_source_span = Some(span);
        }

        let stmt = self.body.stmts[stmt_id].clone();
        match stmt {
            AstStmt::Expr(expr_id) => {
                let ty = self.expr_ty(expr_id);
                let temp = self.builder.temp(ty);
                self.lower_expr(expr_id, Place::local(temp));
            }

            // `let PATTERN = init else { … };` — refutable binding lowered
            // as a two-way pattern test. On match: bind into the current
            // scope (locals survive past the statement); on miss: lower the
            // else expression (guaranteed `Ty::Never` by TIR, so no
            // successor edge is needed). Handled before the structural
            // arms below because a refutable destructure ends up here too.
            AstStmt::Let {
                pattern,
                initializer,
                is_watched,
                else_branch: Some(else_expr),
                ..
            } => {
                // Materialize the scrutinee into a local once. The
                // scrutinee carries the BROAD initializer type (e.g.
                // `int | string`), not the pattern's narrowed match type —
                // narrowing only kicks in on the match arm, after the
                // refutable test.
                let scrutinee_ty = initializer
                    .map(|init| self.expr_ty(init))
                    .unwrap_or_else(|| self.pat_ty(pattern));
                let scrutinee = self.builder.temp(scrutinee_ty);
                if let Some(init) = initializer {
                    self.lower_expr(init, Place::local(scrutinee));
                } else {
                    self.builder.assign(
                        Place::local(scrutinee),
                        Rvalue::Use(Operand::Constant(Constant::Null)),
                    );
                }

                let bb_match = self.builder.create_block();
                let bb_fail = self.builder.create_block();
                self.lower_pattern_test(scrutinee, pattern, bb_match, bb_fail);

                // Fail path: lower the else expression. TIR enforced that
                // the else has type `!`, so this block has no successor and
                // we don't emit a join edge. Use a throwaway temp as dest
                // because diverging expressions don't write through it.
                self.builder.set_current_block(bb_fail);
                let else_ty = self.expr_ty(else_expr);
                let else_dest = self.builder.temp(else_ty);
                self.lower_expr(else_expr, Place::local(else_dest));
                // If for any reason lowering produced a fall-through (e.g.
                // recovery state with an Unknown-typed else), terminate the
                // block with an unreachable so the CFG stays valid.
                if !self.builder.is_current_terminated() {
                    self.builder.unreachable();
                }

                // Match path: bind pattern names into the enclosing scope.
                // No saved/restored locals — these flow forward like a
                // plain `let`.
                self.builder.set_current_block(bb_match);
                self.bind_pattern_inner(scrutinee, pattern, pattern, pattern, false, is_watched);

                let names: Vec<Name> = self.body.patterns[pattern]
                    .bound_names(&self.body.patterns)
                    .into_iter()
                    .cloned()
                    .collect();
                for name in names {
                    if let Some(&local) = self.locals.get(&name)
                        && let Some(binding_id) =
                            self.binding_id_for_statement_name(stmt_id, pattern, &name)
                    {
                        self.binding_locals.insert(binding_id, local);
                    }
                }

                if is_watched {
                    for name in self.body.patterns[pattern].bound_names(&self.body.patterns) {
                        if let Some(&local) = self.locals.get(name) {
                            self.watched_locals_stack.push(local);
                        }
                    }
                }
            }

            AstStmt::Let {
                pattern,
                initializer,
                is_watched,
                ..
            } if self.pattern_contains_structural(pattern) => {
                let local_ty = self.pat_ty(pattern);
                let scrutinee = self.builder.temp(local_ty);

                if let Some(init) = initializer {
                    self.lower_expr(init, Place::local(scrutinee));
                } else {
                    self.builder.assign(
                        Place::local(scrutinee),
                        Rvalue::Use(Operand::Constant(Constant::Null)),
                    );
                }

                self.bind_pattern_inner(scrutinee, pattern, pattern, pattern, false, is_watched);

                let names: Vec<Name> = self.body.patterns[pattern]
                    .bound_names(&self.body.patterns)
                    .into_iter()
                    .cloned()
                    .collect();
                for name in names {
                    if let Some(&local) = self.locals.get(&name)
                        && let Some(binding_id) =
                            self.binding_id_for_statement_name(stmt_id, pattern, &name)
                    {
                        self.binding_locals.insert(binding_id, local);
                    }
                }

                if is_watched {
                    for name in self.body.patterns[pattern].bound_names(&self.body.patterns) {
                        if let Some(&local) = self.locals.get(name) {
                            self.watched_locals_stack.push(local);
                        }
                    }
                }
            }

            AstStmt::Let {
                pattern,
                initializer,
                is_watched,
                ..
            } => {
                // Extract binding names from pattern. A simple `let x` has
                // one name; a chain `let x: let y: let z` has three. The
                // first name owns the declared slot (the init writes into
                // it directly); remaining names alias via copy-assignment.
                let pat = self.body.patterns[pattern].clone();
                let names: Vec<Name> = pat
                    .bound_names(&self.body.patterns)
                    .into_iter()
                    .cloned()
                    .collect();
                let first_name = names.first().cloned();

                let local_ty = self.pat_ty(pattern);
                let local = self.builder.declare_local(
                    first_name.clone(),
                    local_ty.clone(),
                    None,
                    is_watched,
                );

                if let Some(init) = initializer {
                    self.lower_expr(init, Place::local(local));
                } else {
                    self.builder.assign(
                        Place::local(local),
                        Rvalue::Use(Operand::Constant(Constant::Null)),
                    );
                }

                if let Some(first_name) = first_name {
                    if let Some(binding_id) =
                        self.binding_id_for_statement_name(stmt_id, pattern, &first_name)
                    {
                        self.binding_locals.insert(binding_id, local);
                    }
                    self.locals.insert(first_name, local);
                }

                // Additional chain-link bindings get their own locals that
                // copy from the first. `let x: let y` ⇒ y = x at runtime.
                for extra in names.iter().skip(1) {
                    let alias = self.builder.declare_local(
                        Some(extra.clone()),
                        local_ty.clone(),
                        None,
                        false,
                    );
                    self.builder.assign(
                        Place::local(alias),
                        Rvalue::Use(Operand::Copy(Place::Local(local))),
                    );
                    if let Some(binding_id) =
                        self.binding_id_for_statement_name(stmt_id, pattern, extra)
                    {
                        self.binding_locals.insert(binding_id, alias);
                    }
                    self.locals.insert(extra.clone(), alias);
                }

                if is_watched {
                    self.watched_locals_stack.push(local);
                }
            }

            AstStmt::While {
                condition,
                body,
                after,
                ..
            } => {
                let bb_cond = self.builder.create_block();
                let bb_body = self.builder.create_block();
                let bb_after = if after.is_some() {
                    self.builder.create_block()
                } else {
                    bb_cond
                };
                let bb_exit = self.builder.create_block();

                let prev_loop = self.loop_context.take();
                let watched_depth = self.watched_locals_stack.len();
                self.loop_context = Some(LoopContext {
                    break_target: bb_exit,
                    continue_target: bb_after,
                    watched_locals_depth: watched_depth,
                });

                if !self.builder.is_current_terminated() {
                    self.builder.goto(bb_cond);
                }

                self.builder.set_current_block(bb_cond);
                let cond_op = self.lower_to_operand(condition);
                self.builder.branch(cond_op, bb_body, bb_exit);

                self.builder.set_current_block(bb_body);
                let body_ty = self.expr_ty(body);
                let body_temp = self.builder.temp(body_ty);
                self.lower_expr(body, Place::local(body_temp));

                if !self.builder.is_current_terminated() {
                    self.builder.goto(bb_after);
                }

                if after.is_some() {
                    self.builder.set_current_block(bb_after);
                }
                if let Some(after_stmt) = after {
                    self.lower_stmt(after_stmt);
                }

                if !self.builder.is_current_terminated() {
                    self.builder.goto(bb_cond);
                }

                self.loop_context = prev_loop;
                self.builder.set_current_block(bb_exit);
            }

            // `while let PATTERN = SCRUTINEE { BODY }` — a loop header that
            // re-evaluates the scrutinee and re-tests the refutable pattern each
            // iteration. A structural cross of `AstStmt::While` (loop scaffold +
            // LoopContext + back-edge) and `lower_if_let` (refutable
            // `lower_pattern_test` + scoped fresh-cell binding). On match: bind
            // + run body + jump back to the header. On miss: exit the loop.
            AstStmt::WhileLet {
                pattern,
                scrutinee,
                body,
            } => {
                let bb_header = self.builder.create_block();
                let bb_body = self.builder.create_block();
                let bb_exit = self.builder.create_block();

                // `continue` re-enters the header (re-evaluates scrutinee +
                // re-tests the pattern); `break` jumps to the exit. Save/swap/
                // restore loop_context so nested loops work — mirrors While.
                let prev_loop = self.loop_context.take();
                let watched_depth = self.watched_locals_stack.len();
                self.loop_context = Some(LoopContext {
                    break_target: bb_exit,
                    continue_target: bb_header,
                    watched_locals_depth: watched_depth,
                });

                if !self.builder.is_current_terminated() {
                    self.builder.goto(bb_header);
                }

                // Header: resolve the scrutinee to a local, then run the
                // refutable test (match -> body, miss -> exit). For a bare-path
                // scrutinee this resolves to its OWN local, so a body that
                // mutates that local is observed when the header re-tests it on
                // the next pass (re-evaluation without a copy); other expressions
                // are re-lowered into a fresh local each pass. Mirrors
                // `lower_if_let`'s scrutinee handling exactly.
                self.builder.set_current_block(bb_header);
                let scrutinee_local = self.try_resolve_to_local(scrutinee).unwrap_or_else(|| {
                    let op = self.lower_to_operand(scrutinee);
                    let ty = self.expr_ty(scrutinee);
                    self.operand_to_local(op, ty)
                });
                self.lower_pattern_test(scrutinee_local, pattern, bb_body, bb_exit);

                // Body: bind pattern locals (scoped to the body, re-bound per
                // iteration via fresh cells so a closure created each pass
                // captures a distinct cell), record binding_locals for
                // go-to-definition, lower the body (result discarded), then jump
                // back to the header.
                self.builder.set_current_block(bb_body);
                let saved_locals = self.locals.clone();
                self.bind_pattern_with_fresh_cells(scrutinee_local, pattern);
                let names: Vec<Name> = self.body.patterns[pattern]
                    .bound_names(&self.body.patterns)
                    .into_iter()
                    .cloned()
                    .collect();
                for name in names {
                    if let Some(&local) = self.locals.get(&name)
                        && let Some(binding_id) =
                            self.binding_id_for_statement_name(stmt_id, pattern, &name)
                    {
                        self.binding_locals.insert(binding_id, local);
                    }
                }
                let body_temp = self.builder.temp(Ty::Void {
                    attr: TyAttr::default(),
                });
                self.lower_expr(body, Place::local(body_temp));
                if !self.builder.is_current_terminated() {
                    self.emit_unwatch_to_depth(watched_depth);
                    self.builder.goto(bb_header);
                }
                self.restore_locals_after_scope(saved_locals, watched_depth);

                // Exit.
                self.loop_context = prev_loop;
                self.builder.set_current_block(bb_exit);
            }

            // For loops use the Iterable interface: evaluate the collection,
            // call iter(), then repeatedly call next() until Done.
            AstStmt::For {
                binding,
                collection,
                body,
            } => {
                let coll_tir_ty = self
                    .expr_types
                    .get(&self.expr_metadata_key(collection))
                    .cloned();
                let iterable_view = coll_tir_ty
                    .as_ref()
                    .and_then(|ty| self.iterable_view_for_tir_ty(ty));

                if let Some(iterable_view) = iterable_view {
                    self.lower_iterable_for_loop(stmt_id, binding, collection, body, iterable_view);
                } else {
                    self.emit_panic_call("for loop collection is not iterable", collection);
                }
            }

            AstStmt::Return(expr) => {
                let ret = Local(0); // _0 is always the return place
                if let Some(e) = expr {
                    self.lower_expr(e, Place::local(ret));
                }
                // Unwatch all watched locals in this function (the stack is
                // swapped at lambda boundaries, so depth=0 covers exactly the
                // current function's watches).
                self.emit_unwatch_to_depth(0);
                self.builder.goto(self.exit_block);
                // Create a dead successor block for the builder cursor
                // (subsequent statements in the same block-list are dead code)
                let dead = self.builder.create_block();
                self.builder.set_current_block(dead);
                // Dead block is unterminated — subsequent stmts are lowered as
                // dead code (matching AstStmt::Throw behavior at lower.rs:1653-1658).
                // Phase 1 eliminates unreachable blocks.
            }

            AstStmt::Throw { value } => {
                let val_op = self.lower_to_operand(value);
                // Unwatch all watched locals in this function before throwing,
                // matching the Return path. Without this, a
                // `watch let conn = …` followed by a `throw` leaks the watcher.
                self.emit_unwatch_to_depth(0);
                self.builder.throw(val_op);
                let dead = self.builder.create_block();
                self.builder.set_current_block(dead);
            }

            AstStmt::Break => {
                if let Some(ref loop_ctx) = self.loop_context {
                    let target = loop_ctx.break_target;
                    let depth = loop_ctx.watched_locals_depth;
                    self.emit_unwatch_to_depth(depth);
                    self.builder.goto(target);
                }
                let dead = self.builder.create_block();
                self.builder.set_current_block(dead);
            }

            AstStmt::Continue => {
                if let Some(ref loop_ctx) = self.loop_context {
                    let target = loop_ctx.continue_target;
                    let depth = loop_ctx.watched_locals_depth;
                    self.emit_unwatch_to_depth(depth);
                    self.builder.goto(target);
                }
                let dead = self.builder.create_block();
                self.builder.set_current_block(dead);
            }

            AstStmt::Assign { target, value } => {
                let target_expr = &self.body.exprs[target];
                if let AstExpr::OptionalChain { expr: inner } = target_expr {
                    let inner = *inner;
                    self.lower_assign_optional_chain(inner, value);
                } else {
                    let place = self.lower_lvalue(target);
                    self.lower_expr(value, place);
                }
            }

            AstStmt::AssignOp { target, op, value } => {
                let target_expr = &self.body.exprs[target];
                if let AstExpr::OptionalChain { expr: inner } = target_expr {
                    let inner = *inner;
                    self.lower_assign_op_optional_chain(inner, op, value);
                } else {
                    let place = self.lower_lvalue(target);
                    let current = Operand::Copy(place.clone());
                    // Mixed `bigint OP= int` does NOT widen the int rhs: the
                    // specialized `*Bigint` opcodes accept a lone `int` operand
                    // and resolve it in the VM without allocating a heap bigint.
                    // Lower the value naturally.
                    let rhs = self.lower_to_operand(value);
                    let mir_op = Self::convert_assign_op(op);
                    self.builder.assign(
                        place,
                        Rvalue::BinaryOp {
                            op: mir_op,
                            left: current,
                            right: rhs,
                        },
                    );
                }
            }

            AstStmt::Missing => {
                let callee = Operand::Constant(Constant::Function(ItemRef::Free {
                    package: Name::new("baml"),
                    namespace: vec![Name::new("sys")],
                    name: Name::new("panic"),
                }));
                let msg = Operand::Constant(Constant::String("missing statement".to_string()));
                let temp = self.builder.temp(Ty::Null {
                    attr: TyAttr::default(),
                });
                let unreachable_block = self.builder.create_block();
                self.builder.call(
                    callee,
                    vec![msg],
                    Place::local(temp),
                    unreachable_block,
                    None,
                );
                self.builder.set_current_block(unreachable_block);
                self.builder.unreachable();
                let dead = self.builder.create_block();
                self.builder.set_current_block(dead);
            }

            AstStmt::HeaderComment { name, level } => {
                self.builder
                    .push_statement(StatementKind::NotifyBlock { name, level }, None);
            }
        }

        self.builder.current_source_span = prev_span;
    }

    fn convert_assign_op(op: AstAssignOp) -> BinOp {
        match op {
            AstAssignOp::Add => BinOp::Add,
            AstAssignOp::Sub => BinOp::Sub,
            AstAssignOp::Mul => BinOp::Mul,
            AstAssignOp::Div => BinOp::Div,
            AstAssignOp::Mod => BinOp::Mod,
            AstAssignOp::BitAnd => BinOp::BitAnd,
            AstAssignOp::BitOr => BinOp::BitOr,
            AstAssignOp::BitXor => BinOp::BitXor,
            AstAssignOp::Shl => BinOp::Shl,
            AstAssignOp::Shr => BinOp::Shr,
        }
    }

    fn lower_lvalue(&mut self, expr_id: AstExprId) -> Place {
        let expr = self.body.exprs[expr_id].clone();
        match &expr {
            AstExpr::Path(segments) if segments.len() == 1 => {
                if let Some(&local) = self.locals.get(&segments[0]) {
                    Place::Local(local)
                } else if let Some(cap_idx) = self.capture_index_for_name_at(expr_id, &segments[0])
                {
                    // Assignment to a captured variable in a closure body.
                    Place::Capture(cap_idx)
                } else {
                    let temp = self.builder.temp(Ty::Null {
                        attr: TyAttr::default(),
                    });
                    Place::Local(temp)
                }
            }
            AstExpr::Path(segments) if segments.len() >= 2 => {
                // Multi-segment path lvalue: `a.b` or `a.b.c`.
                // Chain field projections from the root local or capture.
                let (mut current_place, mut current_ty) = if let Some(&l) =
                    self.locals.get(&segments[0])
                {
                    let ty = self
                        .path_root_ty(expr_id)
                        .unwrap_or_else(|| self.builder.local_ty(l));
                    (Place::Local(l), ty)
                } else if let Some(cap_idx) = self.capture_index_for_name_at(expr_id, &segments[0])
                {
                    let ty = self
                        .path_root_ty(expr_id)
                        .unwrap_or_else(|| Ty::BuiltinUnknown {
                            attr: TyAttr::default(),
                        });
                    (Place::Capture(cap_idx), ty)
                } else {
                    let tmp = self.builder.temp(Ty::Null {
                        attr: TyAttr::default(),
                    });
                    (
                        Place::Local(tmp),
                        Ty::Null {
                            attr: TyAttr::default(),
                        },
                    )
                };

                for (offset, seg) in segments[1..].iter().enumerate() {
                    let seg_idx = offset + 1;
                    if let Some((tn, class_type_args)) =
                        self.class_receiver_for_path_prefix(expr_id, seg_idx - 1, &current_ty)
                    {
                        if let Some(fields) = self.class_fields.get(&tn) {
                            if let Some(&idx) = fields.get(seg.as_str()) {
                                let next_ty = self.class_field_ty(&tn, seg, &class_type_args);
                                current_place = Place::Field {
                                    base: Box::new(current_place),
                                    field: idx,
                                };
                                current_ty = next_ty;
                                continue;
                            }
                        }
                    }
                    // Dynamic map fallback for non-class base or unknown field
                    let key_local = self.builder.temp(Ty::String {
                        attr: TyAttr::default(),
                    });
                    self.builder.assign(
                        Place::local(key_local),
                        Rvalue::Use(Operand::Constant(Constant::String(seg.to_string()))),
                    );
                    current_place = Place::Index {
                        base: Box::new(current_place),
                        index: key_local,
                        kind: IndexKind::Map,
                    };
                    break;
                }
                current_place
            }
            AstExpr::MemberAccess { base, member } => {
                let base_id = *base;
                let member_name = member.clone();
                let base_place = self.lower_lvalue(base_id);
                let base_ty = self.expr_ty(base_id);
                if let Ty::Class(ref tn, _, _) = base_ty {
                    if let Some(fields) = self.class_fields.get(tn) {
                        if let Some(&idx) = fields.get(member_name.as_str()) {
                            return Place::Field {
                                base: Box::new(base_place),
                                field: idx,
                            };
                        }
                    }
                    self.emit_panic_call(
                        &format!(
                            "internal compiler error: MIR failed to resolve member access \
                             .{} against class definition '{}' (module_path: {:?}). \
                             This class should be in class_fields but isn't.",
                            member_name,
                            tn.name(),
                            tn.module_path(),
                        ),
                        base_id,
                    );
                    // Dead code after panic — return a dummy place
                    let dead = self.builder.temp(Ty::Null {
                        attr: TyAttr::default(),
                    });
                    return Place::Local(dead);
                }
                // Dynamic map access — only valid for map types, unknown, etc.
                let key_local = self.builder.temp(Ty::String {
                    attr: TyAttr::default(),
                });
                self.builder.assign(
                    Place::local(key_local),
                    Rvalue::Use(Operand::Constant(Constant::String(member_name.to_string()))),
                );
                Place::Index {
                    base: Box::new(base_place),
                    index: key_local,
                    kind: IndexKind::Map,
                }
            }
            AstExpr::Index { base, index } => {
                let base_id = *base;
                let index_id = *index;
                let base_place = self.lower_lvalue(base_id);
                let index_op = self.lower_to_operand(index_id);
                let base_ty = self.expr_ty(base_id);
                let index_ty = self.expr_ty(index_id);
                let index_local = self.operand_to_local(index_op, index_ty);
                let unwrapped_ty = base_ty.strip_null();
                let kind = if matches!(&unwrapped_ty, Ty::List(..) | Ty::Uint8Array { .. }) {
                    IndexKind::Array
                } else {
                    IndexKind::Map
                };
                Place::Index {
                    base: Box::new(base_place),
                    index: index_local,
                    kind,
                }
            }
            AstExpr::OptionalMemberAccess { base, member } => {
                let base_id = *base;
                let member_name = member.clone();

                // Evaluate base once into a temp local
                let base_op = self.lower_to_operand(base_id);
                let base_ty = self.expr_ty(base_id);
                let base_local = self.operand_to_local(base_op, base_ty.clone());

                // Null check using the operand
                let is_null = Rvalue::BinaryOp {
                    op: BinOp::Eq,
                    left: Operand::Copy(Place::Local(base_local)),
                    right: Operand::Constant(Constant::Null),
                };
                let test_local = self.builder.temp(Ty::Bool {
                    attr: TyAttr::default(),
                });
                self.builder.assign(Place::local(test_local), is_null);

                let bb_continue = self.builder.create_block();
                let bb_null = *self
                    .chain_null_exits
                    .last()
                    .expect("OptionalMemberAccess in lvalue must be inside OptionalChain");
                self.builder.branch(
                    Operand::Copy(Place::Local(test_local)),
                    bb_null,
                    bb_continue,
                );

                self.builder.set_current_block(bb_continue);

                // Project member from the same temp local — no second evaluation
                let base_place = Place::Local(base_local);
                // Unwrap Optional — we've already null-checked, so use the inner type.
                let unwrapped_ty = base_ty.strip_null();
                if let Ty::Class(tn, _, _) = &unwrapped_ty {
                    if let Some(fields) = self.class_fields.get(tn) {
                        if let Some(&idx) = fields.get(member_name.as_str()) {
                            return Place::Field {
                                base: Box::new(base_place),
                                field: idx,
                            };
                        }
                    }
                }
                // Dynamic map access
                let key_local = self.builder.temp(Ty::String {
                    attr: TyAttr::default(),
                });
                self.builder.assign(
                    Place::local(key_local),
                    Rvalue::Use(Operand::Constant(Constant::String(member_name.to_string()))),
                );
                Place::Index {
                    base: Box::new(base_place),
                    index: key_local,
                    kind: IndexKind::Map,
                }
            }
            AstExpr::OptionalIndex { base, index } => {
                let base_id = *base;
                let index_id = *index;

                // Evaluate base once into a temp local
                let base_op = self.lower_to_operand(base_id);
                let base_ty = self.expr_ty(base_id);
                let base_local = self.operand_to_local(base_op, base_ty.clone());

                // Null check
                let is_null = Rvalue::BinaryOp {
                    op: BinOp::Eq,
                    left: Operand::Copy(Place::Local(base_local)),
                    right: Operand::Constant(Constant::Null),
                };
                let test_local = self.builder.temp(Ty::Bool {
                    attr: TyAttr::default(),
                });
                self.builder.assign(Place::local(test_local), is_null);

                let bb_continue = self.builder.create_block();
                let bb_null = *self
                    .chain_null_exits
                    .last()
                    .expect("OptionalIndex in lvalue must be inside OptionalChain");
                self.builder.branch(
                    Operand::Copy(Place::Local(test_local)),
                    bb_null,
                    bb_continue,
                );

                self.builder.set_current_block(bb_continue);

                // Project index from the same temp local
                let index_op = self.lower_to_operand(index_id);
                let index_ty = self.expr_ty(index_id);
                let index_local = self.operand_to_local(index_op, index_ty);
                let unwrapped_ty = base_ty.strip_null();
                let kind = if matches!(&unwrapped_ty, Ty::List(..) | Ty::Uint8Array { .. }) {
                    IndexKind::Array
                } else {
                    IndexKind::Map
                };
                Place::Index {
                    base: Box::new(Place::Local(base_local)),
                    index: index_local,
                    kind,
                }
            }
            _ => {
                let ty = self.expr_ty(expr_id);
                let temp = self.builder.temp(ty);
                Place::Local(temp)
            }
        }
    }
}

// ─── Match lowering ───────────────────────────────────────────────────────────

impl LoweringContext<'_> {
    fn lower_match(
        &mut self,
        expr_id: AstExprId,
        scrutinee: AstExprId,
        arm_ids: &[baml_compiler2_ast::MatchArmId],
        dest: Place,
    ) {
        let is_exhaustive = self
            .exhaustive_matches
            .contains(&self.expr_metadata_key(expr_id));

        // If scrutinee is a simple variable reference, reuse the local directly
        // instead of copying into a temp (matches MIR1 behavior).
        let scrutinee_local = self.try_resolve_to_local(scrutinee).unwrap_or_else(|| {
            let op = self.lower_to_operand(scrutinee);
            let ty = self.expr_ty(scrutinee);
            self.operand_to_local(op, ty)
        });

        let bb_join = self.builder.create_block();

        // Collect arms from arena
        let arms: Vec<baml_compiler2_ast::MatchArm> = arm_ids
            .iter()
            .map(|&id| self.body.match_arms[id].clone())
            .collect();

        // Try switch optimization: if all non-wildcard arms have compatible patterns
        // (int literal, enum variant, or type tag) with no guards, emit a Switch.
        let switch_arms: Vec<(AstPatId, AstExprId, Option<AstExprId>)> = arms
            .iter()
            .map(|arm| (arm.pattern, arm.body, arm.guard))
            .collect();
        if self.try_lower_as_switch(
            scrutinee_local,
            &switch_arms,
            dest.clone(),
            bb_join,
            SwitchOtherwise::Match { is_exhaustive },
            None,
        ) {
            self.builder.set_current_block(bb_join);
            return;
        }

        self.lower_match_chain(scrutinee_local, &arms, dest, bb_join, is_exhaustive);

        self.builder.set_current_block(bb_join);
    }

    /// Attempt to lower a match or catch as a Switch terminator.
    /// Returns true if successful, false if the arms aren't switch-eligible.
    ///
    /// Unified entry point for both match and catch switch dispatch.
    /// - `arms`: `(pattern, body_expr, optional_guard)` tuples
    /// - `otherwise`: controls what happens for unmatched values
    /// - `pre_created_blocks`: if `Some`, use these pre-created body blocks instead
    ///   of creating new ones (used by catch, which pre-creates blocks)
    fn try_lower_as_switch(
        &mut self,
        scrutinee: Local,
        arms: &[(AstPatId, AstExprId, Option<AstExprId>)],
        dest: Place,
        join: BlockId,
        otherwise: SwitchOtherwise,
        pre_created_blocks: Option<&[Option<BlockId>]>,
    ) -> bool {
        use std::collections::HashSet;

        if arms.is_empty() {
            return false;
        }

        let is_exhaustive = matches!(
            &otherwise,
            SwitchOtherwise::Match {
                is_exhaustive: true
            }
        );

        // Classify arms: collect (i64_value, arm_index) for int literal or enum variant
        // patterns, and check for a trailing wildcard/binding.
        let mut switch_kind: Option<SwitchKind> = None;
        let mut int_arms: Vec<(i64, usize)> = Vec::new();
        let mut otherwise_idx: Option<usize> = None;
        // Deduplicate discriminant values so union patterns don't produce duplicate switch arms.
        let mut seen_values: HashSet<i64> = HashSet::new();

        for (i, &(pattern, _body, guard)) in arms.iter().enumerate() {
            // Guards disqualify switch optimization
            if guard.is_some() {
                return false;
            }
            // OLD `pat.narrow.is_some()` branch: Chain encodes the narrow
            // as a `Type` link, so recover it and treat as a TypeTag arm.
            if self.pattern_narrow_type(pattern).is_some() {
                match &switch_kind {
                    None => switch_kind = Some(SwitchKind::TypeTag),
                    Some(SwitchKind::TypeTag) => {}
                    Some(_) => return false,
                }
                match self.classify_pattern_type_tag(pattern) {
                    Some(tags) => {
                        for tag in tags {
                            if seen_values.insert(tag) {
                                int_arms.push((tag, i));
                            }
                        }
                    }
                    None => return false,
                }
                continue;
            }

            // Helpers that classify a pattern (the arm pattern itself, or a
            // sub-pattern of an `Or`) into a switch kind. Mutate `switch_kind`
            // and `int_arms`. Return `false` if the pattern disqualifies.
            let pat = &self.body.patterns[pattern];
            let classify_atom = |this: &Self,
                                 atom_id: AstPatId,
                                 atom: &AstPattern,
                                 switch_kind: &mut Option<SwitchKind>,
                                 int_arms: &mut Vec<(i64, usize)>,
                                 seen_values: &mut HashSet<i64>|
             -> bool {
                match atom {
                    // OLD `Literal(Int(val))`: integer switch
                    AstPattern::Type(AstTypeExpr::Literal {
                        value: AstLiteral::Int(val),
                        ..
                    }) => {
                        match switch_kind.as_ref() {
                            None => *switch_kind = Some(SwitchKind::Integer),
                            Some(SwitchKind::Integer) => {}
                            Some(_) => return false,
                        }
                        if seen_values.insert(*val) {
                            int_arms.push((*val, i));
                        }
                        true
                    }
                    // OLD `EnumVariant { ... }`: integer switch with discriminant.
                    // The new repr puts enum variants inside `Pattern::Type`;
                    // detect via TIR.
                    AstPattern::Type(AstTypeExpr::Path { .. })
                        if matches!(
                            this.pat_types.get(&this.pat_metadata_key(atom_id)),
                            Some(Tir2Ty::EnumVariant(_, _, _))
                        ) =>
                    {
                        let Some(Tir2Ty::EnumVariant(qtn, variant, _)) =
                            this.pat_types.get(&this.pat_metadata_key(atom_id))
                        else {
                            unreachable!("guarded by matches! above");
                        };
                        let enum_name = qtn.clone();
                        let variant = variant.clone();
                        match switch_kind.as_ref() {
                            None => {
                                *switch_kind =
                                    Some(SwitchKind::EnumDiscriminant(enum_name.clone()));
                            }
                            Some(SwitchKind::EnumDiscriminant(n)) if *n == enum_name => {}
                            _ => return false,
                        }
                        let idx = this
                            .enum_variants
                            .get(&enum_name)
                            .and_then(|m| m.get(variant.as_str()))
                            .copied();
                        let Some(idx) = idx else { return false };
                        let disc = i64::try_from(idx).expect("discriminant overflow");
                        if seen_values.insert(disc) {
                            int_arms.push((disc, i));
                        }
                        true
                    }
                    // OLD `Type(_)` / `Bind { .. }` (with TIR type): TypeTag.
                    AstPattern::Type(_) | AstPattern::Bind { .. } => {
                        match switch_kind.as_ref() {
                            None => *switch_kind = Some(SwitchKind::TypeTag),
                            Some(SwitchKind::TypeTag) => {}
                            Some(_) => return false,
                        }
                        match this.classify_pattern_type_tag(atom_id) {
                            Some(tags) => {
                                for tag in tags {
                                    if seen_values.insert(tag) {
                                        int_arms.push((tag, i));
                                    }
                                }
                            }
                            None => return false,
                        }
                        true
                    }
                    _ => false,
                }
            };

            match pat {
                AstPattern::Or(sub_pats) => {
                    for sub_pat_id in sub_pats {
                        let sub_pat = &self.body.patterns[*sub_pat_id];
                        if !classify_atom(
                            self,
                            *sub_pat_id,
                            sub_pat,
                            &mut switch_kind,
                            &mut int_arms,
                            &mut seen_values,
                        ) {
                            return false;
                        }
                    }
                }
                AstPattern::Wildcard => {
                    if i != arms.len() - 1 {
                        return false;
                    }
                    otherwise_idx = Some(i);
                }
                // Plain `let x` without a narrow always acts as the
                // catch-all arm, enabling jump-table dispatch. (Narrowed
                // bindings — e.g. `let n: int` — are encoded as `Chain` and
                // were handled by the `pattern_narrow_type` branch above.)
                AstPattern::Bind { .. } => {
                    if i != arms.len() - 1 {
                        return false;
                    }
                    otherwise_idx = Some(i);
                }
                _ => {
                    if !classify_atom(
                        self,
                        pattern,
                        pat,
                        &mut switch_kind,
                        &mut int_arms,
                        &mut seen_values,
                    ) {
                        return false;
                    }
                }
            }
        }

        // Need at least one int arm to justify a switch.
        if int_arms.is_empty() {
            return false;
        }

        // TypeTag switches only pay off at 4+ arms (JumpTable). For fewer arms
        // the sequential `is_type` chain is more compact because the if-else
        // chain adds copy/pop stack management overhead per arm.
        if matches!(switch_kind, Some(SwitchKind::TypeTag)) && int_arms.len() < 4 {
            return false;
        }

        // Exhaustiveness: for **match** TypeTag switches without a wildcard arm,
        // all typed arms together cover the union — the otherwise block is dead.
        // TIR's `required_match_cases` returns None for class types, so class
        // unions are never marked exhaustive by TIR even when all arms are
        // covered. For match + TypeTag, if there's no wildcard, treat as
        // exhaustive so the last arm skips its comparison and the otherwise
        // block becomes Unreachable.
        //
        // For **catch** expressions, we never mark the switch as exhaustive
        // even when all declared thrown types are covered, because panics can
        // always occur at runtime and must be rethrown via the otherwise block.
        let is_match = matches!(&otherwise, SwitchOtherwise::Match { .. });
        let is_switch_exhaustive = otherwise_idx.is_none()
            && (is_exhaustive || (is_match && matches!(switch_kind, Some(SwitchKind::TypeTag))));

        // Save the entry block — this is where the switch terminator goes
        let bb_entry = self.builder.current_block();

        // Emit discriminant/type-tag extraction before building arm blocks.
        // We must do this before create_block() calls so the assignment goes into bb_entry.
        let switch_operand = match &switch_kind {
            Some(SwitchKind::EnumDiscriminant(_)) => {
                let disc = self.builder.temp(Ty::Int {
                    attr: TyAttr::default(),
                });
                self.builder.assign(
                    Place::local(disc),
                    Rvalue::Discriminant(Place::local(scrutinee)),
                );
                Operand::Copy(Place::Local(disc))
            }
            Some(SwitchKind::TypeTag) => {
                let tag_local = self.builder.temp(Ty::Int {
                    attr: TyAttr::default(),
                });
                self.builder.assign(
                    Place::local(tag_local),
                    Rvalue::TypeTag(Place::local(scrutinee)),
                );
                Operand::Copy(Place::Local(tag_local))
            }
            _ => Operand::Copy(Place::Local(scrutinee)),
        };

        // Build body blocks for each arm. Union sub-patterns sharing the same
        // arm_idx reuse a single block (e.g. Active | Pending → same bb).
        let bb_otherwise = self.builder.create_block();
        let mut switch_arms: Vec<(i64, BlockId)> = Vec::new();
        let mut arm_blocks: std::collections::HashMap<usize, BlockId> =
            std::collections::HashMap::new();

        for &(val, arm_idx) in &int_arms {
            if let Some(&existing_bb) = arm_blocks.get(&arm_idx) {
                // Union sub-pattern: reuse the same body block
                switch_arms.push((val, existing_bb));
            } else {
                // Use pre-created block if available, otherwise create a new one
                let bb_body = if let Some(blocks) = pre_created_blocks {
                    blocks[arm_idx].expect("pre-created block missing for arm")
                } else {
                    self.builder.create_block()
                };
                switch_arms.push((val, bb_body));
                arm_blocks.insert(arm_idx, bb_body);

                self.builder.set_current_block(bb_body);
                let (pattern, body, _) = arms[arm_idx];
                let saved_locals = self.locals.clone();
                let watched_depth = self.watched_locals_stack.len();
                self.bind_pattern(scrutinee, pattern);
                self.lower_expr(body, dest.clone());
                if !self.builder.is_current_terminated() {
                    // A `watch let` declared inside an arm body must be torn
                    // down on fallthrough. Without this the watcher leaks past
                    // the arm. Mirrors `lower_match_chain`.
                    self.emit_unwatch_to_depth(watched_depth);
                    self.builder.goto(join);
                }
                // Restore both the name→local map AND truncate the watched
                // stack back to the arm-entry depth (mirrors `lower_scoped_block`).
                self.restore_locals_after_scope(saved_locals, watched_depth);
            }
        }

        // Build arm_names: symbolic labels for the switch arms (debug metadata).
        let arm_names: Vec<(i64, String)> = match &switch_kind {
            Some(SwitchKind::EnumDiscriminant(enum_name)) => {
                if let Some(variants) = self.enum_variants.get(enum_name) {
                    // Build reverse map: variant_idx -> variant_name
                    let reverse: std::collections::HashMap<i64, &str> = variants
                        .iter()
                        .map(|(name, idx)| {
                            (
                                i64::try_from(*idx).expect("discriminant overflow"),
                                name.as_str(),
                            )
                        })
                        .collect();
                    int_arms
                        .iter()
                        .filter_map(|(val, _)| {
                            reverse
                                .get(val)
                                .map(|vname| (*val, format!("{}.{vname}", enum_name.name())))
                        })
                        .collect()
                } else {
                    vec![]
                }
            }
            Some(SwitchKind::TypeTag) => {
                // Reverse map: tag value → human-readable type name.
                let reverse_class: std::collections::HashMap<i64, &str> = self
                    .class_type_tags
                    .iter()
                    .map(|(tn, tag)| (*tag, tn.name().as_str()))
                    .collect();
                int_arms
                    .iter()
                    .map(|(v, _)| {
                        let name = reverse_class
                            .get(v)
                            .map(ToString::to_string)
                            .unwrap_or_else(|| format_type_tag_name(*v));
                        (*v, name)
                    })
                    .collect()
            }
            _ => int_arms.iter().map(|(v, _)| (*v, v.to_string())).collect(),
        };

        // Lower the otherwise block
        self.builder.set_current_block(bb_otherwise);
        if let Some(idx) = otherwise_idx {
            // Wildcard arm present
            if let SwitchOtherwise::Catch {
                error_local,
                needs_throw_if_panic: true,
            } = &otherwise
            {
                let bb_wildcard_body = self.builder.create_block();
                self.builder
                    .throw_if_panic(Operand::Copy(Place::Local(*error_local)), bb_wildcard_body);
                self.builder.set_current_block(bb_wildcard_body);
            }
            let (pattern, body, _) = arms[idx];
            let saved_locals = self.locals.clone();
            let watched_depth = self.watched_locals_stack.len();
            self.bind_pattern(scrutinee, pattern);
            self.lower_expr(body, dest);
            if !self.builder.is_current_terminated() {
                // A `watch let` declared inside the wildcard body must be
                // torn down on fallthrough; mirrors the int-arm path above.
                self.emit_unwatch_to_depth(watched_depth);
                self.builder.goto(join);
            }
            // Restore name→local map AND truncate the watched stack back to
            // the arm-entry depth (mirrors `lower_scoped_block`).
            self.restore_locals_after_scope(saved_locals, watched_depth);
        } else {
            // No wildcard — decide what the otherwise block does.
            // Use `is_switch_exhaustive` (which may be inferred for TypeTag)
            // rather than the caller's original `is_exhaustive`, so the
            // otherwise block stays consistent with the switch terminator flag.
            if is_switch_exhaustive {
                match &otherwise {
                    SwitchOtherwise::Match { .. } => {
                        self.builder.unreachable();
                    }
                    SwitchOtherwise::Catch { error_local, .. } => {
                        // Even if exhaustive, catch otherwise should rethrow
                        // (the error might not match any arm at runtime).
                        self.builder
                            .throw(Operand::Copy(Place::Local(*error_local)));
                    }
                }
            } else {
                match &otherwise {
                    SwitchOtherwise::Catch { error_local, .. } => {
                        self.builder
                            .throw(Operand::Copy(Place::Local(*error_local)));
                    }
                    SwitchOtherwise::Match { .. } => {
                        self.builder.goto(join);
                    }
                }
            }
        }

        // For catch with pre-created blocks: redirect wildcard arm's pre-created block
        // to bb_otherwise, since the wildcard body was lowered there.
        if let Some(blocks) = pre_created_blocks {
            for (i, block_opt) in blocks.iter().enumerate() {
                if let Some(block) = block_opt {
                    if otherwise_idx == Some(i) {
                        // Wildcard arm's pre-created block → redirect to otherwise
                        self.builder.set_current_block(*block);
                        self.builder.goto(bb_otherwise);
                    } else if !arm_blocks.contains_key(&i) {
                        // Unreachable pre-created block (e.g. duplicate tag) → terminate it
                        self.builder.set_current_block(*block);
                        self.builder.goto(bb_otherwise);
                    }
                }
            }
        }

        // Emit the switch terminator in the entry block
        self.builder.set_current_block(bb_entry);
        self.builder.switch(
            switch_operand,
            switch_arms,
            bb_otherwise,
            is_switch_exhaustive,
            arm_names,
        );

        true
    }

    fn lower_match_chain(
        &mut self,
        scrutinee: Local,
        arms: &[baml_compiler2_ast::MatchArm],
        dest: Place,
        join: BlockId,
        exhaustive: bool,
    ) {
        if arms.is_empty() {
            // No more arms to test. Either a preceding wildcard/binding arm
            // consumed all inputs (making this dead code), or the match is
            // non-exhaustive and a runtime value could reach here. In both
            // cases, jump to the join block so execution continues.
            self.builder.goto(join);
            return;
        }

        let arm = &arms[0];
        let rest = &arms[1..];

        // Exhaustive last arm: skip the pattern test — it must match. Do not
        // take this shortcut for Or-patterns because bindings must come from
        // the specific alternative that matched.
        if exhaustive
            && rest.is_empty()
            && arm.guard.is_none()
            && !matches!(self.body.patterns[arm.pattern], AstPattern::Or(_))
        {
            let saved_locals = self.locals.clone();
            let watched_depth = self.watched_locals_stack.len();
            self.bind_pattern(scrutinee, arm.pattern);
            self.lower_expr(arm.body, dest);
            if !self.builder.is_current_terminated() {
                // A `watch let` declared inside an arm body must be torn
                // down on fallthrough. Without this the watcher leaks past
                // the arm.
                self.emit_unwatch_to_depth(watched_depth);
                self.builder.goto(join);
            }
            // Restore both the name→local map AND truncate the watched stack
            // back to the arm-entry depth (mirrors `lower_scoped_block`).
            self.restore_locals_after_scope(saved_locals, watched_depth);
            return;
        }

        if let AstPattern::Or(parts) = self.body.patterns[arm.pattern].clone() {
            let bb_next = self.builder.create_block();
            for (idx, part) in parts.iter().copied().enumerate() {
                let bb_body = self.builder.create_block();
                let bb_alt_next = if idx + 1 == parts.len() {
                    bb_next
                } else {
                    self.builder.create_block()
                };

                self.lower_pattern_test(scrutinee, part, bb_body, bb_alt_next);

                self.builder.set_current_block(bb_body);
                let saved_locals = self.locals.clone();
                let watched_depth = self.watched_locals_stack.len();
                self.bind_pattern(scrutinee, part);
                if let Some(guard) = arm.guard {
                    let guard_op = self.lower_to_operand(guard);
                    let bb_guarded = self.builder.create_block();
                    self.builder.branch(guard_op, bb_guarded, bb_next);
                    self.builder.set_current_block(bb_guarded);
                }
                self.lower_expr(arm.body, dest.clone());
                if !self.builder.is_current_terminated() {
                    self.emit_unwatch_to_depth(watched_depth);
                    self.builder.goto(join);
                }
                self.restore_locals_after_scope(saved_locals, watched_depth);

                if idx + 1 < parts.len() {
                    self.builder.set_current_block(bb_alt_next);
                }
            }

            self.builder.set_current_block(bb_next);
            self.lower_match_chain(scrutinee, rest, dest, join, exhaustive);
            return;
        }

        let bb_body = self.builder.create_block();
        let bb_next = self.builder.create_block();

        self.lower_pattern_test(scrutinee, arm.pattern, bb_body, bb_next);

        self.builder.set_current_block(bb_body);
        let saved_locals = self.locals.clone();
        let watched_depth = self.watched_locals_stack.len();
        self.bind_pattern(scrutinee, arm.pattern);
        if let Some(guard) = arm.guard {
            let guard_op = self.lower_to_operand(guard);
            let bb_guarded = self.builder.create_block();
            self.builder.branch(guard_op, bb_guarded, bb_next);
            self.builder.set_current_block(bb_guarded);
        }
        self.lower_expr(arm.body, dest.clone());
        if !self.builder.is_current_terminated() {
            // See exhaustive arm comment above.
            self.emit_unwatch_to_depth(watched_depth);
            self.builder.goto(join);
        }
        self.restore_locals_after_scope(saved_locals, watched_depth);

        self.builder.set_current_block(bb_next);
        self.lower_match_chain(scrutinee, rest, dest, join, exhaustive);
    }

    /// Emit an `IsType` check that handles union types by expanding them
    /// into a chain: try each member, branch to `success` if any matches.
    fn emit_is_type_branch(
        &mut self,
        scrutinee: Local,
        ty: Ty,
        success: BlockId,
        failure: BlockId,
    ) {
        // BEP-044/BEP-057: testing a value against an *interface* type means
        // "is its runtime class an implementor". Interface types used to lower
        // to `Ty::Class`; they now lower to `Ty::Interface` so reflection can
        // retain associated bindings. Accept both runtime shapes here.
        if let Ty::Class(tn, _, _) | Ty::Interface(tn, _, _, _) = &ty
            && let Some(impls) = self.interface_implementors.get(tn).cloned()
        {
            if impls.is_empty() {
                // No class implements the interface — the test can never hold.
                self.builder.goto(failure);
                return;
            }
            let members: Vec<Ty> = impls
                .into_iter()
                .map(|cn| Ty::Class(cn, Vec::new(), TyAttr::default()))
                .collect();
            self.emit_is_type_branch(
                scrutinee,
                Ty::Union(members, TyAttr::default()),
                success,
                failure,
            );
            return;
        }
        if let Ty::Union(members, _) = ty {
            // For union A | B | C: check A → success, else check B → success,
            // else check C → success, else failure.
            let mut remaining = members.into_iter().peekable();
            while let Some(member) = remaining.next() {
                if remaining.peek().is_none() {
                    // Last member: branch directly to success/failure.
                    self.emit_is_type_branch(scrutinee, member, success, failure);
                } else {
                    // Not last: if this member matches, jump to success;
                    // otherwise try the next member.
                    let next_check = self.builder.create_block();
                    self.emit_is_type_branch(scrutinee, member, success, next_check);
                    self.builder.set_current_block(next_check);
                }
            }
        } else {
            // Convert Ty → TyTemplate so the emitter can handle generic class
            // checks (Ty::Class with args containing TypeVars map to
            // TyTemplate::Class with TypeArgRef leaves).  For non-generic types
            // the template is TyTemplate::Concrete(ty) — the emitter falls back
            // to the same fast path as before.
            let ty_template = ty_to_template_from_resolved_ty(&ty);
            self.emit_is_type_template_branch(scrutinee, ty_template, success, failure);
        }
    }

    /// Emit an `IsType` test + branch for an already-built `TyTemplate`.
    ///
    /// Used directly (instead of [`Self::emit_is_type_branch`]) when the
    /// pattern type still contains the enclosing function's `TypeVar`s: the
    /// caller builds the template via `ty_to_template` so those lower to
    /// `TypeArgRef` leaves resolved against `frame.type_args` at runtime,
    /// rather than being erased to `Ty::Void` (a constant-false test).
    fn emit_is_type_template_branch(
        &mut self,
        scrutinee: Local,
        ty_template: TyTemplate,
        success: BlockId,
        failure: BlockId,
    ) {
        let test = Rvalue::IsType {
            operand: Operand::Copy(Place::Local(scrutinee)),
            ty_template,
        };
        let test_local = self.builder.temp(Ty::Bool {
            attr: TyAttr::default(),
        });
        self.builder.assign(Place::local(test_local), test);
        self.builder
            .branch(Operand::Copy(Place::Local(test_local)), success, failure);
    }

    fn emit_is_tir_type_branch(
        &mut self,
        scrutinee: Local,
        ty: &Tir2Ty,
        success: BlockId,
        failure: BlockId,
    ) {
        let mut visited = HashSet::new();
        self.emit_is_tir_type_branch_inner(scrutinee, ty, success, failure, &mut visited);
    }

    /// The members of a union receiver, transparently unwrapping `Optional`
    /// layers — `(Dog | Named)?` after a null check still dispatches the
    /// field/method on the underlying union. Returns `None` when `ty` isn't a
    /// (optionally-wrapped) union.
    fn tir_union_members(ty: &Tir2Ty) -> Option<Vec<Tir2Ty>> {
        match ty {
            Tir2Ty::Union(members, _) => Some(members.clone()),
            _ => None,
        }
    }

    /// Whether `ty` is (or contains, inside a union/optional) an interface view
    /// whose runtime test must respect type arguments or associated bindings.
    /// Used to opt only these patterns into the TIR-typed test path, leaving
    /// non-interface patterns on the unchanged erased fast path.
    fn tir_ty_needs_interface_shape_test(ty: &Tir2Ty) -> bool {
        match ty {
            Tir2Ty::Interface(_, args, associated_bindings, _) => {
                !args.is_empty() || !associated_bindings.is_empty()
            }
            Tir2Ty::Union(members, _) => {
                members.iter().any(Self::tir_ty_needs_interface_shape_test)
            }
            _ => false,
        }
    }

    fn emit_is_tir_type_branch_inner(
        &mut self,
        scrutinee: Local,
        ty: &Tir2Ty,
        success: BlockId,
        failure: BlockId,
        visited: &mut HashSet<String>,
    ) {
        match ty {
            Tir2Ty::Union(members, _) => {
                let mut remaining = members.iter().peekable();
                while let Some(member) = remaining.next() {
                    if remaining.peek().is_none() {
                        self.emit_is_tir_type_branch_inner(
                            scrutinee, member, success, failure, visited,
                        );
                    } else {
                        let next_check = self.builder.create_block();
                        self.emit_is_tir_type_branch_inner(
                            scrutinee, member, success, next_check, visited,
                        );
                        self.builder.set_current_block(next_check);
                    }
                }
            }
            Tir2Ty::Class(qtn, type_args, _) if !type_args.is_empty() => {
                let erased = self.resolved_aliases.convert(ty);
                let class_fields = self.lookup_tir_class_fields(qtn, type_args);
                if class_fields.is_empty() {
                    self.emit_is_type_branch(scrutinee, erased, success, failure);
                    return;
                }

                let class_success = self.builder.create_block();
                self.emit_is_type_branch(scrutinee, erased, class_success, failure);
                self.builder.set_current_block(class_success);

                let key = format!("{qtn:?}<{type_args:?}>");
                if !visited.insert(key.clone()) {
                    self.builder.goto(success);
                    return;
                }

                let class_tn = qtn.clone();
                let fields: Vec<_> = class_fields.into_iter().collect();
                for (idx, (field_name, field_ty)) in fields.iter().enumerate() {
                    let next = if idx + 1 == fields.len() {
                        success
                    } else {
                        self.builder.create_block()
                    };

                    let Some(field_idx) = self
                        .class_fields
                        .get(&class_tn)
                        .and_then(|fields| fields.get(field_name.as_str()))
                        .copied()
                    else {
                        self.builder.goto(failure);
                        visited.remove(&key);
                        return;
                    };

                    let field_local = self.builder.temp(self.resolved_aliases.convert(field_ty));
                    self.builder.assign(
                        Place::local(field_local),
                        Rvalue::Use(Operand::Copy(Place::Field {
                            base: Box::new(Place::Local(scrutinee)),
                            field: field_idx,
                        })),
                    );
                    self.emit_is_tir_type_branch_inner(
                        field_local,
                        field_ty,
                        next,
                        failure,
                        visited,
                    );
                    if idx + 1 < fields.len() {
                        self.builder.set_current_block(next);
                    }
                }

                visited.remove(&key);
            }
            // An associated or generic interface pattern (`Slot<int>`,
            // `Source<Item=int>`) must respect its full interface view: test
            // only the implementors of *that* view, not every implementor of
            // the bare interface.
            Tir2Ty::Interface(iface_qtn, type_args, associated_bindings, _)
                if !type_args.is_empty() || !associated_bindings.is_empty() =>
            {
                let iface_tn = iface_qtn.clone();
                let guards = self.interface_implementor_class_guards(
                    &iface_tn,
                    type_args,
                    associated_bindings,
                );
                if guards.is_empty() {
                    self.builder.goto(failure);
                    return;
                }
                let mut next_check = self.builder.current_block();
                for (idx, (impl_tn, guard)) in guards.iter().enumerate() {
                    let bb_next = if idx + 1 == guards.len() {
                        failure
                    } else {
                        self.builder.create_block()
                    };
                    self.builder.set_current_block(next_check);
                    self.emit_interface_class_guard_branch(
                        scrutinee, impl_tn, guard, success, bb_next,
                    );
                    next_check = bb_next;
                }
            }
            // Singleton-valued types pin a specific runtime value, so emit
            // equality checks rather than type-tag tests. `is_type` on a
            // literal type like `Ty::Literal("specific")` checks the value's
            // *type* (string) rather than its content — which is too permissive
            // and would let `let x: "specific" => …` fire on any string.
            Tir2Ty::Literal(lit, _, _) => {
                let constant = Self::lower_literal(lit);
                self.emit_value_eq_branch(scrutinee, Operand::Constant(constant), success, failure);
            }
            Tir2Ty::Null { .. } => {
                self.emit_value_eq_branch(
                    scrutinee,
                    Operand::Constant(Constant::Null),
                    success,
                    failure,
                );
            }
            _ => {
                let resolved = self.resolved_aliases.convert(ty);
                self.emit_is_type_branch(scrutinee, resolved, success, failure);
            }
        }
    }

    /// Branch on `scrutinee == rhs` (value equality). Used for singleton-typed
    /// patterns where the type pins a specific value.
    fn emit_value_eq_branch(
        &mut self,
        scrutinee: Local,
        rhs: Operand,
        success: BlockId,
        failure: BlockId,
    ) {
        let test = Rvalue::BinaryOp {
            op: BinOp::Eq,
            left: Operand::Copy(Place::Local(scrutinee)),
            right: rhs,
        };
        let test_local = self.builder.temp(Ty::Bool {
            attr: TyAttr::default(),
        });
        self.builder.assign(Place::local(test_local), test);
        self.builder
            .branch(Operand::Copy(Place::Local(test_local)), success, failure);
    }

    fn lookup_tir_class_fields(
        &self,
        class_name: &QualifiedTypeName,
        class_type_args: &[Tir2Ty],
    ) -> IndexMap<Name, Tir2Ty> {
        let pkg_id = PackageId::new(self.db, class_name.package().clone());
        let pkg_items_for_class = package_items(self.db, pkg_id);
        let Some(Definition::Class(class_loc)) =
            pkg_items_for_class.lookup_type(class_name.namespace(), class_name.name())
        else {
            return IndexMap::new();
        };

        let file = class_loc.file(self.db);
        let ns_context = file_package(self.db, file).namespace_path;
        let item_tree = file_item_tree(self.db, file);
        let class_data = &item_tree[class_loc.id(self.db)];
        let bindings = baml_compiler2_tir::generics::bind_type_vars(
            &class_data.generic_params,
            class_type_args,
        );

        let mut result = IndexMap::new();
        for field in &class_data.fields {
            let mut diags = Vec::new();
            let field_ty = field
                .type_expr
                .as_ref()
                .map(|te| {
                    if bindings.is_empty() {
                        baml_compiler2_tir::lower_type_expr::lower_type_expr_in_ns(
                            self.db,
                            &te.expr,
                            pkg_items_for_class,
                            &ns_context,
                            &class_data.generic_params,
                            &mut diags,
                        )
                    } else {
                        baml_compiler2_tir::generics::lower_type_expr_with_generics(
                            self.db,
                            &te.expr,
                            pkg_items_for_class,
                            &ns_context,
                            &bindings,
                            &mut diags,
                        )
                    }
                })
                .unwrap_or(Tir2Ty::Unknown {
                    attr: baml_compiler2_tir::ty::TyAttr::default(),
                });
            result.insert(field.name.clone(), field_ty);
        }
        result
    }

    /// Look up the integer type tag for a type. Returns `Some(tag)` for
    /// primitives (INT=0, STRING=1, etc.) and classes (`CLASS_BASE` + index),
    /// or `None` for types that don't have a tag (unions, generics, etc.).
    fn type_tag_for_ty(&self, ty: &Ty) -> Option<i64> {
        match ty {
            Ty::Int { .. } => Some(baml_type::typetag::INT),
            Ty::Bigint { .. } => Some(baml_type::typetag::BIGINT),
            Ty::String { .. } => Some(baml_type::typetag::STRING),
            Ty::Bool { .. } => Some(baml_type::typetag::BOOL),
            Ty::Null { .. } => Some(baml_type::typetag::NULL),
            Ty::Float { .. } => Some(baml_type::typetag::FLOAT),
            Ty::Uint8Array { .. } => Some(baml_type::typetag::UINT8ARRAY),
            Ty::Enum(..) | Ty::EnumVariant(..) => Some(baml_type::typetag::ENUM),
            Ty::List(..) => Some(baml_type::typetag::LIST),
            Ty::Map { .. } => Some(baml_type::typetag::MAP),
            Ty::Function { .. } => Some(baml_type::typetag::FUNCTION),
            Ty::Future(..) => Some(baml_type::typetag::FUTURE),
            Ty::Type { .. } => Some(baml_type::typetag::TYPE),
            Ty::Class(tn, _, _) => self.class_type_tags.get(tn).copied(),
            _ => None,
        }
    }

    fn pattern_contains_structural(&self, pat_id: AstPatId) -> bool {
        match &self.body.patterns[pat_id] {
            AstPattern::Class { .. } | AstPattern::Array { .. } => true,
            AstPattern::Or(parts) => parts.iter().any(|p| self.pattern_contains_structural(*p)),
            AstPattern::Wildcard | AstPattern::Bind { .. } | AstPattern::Type(_) => false,
        }
    }

    fn class_pattern_fields(&self, pat_id: AstPatId) -> Vec<baml_compiler2_ast::FieldPat> {
        match &self.body.patterns[pat_id] {
            AstPattern::Class { fields, .. } => fields.clone(),
            _ => Vec::new(),
        }
    }

    fn class_pattern_type_name(&self, pat_id: AstPatId) -> Option<TypeName> {
        let tir_ty = self.pat_types.get(&self.pat_metadata_key(pat_id))?;
        match convert_tir2_ty(tir_ty, self.resolved_aliases) {
            Ty::Class(tn, _, _) => Some(tn),
            _ => None,
        }
    }

    fn class_pattern_field_ty(&self, pat_id: AstPatId, field: &Name) -> Option<Ty> {
        let tir_ty = self.pat_types.get(&self.pat_metadata_key(pat_id))?;
        let Tir2Ty::Class(qtn, type_args, _) = tir_ty else {
            return None;
        };
        let fields = self.lookup_tir_class_fields(qtn, type_args);
        fields
            .get(field)
            .map(|field_ty| self.resolved_aliases.convert(field_ty))
    }

    fn project_class_pattern_field(
        &mut self,
        scrutinee: Local,
        class_pat_id: AstPatId,
        field_pat_id: AstPatId,
        field: &Name,
    ) -> Option<Local> {
        // BEP-044: an interface head (`Animal { name } => ...`) has no
        // positional field layout. Branch on the raw TIR type so interface
        // patterns project through field-view dispatch instead of class slots.
        if matches!(
            self.pat_types.get(&self.pat_metadata_key(class_pat_id)),
            Some(Tir2Ty::Interface(..))
        ) {
            return self.project_interface_pattern_field(
                scrutinee,
                class_pat_id,
                field_pat_id,
                field,
            );
        }
        let class_tn = self.class_pattern_type_name(class_pat_id)?;
        let field_idx = self
            .class_fields
            .get(&class_tn)?
            .get(field.as_str())
            .copied()?;
        let inferred_pat_ty = self.pat_ty(field_pat_id);
        let source_field_ty = self.class_pattern_field_ty(class_pat_id, field);
        let cached_field_ty = self
            .class_field_types
            .get(&class_tn)
            .and_then(|fields| fields.get(field.as_str()))
            .cloned();
        let field_ty = source_field_ty
            .or_else(|| cached_field_ty.filter(|ty| !Self::is_pattern_type_recovery(ty)))
            .unwrap_or(inferred_pat_ty);
        let field_local = self.builder.temp(field_ty);
        self.builder.assign(
            Place::local(field_local),
            Rvalue::Use(Operand::Copy(Place::Field {
                base: Box::new(Place::Local(scrutinee)),
                field: field_idx,
            })),
        );
        Some(field_local)
    }

    /// BEP-044: project a field bound by an *interface* destructure pattern
    /// (`Animal { name } => …`). The scrutinee's concrete runtime class is not
    /// known statically, so we can't index a fixed field slot. Instead we reuse
    /// the interface field-view dispatch (`try_lower_interface_field_access`) —
    /// the same code that lowers `iface_value.name` — to read the linked field
    /// off whichever implementor the value actually is.
    fn project_interface_pattern_field(
        &mut self,
        scrutinee: Local,
        class_pat_id: AstPatId,
        field_pat_id: AstPatId,
        field: &Name,
    ) -> Option<Local> {
        let tir_ty = self
            .pat_types
            .get(&self.pat_metadata_key(class_pat_id))?
            .clone();
        let (iface_tn, iface_args, iface_assoc) =
            self.interface_dispatch_target_for_tir_ty(&tir_ty)?;
        let field_local = self.builder.temp(self.pat_ty(field_pat_id));
        self.try_lower_interface_field_access(
            scrutinee,
            &iface_tn,
            &iface_args,
            &iface_assoc,
            field,
            &Place::local(field_local),
        )
        .then_some(field_local)
    }

    fn const_int_local(&mut self, value: i64) -> Local {
        let local = self.builder.temp(Ty::Int {
            attr: TyAttr::default(),
        });
        self.builder.assign(
            Place::local(local),
            Rvalue::Use(Operand::Constant(Constant::Int(value))),
        );
        local
    }

    fn const_usize_int_local(&mut self, value: usize) -> Local {
        self.const_int_local(i64::try_from(value).expect("array pattern length/index overflow"))
    }

    fn array_len_local(&mut self, scrutinee: Local) -> Local {
        let len_local = self.builder.temp(Ty::Int {
            attr: TyAttr::default(),
        });
        self.builder.assign(
            Place::local(len_local),
            Rvalue::Len(Place::local(scrutinee)),
        );
        len_local
    }

    fn lower_array_pattern_length_test(
        &mut self,
        scrutinee: Local,
        has_rest: bool,
        fixed_len: usize,
        success: BlockId,
        failure: BlockId,
    ) {
        let len_local = self.array_len_local(scrutinee);
        let expected = self.const_usize_int_local(fixed_len);
        let test_local = self.builder.temp(Ty::Bool {
            attr: TyAttr::default(),
        });
        self.builder.assign(
            Place::local(test_local),
            Rvalue::BinaryOp {
                op: if has_rest { BinOp::Ge } else { BinOp::Eq },
                left: Operand::Copy(Place::local(len_local)),
                right: Operand::Copy(Place::local(expected)),
            },
        );
        self.builder
            .branch(Operand::Copy(Place::local(test_local)), success, failure);
    }

    fn project_array_pattern_element_from_start(
        &mut self,
        scrutinee: Local,
        elem_pat: AstPatId,
        index: usize,
    ) -> Local {
        let index_local = self.const_usize_int_local(index);
        self.project_array_pattern_element(scrutinee, elem_pat, index_local)
    }

    fn project_array_pattern_element_from_end(
        &mut self,
        scrutinee: Local,
        elem_pat: AstPatId,
        index_from_end: usize,
    ) -> Local {
        let len_local = self.array_len_local(scrutinee);
        let offset = self.const_usize_int_local(index_from_end);
        let index_local = self.builder.temp(Ty::Int {
            attr: TyAttr::default(),
        });
        self.builder.assign(
            Place::local(index_local),
            Rvalue::BinaryOp {
                op: BinOp::Sub,
                left: Operand::Copy(Place::local(len_local)),
                right: Operand::Copy(Place::local(offset)),
            },
        );
        self.project_array_pattern_element(scrutinee, elem_pat, index_local)
    }

    fn project_array_pattern_element(
        &mut self,
        scrutinee: Local,
        elem_pat: AstPatId,
        index_local: Local,
    ) -> Local {
        let elem_ty = self.pat_ty(elem_pat);
        let elem_local = self.builder.temp(elem_ty);
        self.builder.assign(
            Place::local(elem_local),
            Rvalue::Use(Operand::Copy(Place::Index {
                base: Box::new(Place::Local(scrutinee)),
                index: index_local,
                kind: IndexKind::Array,
            })),
        );
        elem_local
    }

    fn project_array_pattern_rest(
        &mut self,
        scrutinee: Local,
        rest_pat: AstPatId,
        prefix_len: usize,
        suffix_len: usize,
    ) -> Local {
        let rest_ty = self.pat_ty(rest_pat);
        let rest_local = self.builder.temp(rest_ty);
        let start = self.const_usize_int_local(prefix_len);
        let end = if suffix_len == 0 {
            self.array_len_local(scrutinee)
        } else {
            let len_local = self.array_len_local(scrutinee);
            let suffix = self.const_usize_int_local(suffix_len);
            let end = self.builder.temp(Ty::Int {
                attr: TyAttr::default(),
            });
            self.builder.assign(
                Place::local(end),
                Rvalue::BinaryOp {
                    op: BinOp::Sub,
                    left: Operand::Copy(Place::local(len_local)),
                    right: Operand::Copy(Place::local(suffix)),
                },
            );
            end
        };
        let target = self.builder.create_block();
        let unwind = self.catch_context.as_ref().map(|c| c.unwind_target);
        self.builder.call(
            Operand::Constant(Constant::Function(ItemRef::Method {
                package: Name::new("baml"),
                namespace: Vec::new(),
                class: Name::new("Array"),
                name: Name::new("slice"),
            })),
            vec![
                Operand::Copy(Place::local(scrutinee)),
                Operand::Copy(Place::local(start)),
                Operand::Copy(Place::local(end)),
            ],
            Place::local(rest_local),
            target,
            unwind,
        );
        self.builder.set_current_block(target);
        rest_local
    }

    fn lower_pattern_test(
        &mut self,
        scrutinee: Local,
        pat_id: AstPatId,
        success: BlockId,
        failure: BlockId,
    ) {
        let pat = self.body.patterns[pat_id].clone();

        // Bind sub-pattern: `let x: <pattern>` defers to the sub-
        // pattern's runtime test (recursively). The bind itself doesn't
        // emit a runtime check; the sub-pattern does.
        if let AstPattern::Bind {
            subpat: Some(sp), ..
        } = &pat
        {
            return self.lower_pattern_test(scrutinee, *sp, success, failure);
        }
        // Array `: T` ascription emits an `is_type` test before the
        // structural shape test below.
        if let AstPattern::Array {
            ascription: Some(ty_expr),
            ..
        } = &pat
        {
            let after_ascription = self.builder.create_block();
            if let Some(tir_ty) = self
                .pat_types
                .get(&self.pat_metadata_key(pat_id))
                .filter(|ty| !matches!(ty, Tir2Ty::Never { .. }))
                .cloned()
            {
                self.emit_is_tir_type_branch(scrutinee, &tir_ty, after_ascription, failure);
            } else {
                let annotation_ty = self.resolve_type_annotation(ty_expr);
                self.emit_is_type_branch(scrutinee, annotation_ty, after_ascription, failure);
            }
            self.builder.set_current_block(after_ascription);
            // Fall through to the array shape test below.
        }

        match &pat {
            AstPattern::Wildcard => {
                self.builder.goto(success);
            }
            AstPattern::Bind { .. } => {
                // A bare `let e` (no annotation — annotated binds carry the
                // annotation as a subpattern and recursed above) is
                // IRREFUTABLE: arm dispatch is sequential, so the bind takes
                // whatever reaches it; its `pat_types` entry is exhaustiveness
                // bookkeeping, not a runtime dispatch condition. Emitting a
                // type test here is at best a tautology and at worst a
                // miscompile: a rigid generic (e.g. the `E` of a combinator's
                // `catch (e) { let e => … }`) erases to `Ty::Void` in
                // `convert_tir2_ty`, making the test constant-false and the
                // catch arm silently rethrow. (Panic fall-through for catch
                // arms is handled separately by `ThrowIfPanic`.)
                self.builder.goto(success);
            }
            // OLD's Pattern::Type covered structural shape tests; OLD's
            // Pattern::Literal / Pattern::Null / Pattern::EnumVariant were
            // separate variants. The new flat enum collapses all of those
            // into `Pattern::Type(TypeExpr)`, so we dispatch on the inner
            // TypeExpr to recover OLD's per-kind codegen.
            AstPattern::Type(ty_expr) => match ty_expr {
                AstTypeExpr::Literal { value: lit, .. } => {
                    let constant = Self::lower_literal(lit);
                    let test = Rvalue::BinaryOp {
                        op: BinOp::Eq,
                        left: Operand::Copy(Place::Local(scrutinee)),
                        right: Operand::Constant(constant),
                    };
                    let test_local = self.builder.temp(Ty::Bool {
                        attr: TyAttr::default(),
                    });
                    self.builder.assign(Place::local(test_local), test);
                    self.builder
                        .branch(Operand::Copy(Place::Local(test_local)), success, failure);
                }
                AstTypeExpr::Null { .. } => {
                    let test = Rvalue::BinaryOp {
                        op: BinOp::Eq,
                        left: Operand::Copy(Place::Local(scrutinee)),
                        right: Operand::Constant(Constant::Null),
                    };
                    let test_local = self.builder.temp(Ty::Bool {
                        attr: TyAttr::default(),
                    });
                    self.builder.assign(Place::local(test_local), test);
                    self.builder
                        .branch(Operand::Copy(Place::Local(test_local)), success, failure);
                }
                AstTypeExpr::Path { .. }
                    if matches!(
                        self.pat_types.get(&self.pat_metadata_key(pat_id)),
                        Some(Tir2Ty::EnumVariant(_, _, _))
                    ) =>
                {
                    let Some(Tir2Ty::EnumVariant(qtn, variant, _)) =
                        self.pat_types.get(&self.pat_metadata_key(pat_id))
                    else {
                        unreachable!("guarded by matches! above");
                    };
                    let enum_ref = ItemRef::EnumType {
                        package: qtn.package().clone(),
                        namespace: qtn.namespace().clone(),
                        name: qtn.name().clone(),
                    };
                    let variant = variant.clone();
                    let test = Rvalue::BinaryOp {
                        op: BinOp::Eq,
                        left: Operand::Copy(Place::Local(scrutinee)),
                        right: Operand::Constant(Constant::EnumVariant { enum_ref, variant }),
                    };
                    let test_local = self.builder.temp(Ty::Bool {
                        attr: TyAttr::default(),
                    });
                    self.builder.assign(Place::local(test_local), test);
                    self.builder
                        .branch(Operand::Copy(Place::Local(test_local)), success, failure);
                }
                _ => {
                    // The annotated-bind recursion (`let e: T => …` recurses
                    // into its Type subpattern) has no `pat_types` entry for
                    // the subpattern, so fall back to lowering the annotation
                    // itself with the enclosing generic params in scope.
                    let pat_tir_ty = self
                        .pat_types
                        .get(&self.pat_metadata_key(pat_id))
                        .cloned()
                        .unwrap_or_else(|| self.lower_type_annotation_tir(ty_expr));
                    // A generic-interface pattern (`Slot<int>`) needs the
                    // TIR-typed test, which preserves the type argument and
                    // tests only the implementors of *that* instantiation —
                    // otherwise the erased path matches every implementor and a
                    // `Slot<string>` value falls into a `Slot<int>` arm.
                    if Self::tir_ty_needs_interface_shape_test(&pat_tir_ty) {
                        self.emit_is_tir_type_branch(scrutinee, &pat_tir_ty, success, failure);
                        return;
                    }
                    // A class pattern type still carrying the enclosing
                    // function's TypeVars (e.g. `let e: AllFailed<E>` inside
                    // `any<T, E>`) must NOT go through `convert_tir2_ty` —
                    // that erases TypeVar → Void and the test becomes
                    // constant-false. Build a template instead so the args
                    // lower to `TypeArgRef` resolved against the frame.
                    if matches!(&pat_tir_ty, Tir2Ty::Class(..))
                        && baml_compiler2_tir::generics::contains_typevar(&pat_tir_ty)
                    {
                        let generic_params = self.enclosing_generic_params();
                        let template = self.ty_to_template(&pat_tir_ty, &generic_params);
                        self.emit_is_type_template_branch(scrutinee, template, success, failure);
                        return;
                    }
                    // Other patterns keep the erased fast path (unchanged codegen).
                    let annotation_ty = convert_tir2_ty(&pat_tir_ty, self.resolved_aliases);
                    self.emit_is_type_branch(scrutinee, annotation_ty, success, failure);
                }
            },
            AstPattern::Or(sub_pats) => {
                if sub_pats.is_empty() {
                    self.builder.goto(failure);
                    return;
                }
                let n = sub_pats.len();
                for (i, &sub_pat) in sub_pats.iter().enumerate() {
                    let next = if i + 1 < n {
                        self.builder.create_block()
                    } else {
                        failure
                    };
                    self.lower_pattern_test(scrutinee, sub_pat, success, next);
                    if i + 1 < n {
                        self.builder.set_current_block(next);
                    }
                }
            }
            AstPattern::Class { .. } => {
                let class_success = if self.class_pattern_fields(pat_id).is_empty() {
                    success
                } else {
                    self.builder.create_block()
                };

                if let Some(tir_ty) = self.pat_types.get(&self.pat_metadata_key(pat_id)).cloned() {
                    self.emit_is_tir_type_branch(scrutinee, &tir_ty, class_success, failure);
                } else if class_success == success {
                    self.builder.goto(success);
                } else {
                    self.builder.goto(class_success);
                }

                if class_success != success {
                    self.builder.set_current_block(class_success);
                    let fields = self.class_pattern_fields(pat_id);
                    for (idx, field) in fields.iter().enumerate() {
                        let next = if idx + 1 == fields.len() {
                            success
                        } else {
                            self.builder.create_block()
                        };
                        if let Some(field_local) = self.project_class_pattern_field(
                            scrutinee,
                            pat_id,
                            field.pat,
                            &field.field,
                        ) {
                            self.lower_pattern_test(field_local, field.pat, next, failure);
                        } else {
                            self.builder.goto(failure);
                        }
                        if idx + 1 < fields.len() {
                            self.builder.set_current_block(next);
                        }
                    }
                }
            }
            AstPattern::Array {
                prefix,
                rest,
                suffix,
                ascription: _,
            } => {
                let array_success = self.builder.create_block();

                if let Some(tir_ty) = self.pat_types.get(&self.pat_metadata_key(pat_id)).cloned() {
                    self.emit_is_tir_type_branch(scrutinee, &tir_ty, array_success, failure);
                } else {
                    self.builder.goto(array_success);
                }

                self.builder.set_current_block(array_success);
                let has_rest_test = rest.as_ref().and_then(|r| r.pat).is_some();
                let element_count = prefix.len() + suffix.len();
                let has_nested_tests = element_count > 0 || has_rest_test;
                let after_len = if has_nested_tests {
                    self.builder.create_block()
                } else {
                    success
                };
                self.lower_array_pattern_length_test(
                    scrutinee,
                    rest.is_some(),
                    prefix.len() + suffix.len(),
                    after_len,
                    failure,
                );
                if !has_nested_tests {
                    return;
                }

                self.builder.set_current_block(after_len);
                let rest_entry = has_rest_test.then(|| self.builder.create_block());
                let element_success = rest_entry.unwrap_or(success);
                if element_count == 0 {
                    self.builder.goto(element_success);
                }

                for (idx, elem_pat) in prefix.iter().copied().enumerate() {
                    let next = if idx + 1 == element_count {
                        element_success
                    } else {
                        self.builder.create_block()
                    };
                    let elem_local =
                        self.project_array_pattern_element_from_start(scrutinee, elem_pat, idx);
                    self.lower_pattern_test(elem_local, elem_pat, next, failure);
                    if idx + 1 < element_count {
                        self.builder.set_current_block(next);
                    }
                }

                for (suffix_idx, elem_pat) in suffix.iter().copied().enumerate() {
                    let absolute_idx_from_end = suffix.len() - suffix_idx;
                    let elem_idx = prefix.len() + suffix_idx;
                    let next = if elem_idx + 1 == element_count {
                        element_success
                    } else {
                        self.builder.create_block()
                    };
                    let elem_local = self.project_array_pattern_element_from_end(
                        scrutinee,
                        elem_pat,
                        absolute_idx_from_end,
                    );
                    self.lower_pattern_test(elem_local, elem_pat, next, failure);
                    if elem_idx + 1 < element_count {
                        self.builder.set_current_block(next);
                    }
                }

                if let Some(rest) = rest
                    && let Some(rest_pat) = rest.pat
                {
                    if let Some(rest_entry) = rest_entry {
                        self.builder.set_current_block(rest_entry);
                    }
                    let rest_local = self.project_array_pattern_rest(
                        scrutinee,
                        rest_pat,
                        prefix.len(),
                        suffix.len(),
                    );
                    self.lower_pattern_test(rest_local, rest_pat, success, failure);
                }
            }
        }
    }

    fn is_irrefutable_catch_all(&self, pat_id: AstPatId) -> bool {
        match &self.body.patterns[pat_id] {
            AstPattern::Wildcard => true,
            // `let x` is irrefutable; `let x: <pat>` is refutable iff
            // the inner sub-pattern is.
            AstPattern::Bind { subpat, .. } => match subpat {
                None => true,
                Some(sp) => self.is_irrefutable_catch_all(*sp),
            },
            AstPattern::Or(parts) => parts
                .iter()
                .any(|part| self.is_irrefutable_catch_all(*part)),
            AstPattern::Type(_) | AstPattern::Class { .. } | AstPattern::Array { .. } => false,
        }
    }

    /// Type ascription on the pattern, if any. For `let x: T` (where the
    /// sub-pattern is a `Type`), returns `T`. For `[…]: T` (Array with
    /// ascription), returns `T`. Returns `None` for everything else
    /// (including `let x: <non-type-pattern>`).
    fn pattern_narrow_type(&self, pat_id: AstPatId) -> Option<AstTypeExpr> {
        match &self.body.patterns[pat_id] {
            AstPattern::Bind {
                subpat: Some(sp), ..
            } => match &self.body.patterns[*sp] {
                AstPattern::Type(t) => Some(t.clone()),
                _ => None,
            },
            AstPattern::Array {
                ascription: Some(t),
                ..
            } => Some(t.clone()),
            _ => None,
        }
    }

    fn bind_pattern(&mut self, scrutinee: Local, pat_id: AstPatId) {
        // Pass the root pat_id through recursion: HIR registers bindings
        // keyed by the OUTER pattern PatId (the let-stmt's pattern, the
        // match-arm's pattern, etc.), never by the inner Bind. To wire up
        // closure capture lookups correctly, we register the local against
        // that root.
        self.bind_pattern_inner(scrutinee, pat_id, pat_id, pat_id, false, false);
    }

    fn bind_pattern_with_fresh_cells(&mut self, scrutinee: Local, pat_id: AstPatId) {
        self.bind_pattern_inner(scrutinee, pat_id, pat_id, pat_id, true, false);
    }

    fn bind_pattern_inner(
        &mut self,
        scrutinee: Local,
        pat_id: AstPatId,
        root: AstPatId,
        narrow_root: AstPatId,
        fresh_cell: bool,
        is_watched: bool,
    ) {
        match self.body.patterns[pat_id].clone() {
            AstPattern::Bind { name, subpat } => {
                // For Or-patterns we look up `pat_types` against the inner
                // bind's `pat_id`, not the outer `root`. That's safe because
                // TIR rejects Or-branches whose shared bindings disagree on
                // type (`OrPatternBindingTypeMismatch`), so by the time we
                // reach MIR every alternative's bind for `name` carries the
                // same type. If you ever loosen that TIR invariant, switch
                // this lookup to `root` so we don't over-narrow.
                let narrow = self.pattern_narrow_type(narrow_root);
                let ty = if let Some(narrow) = &narrow {
                    self.resolve_type_annotation(narrow)
                } else {
                    self.pat_types
                        .get(&self.pat_metadata_key(pat_id))
                        .map(|ty| convert_tir2_ty(ty, self.resolved_aliases))
                        .unwrap_or_else(|| self.builder.local_ty(scrutinee))
                };
                let local = self
                    .builder
                    .declare_local(Some(name.clone()), ty, None, is_watched);
                if fresh_cell {
                    self.builder.fresh_cell(local);
                }
                self.builder.assign(
                    Place::local(local),
                    Rvalue::Use(Operand::Copy(Place::Local(scrutinee))),
                );
                self.record_pattern_binding_local(root, &name, local);
                self.locals.insert(name, local);
                // Recurse into the sub-pattern so inner bindings (e.g.
                // `let x: let y` or `let x: Class { f }`) get emitted too.
                if let Some(sp) = subpat {
                    self.bind_pattern_inner(scrutinee, sp, root, sp, fresh_cell, is_watched);
                }
            }
            AstPattern::Or(parts) => {
                let mut bindings = Vec::new();
                self.collect_pattern_bindings(pat_id, &mut bindings);
                if bindings.is_empty() {
                    return;
                }
                self.declare_or_pattern_bindings(pat_id, root, fresh_cell, is_watched);
                self.lower_or_pattern_assign_existing(scrutinee, &parts, root, narrow_root);
            }
            AstPattern::Class { fields, .. } => {
                for f in fields {
                    if let Some(field_local) =
                        self.project_class_pattern_field(scrutinee, pat_id, f.pat, &f.field)
                    {
                        self.bind_pattern_inner(
                            field_local,
                            f.pat,
                            root,
                            f.pat,
                            fresh_cell,
                            is_watched,
                        );
                    }
                }
            }
            AstPattern::Array {
                prefix,
                rest,
                suffix,
                ascription: _,
            } => {
                for (idx, elem_pat) in prefix.iter().copied().enumerate() {
                    let elem_local =
                        self.project_array_pattern_element_from_start(scrutinee, elem_pat, idx);
                    self.bind_pattern_inner(
                        elem_local, elem_pat, root, elem_pat, fresh_cell, is_watched,
                    );
                }
                if let Some(rest) = rest
                    && let Some(rest_pat) = rest.pat
                {
                    let rest_local = self.project_array_pattern_rest(
                        scrutinee,
                        rest_pat,
                        prefix.len(),
                        suffix.len(),
                    );
                    self.bind_pattern_inner(
                        rest_local, rest_pat, root, rest_pat, fresh_cell, is_watched,
                    );
                }
                for (suffix_idx, elem_pat) in suffix.iter().copied().enumerate() {
                    let absolute_idx_from_end = suffix.len() - suffix_idx;
                    let elem_local = self.project_array_pattern_element_from_end(
                        scrutinee,
                        elem_pat,
                        absolute_idx_from_end,
                    );
                    self.bind_pattern_inner(
                        elem_local, elem_pat, root, elem_pat, fresh_cell, is_watched,
                    );
                }
            }
            AstPattern::Wildcard | AstPattern::Type(_) => {}
        }
    }

    fn collect_pattern_bindings(&self, pat_id: AstPatId, out: &mut Vec<(Name, AstPatId)>) {
        match self.body.patterns[pat_id].clone() {
            AstPattern::Bind { name, subpat } => {
                out.push((name, pat_id));
                if let Some(sp) = subpat {
                    self.collect_pattern_bindings(sp, out);
                }
            }
            AstPattern::Or(parts) => {
                if let Some(first) = parts.first() {
                    self.collect_pattern_bindings(*first, out);
                }
            }
            AstPattern::Class { fields, .. } => {
                for field in fields {
                    self.collect_pattern_bindings(field.pat, out);
                }
            }
            AstPattern::Array {
                prefix,
                rest,
                suffix,
                ascription: _,
            } => {
                for part in prefix {
                    self.collect_pattern_bindings(part, out);
                }
                if let Some(rest) = rest
                    && let Some(rest_pat) = rest.pat
                {
                    self.collect_pattern_bindings(rest_pat, out);
                }
                for part in suffix {
                    self.collect_pattern_bindings(part, out);
                }
            }
            AstPattern::Wildcard | AstPattern::Type(_) => {}
        }
    }

    fn declare_or_pattern_bindings(
        &mut self,
        pat_id: AstPatId,
        root: AstPatId,
        fresh_cell: bool,
        is_watched: bool,
    ) {
        let mut bindings = Vec::new();
        self.collect_pattern_bindings(pat_id, &mut bindings);
        for (name, bind_pat) in bindings {
            let local = self.builder.declare_local(
                Some(name.clone()),
                self.pat_ty(bind_pat),
                None,
                is_watched,
            );
            if fresh_cell {
                self.builder.fresh_cell(local);
            }
            self.record_pattern_binding_local(root, &name, local);
            self.locals.insert(name, local);
        }
    }

    fn lower_or_pattern_assign_existing(
        &mut self,
        scrutinee: Local,
        parts: &[AstPatId],
        root: AstPatId,
        narrow_root: AstPatId,
    ) {
        if parts.is_empty() {
            self.builder.unreachable();
            return;
        }

        let join = self.builder.create_block();
        let failure = self.builder.create_block();

        for (idx, part) in parts.iter().copied().enumerate() {
            let body = self.builder.create_block();
            let next = if idx + 1 == parts.len() {
                failure
            } else {
                self.builder.create_block()
            };
            self.lower_pattern_test(scrutinee, part, body, next);

            self.builder.set_current_block(body);
            self.assign_pattern_to_existing(scrutinee, part, root, narrow_root);
            if !self.builder.is_current_terminated() {
                self.builder.goto(join);
            }

            if idx + 1 < parts.len() {
                self.builder.set_current_block(next);
            }
        }

        self.builder.set_current_block(failure);
        self.builder.unreachable();
        self.builder.set_current_block(join);
    }

    fn assign_pattern_to_existing(
        &mut self,
        scrutinee: Local,
        pat_id: AstPatId,
        root: AstPatId,
        narrow_root: AstPatId,
    ) {
        match self.body.patterns[pat_id].clone() {
            AstPattern::Bind { name, .. } => {
                if let Some(&local) = self.locals.get(&name) {
                    self.builder.assign(
                        Place::local(local),
                        Rvalue::Use(Operand::Copy(Place::Local(scrutinee))),
                    );
                    self.record_pattern_binding_local(root, &name, local);
                }
            }
            AstPattern::Or(parts) => {
                self.lower_or_pattern_assign_existing(scrutinee, &parts, root, narrow_root);
            }
            AstPattern::Class { fields, .. } => {
                for field in fields {
                    if let Some(field_local) =
                        self.project_class_pattern_field(scrutinee, pat_id, field.pat, &field.field)
                    {
                        self.assign_pattern_to_existing(field_local, field.pat, root, field.pat);
                    }
                }
            }
            AstPattern::Array {
                prefix,
                rest,
                suffix,
                ascription: _,
            } => {
                for (idx, elem_pat) in prefix.iter().copied().enumerate() {
                    let elem_local =
                        self.project_array_pattern_element_from_start(scrutinee, elem_pat, idx);
                    self.assign_pattern_to_existing(elem_local, elem_pat, root, elem_pat);
                }
                if let Some(rest) = rest
                    && let Some(rest_pat) = rest.pat
                {
                    let rest_local = self.project_array_pattern_rest(
                        scrutinee,
                        rest_pat,
                        prefix.len(),
                        suffix.len(),
                    );
                    self.assign_pattern_to_existing(rest_local, rest_pat, root, rest_pat);
                }
                for (suffix_idx, elem_pat) in suffix.iter().copied().enumerate() {
                    let absolute_idx_from_end = suffix.len() - suffix_idx;
                    let elem_local = self.project_array_pattern_element_from_end(
                        scrutinee,
                        elem_pat,
                        absolute_idx_from_end,
                    );
                    self.assign_pattern_to_existing(elem_local, elem_pat, root, elem_pat);
                }
            }
            AstPattern::Wildcard | AstPattern::Type(_) => {}
        }
    }
}

// ─── Type tag classification (shared by match/catch switch dispatch) ──────────

impl LoweringContext<'_> {
    /// Classify a pattern into type tag value(s) for switch dispatch.
    /// Classify a pattern as type-tag-eligible and return its tag(s).
    ///
    /// Shared by match and catch lowering.
    ///
    /// Returns `Some(tags)` for `TypedBinding` and Binding-with-TIR-type patterns
    /// that resolve to primitive or class types. Returns `None` for literals,
    /// wildcards, enum variants, and types without tag mappings.
    fn classify_pattern_type_tag(&self, pat_id: AstPatId) -> Option<Vec<i64>> {
        let pat = &self.body.patterns[pat_id];
        if self.pattern_contains_structural(pat_id) {
            return None;
        }
        // A generic-interface pattern (`Slot<int>`, or one nested in a union /
        // optional like `Slot<int> | Slot<string>`) cannot be discriminated by a
        // flat type-tag switch: every instantiation shares the bare interface's
        // implementor tags, so arms would collide and the first would wrongly
        // capture all of them. Disqualify the switch (recursively) so the
        // match-chain runtime test — which filters implementors by the specific
        // instantiation — is used instead.
        if self
            .pat_types
            .get(&self.pat_metadata_key(pat_id))
            .is_some_and(Self::tir_ty_needs_interface_shape_test)
        {
            return None;
        }
        // Bind/Array patterns may carry a `:T` type ascription; resolve
        // via the ascription's TypeExpr if present. For Bind, the
        // ascription is the sub-pattern when it's a `Type(...)` shape.
        let ascription_ty = match pat {
            AstPattern::Bind {
                subpat: Some(sp), ..
            } => match &self.body.patterns[*sp] {
                AstPattern::Type(t) => Some(t.clone()),
                _ => None,
            },
            AstPattern::Array {
                ascription: Some(t),
                ..
            } => Some(t.clone()),
            _ => None,
        };
        if let Some(ty_expr) = ascription_ty {
            if let Some(tir_ty) = self.pat_types.get(&self.pat_metadata_key(pat_id)) {
                let resolved = self.resolved_aliases.convert(tir_ty);
                if let Some(tags) = self.ty_to_type_tags(&resolved) {
                    return Some(tags);
                }
            }
            let resolved = self.resolve_type_annotation(&ty_expr);
            return self.ty_to_type_tags(&resolved);
        }
        match pat {
            AstPattern::Wildcard => None,
            AstPattern::Bind { .. } => {
                let tir_ty = self.pat_types.get(&self.pat_metadata_key(pat_id))?;
                let resolved = self.resolved_aliases.convert(tir_ty);
                self.ty_to_type_tags(&resolved)
            }
            AstPattern::Type(_) => {
                if let Some(tir_ty) = self.pat_types.get(&self.pat_metadata_key(pat_id)) {
                    let resolved = self.resolved_aliases.convert(tir_ty);
                    if let Some(tags) = self.ty_to_type_tags(&resolved) {
                        return Some(tags);
                    }
                }
                if let AstPattern::Type(ty_expr) = pat {
                    let resolved = self.resolve_type_annotation(ty_expr);
                    return self.ty_to_type_tags(&resolved);
                }
                None
            }
            _ => None,
        }
    }

    /// Convert a `Ty` to the list of type tag integers it corresponds to.
    /// Returns `None` if the type has no simple tag representation.
    ///
    /// Supports primitives (globally-stable tags) and class types (looked up
    /// from `class_type_tags`). Union types are flattened — all members must
    /// be tag-eligible.
    fn ty_to_type_tags(&self, ty: &Ty) -> Option<Vec<i64>> {
        match ty {
            Ty::Union(members, _) => {
                let mut tags = Vec::new();
                for m in members {
                    let member_tags = self.ty_to_type_tags(m)?;
                    tags.extend(member_tags);
                }
                Some(tags)
            }
            _ => self.type_tag_for_ty(ty).map(|tag| vec![tag]),
        }
    }
}

/// Format a type tag integer as a human-readable name for switch arm debug metadata.
fn format_type_tag_name(tag: i64) -> String {
    match tag {
        baml_type::typetag::INT => "int".to_string(),
        baml_type::typetag::BIGINT => "bigint".to_string(),
        baml_type::typetag::STRING => "string".to_string(),
        baml_type::typetag::BOOL => "bool".to_string(),
        baml_type::typetag::NULL => "null".to_string(),
        baml_type::typetag::FLOAT => "float".to_string(),
        baml_type::typetag::LIST => "list".to_string(),
        baml_type::typetag::MAP => "map".to_string(),
        baml_type::typetag::ENUM => "enum".to_string(),
        baml_type::typetag::FUNCTION => "function".to_string(),
        baml_type::typetag::FUTURE => "future".to_string(),
        baml_type::typetag::TYPE => "type".to_string(),
        baml_type::typetag::COLLECTOR => "collector".to_string(),
        baml_type::typetag::UINT8ARRAY => "uint8array".to_string(),
        tag if tag >= baml_type::typetag::CLASS_BASE => {
            format!("class#{}", tag - baml_type::typetag::CLASS_BASE)
        }
        _ => format!("tag#{tag}"),
    }
}

// ─── Catch lowering ───────────────────────────────────────────────────────────

impl LoweringContext<'_> {
    fn lower_catch(
        &mut self,
        _expr_id: AstExprId,
        base: AstExprId,
        clauses: &[baml_compiler2_ast::CatchClause],
        dest: &Place,
    ) {
        use baml_compiler2_ast::CatchClauseKind;

        #[derive(Clone)]
        struct ClauseLocals {
            binding_name: Option<Name>,
            binding_local: Option<Local>,
            binding_copy_local: Option<Local>,
            stack_trace_name: Option<Name>,
            stack_trace_payload: Option<Local>,
            stack_trace_copy_local: Option<Local>,
        }

        fn install_clause_locals(
            ctx: &mut LoweringContext<'_>,
            error_local: Local,
            clause: &ClauseLocals,
        ) {
            if let (Some(name), Some(local)) = (&clause.binding_name, clause.binding_local) {
                ctx.locals.insert(name.clone(), local);
            }
            if let Some(binding_copy_local) = clause.binding_copy_local {
                ctx.builder.assign(
                    Place::local(binding_copy_local),
                    Rvalue::Use(Operand::Copy(Place::Local(error_local))),
                );
            }
            if let (Some(name), Some(local)) =
                (&clause.stack_trace_name, clause.stack_trace_copy_local)
            {
                ctx.locals.insert(name.clone(), local);
            }
            if let (Some(payload), Some(copy_local)) =
                (clause.stack_trace_payload, clause.stack_trace_copy_local)
                && payload != copy_local
            {
                ctx.builder.assign(
                    Place::local(copy_local),
                    Rvalue::Use(Operand::Copy(Place::Local(payload))),
                );
            }
        }

        let saved_catch_outer_locals = self.locals.clone();
        let bb_join = self.builder.create_block();
        let bb_handler = self.builder.create_block();

        // Use the user-provided binding name (e.g. `e` from `catch (e)`) so it
        // shows up in bytecode instead of an anonymous `_N` temp. Only do this
        // for single-clause catches with a non-captured binding.
        let single_clause_binding_name = clauses.first().and_then(|c| {
            if clauses.len() == 1 && !self.pattern_binding_is_captured(c.binding) {
                self.body.patterns[c.binding]
                    .binding_name(&self.body.patterns)
                    .cloned()
            } else {
                None
            }
        });
        let error_local = self.builder.declare_local(
            single_clause_binding_name,
            Ty::BuiltinUnknown {
                attr: TyAttr::default(),
            },
            None,
            false,
        );

        let stack_trace_local = clauses
            .iter()
            .any(|c| c.stack_trace_binding.is_some())
            .then(|| {
                self.builder.declare_local(
                    None,
                    Ty::BuiltinUnknown {
                        attr: TyAttr::default(),
                    },
                    None,
                    false,
                )
            });

        let mut clause_locals = Vec::with_capacity(clauses.len());
        for clause in clauses {
            let binding_name = self.body.patterns[clause.binding]
                .binding_name(&self.body.patterns)
                .cloned();
            let binding_is_captured = self.pattern_binding_is_captured(clause.binding);
            let (binding_local, binding_copy_local) = match binding_name.clone() {
                Some(name) if binding_is_captured => {
                    let local = self.builder.declare_local(
                        Some(name.clone()),
                        Ty::BuiltinUnknown {
                            attr: TyAttr::default(),
                        },
                        None,
                        false,
                    );
                    self.record_pattern_binding_local(clause.binding, &name, local);
                    (Some(local), Some(local))
                }
                Some(name) => {
                    self.record_pattern_binding_local(clause.binding, &name, error_local);
                    (Some(error_local), None)
                }
                None => (None, None),
            };

            let (stack_trace_name, stack_trace_copy_local) = if let (Some(st_pat), Some(payload)) =
                (clause.stack_trace_binding, stack_trace_local)
            {
                let name = self.body.patterns[st_pat]
                    .binding_name(&self.body.patterns)
                    .cloned();
                let is_captured = self.pattern_binding_is_captured(st_pat);
                match name.clone() {
                    Some(name) if is_captured => {
                        let local = self.builder.declare_local(
                            Some(name.clone()),
                            Ty::BuiltinUnknown {
                                attr: TyAttr::default(),
                            },
                            None,
                            false,
                        );
                        self.record_pattern_binding_local(st_pat, &name, local);
                        (Some(name), Some(local))
                    }
                    Some(name) => {
                        self.record_pattern_binding_local(st_pat, &name, payload);
                        (Some(name), Some(payload))
                    }
                    None => (None, None),
                }
            } else {
                (None, None)
            };

            clause_locals.push(ClauseLocals {
                binding_name,
                binding_local,
                binding_copy_local,
                stack_trace_name,
                stack_trace_payload: stack_trace_local,
                stack_trace_copy_local,
            });
        }

        // Flatten all arms from all clauses (blocks created lazily below).
        let mut arms: Vec<(baml_compiler2_ast::CatchArm, bool, usize)> = Vec::new();
        for (clause_idx, clause) in clauses.iter().enumerate() {
            for &arm_id in &clause.arms {
                let arm = self.body.catch_arms[arm_id].clone();
                let is_wildcard = self.is_irrefutable_catch_all(arm.pattern);
                arms.push((arm, is_wildcard, clause_idx));
            }
        }

        let has_wildcard = arms.iter().any(|(_, is_wc, _)| *is_wc);
        let is_catch_all_panics = clauses
            .iter()
            .any(|clause| matches!(clause.kind, CatchClauseKind::CatchAllPanics));

        // Record the catch region (always one handler, one exception table entry).
        let body_entry = self.builder.current_block();
        self.builder.catch_regions.push(CatchRegion {
            body_entry,
            handler: bb_handler,
            error_local,
            stack_trace_local,
        });

        let prev_catch = self.catch_context.take();
        self.catch_context = Some(CatchContext {
            unwind_target: bb_handler,
            error_local,
        });

        // Lower the try body.
        self.lower_expr(base, dest.clone());
        if !self.builder.is_current_terminated() {
            self.builder.goto(bb_join);
        }

        self.catch_context = prev_catch;

        // Before the wildcard arm (if any), insert a throw_if_panic guard to
        // prevent the wildcard from swallowing panics the programmer didn't
        // explicitly name. Skipped for catch_all_panics (user wants everything).
        let needs_throw_if_panic = has_wildcard && !is_catch_all_panics;

        // Attempt switch-style dispatch on type tags.
        // If all non-wildcard arms have pure type-test patterns, emit a single
        // Switch on Rvalue::TypeTag instead of a sequential is_type chain.
        let switch_arms: Vec<(AstPatId, AstExprId, Option<AstExprId>)> = arms
            .iter()
            .map(|(arm, _, _)| (arm.pattern, arm.body, None))
            .collect();
        self.builder.set_current_block(bb_handler);
        if clauses.len() == 1 {
            install_clause_locals(self, error_local, &clause_locals[0]);
        }
        if clauses.len() == 1
            && self.try_lower_as_switch(
                error_local,
                &switch_arms,
                dest.clone(),
                bb_join,
                SwitchOtherwise::Catch {
                    error_local,
                    needs_throw_if_panic,
                },
                None,
            )
        {
            self.builder.set_current_block(bb_join);
            self.restore_active_locals(saved_catch_outer_locals);
            return;
        }

        // Fallback: sequential pattern-test chain.
        // Create body blocks now (not created earlier so the switch path
        // doesn't leave orphaned unterminated blocks).
        let arms_with_blocks: Vec<_> = arms
            .iter()
            .map(|(arm, is_wc, clause_idx)| {
                (
                    arm.clone(),
                    self.builder.create_block(),
                    *is_wc,
                    *clause_idx,
                )
            })
            .collect();

        for &(ref arm, body_block, is_wildcard, _) in &arms_with_blocks {
            if is_wildcard && needs_throw_if_panic {
                let bb_wildcard = self.builder.create_block();
                self.builder
                    .throw_if_panic(Operand::Copy(Place::Local(error_local)), bb_wildcard);
                self.builder.set_current_block(bb_wildcard);
            }

            let bb_arm_next = self.builder.create_block();
            self.lower_pattern_test(error_local, arm.pattern, body_block, bb_arm_next);
            self.builder.set_current_block(bb_arm_next);
        }

        // Rethrow if nothing matched.
        if !self.builder.is_current_terminated() {
            self.builder.throw(Operand::Copy(Place::Local(error_local)));
        }

        // Lower each arm body.
        for &(ref arm, body_block, _, clause_idx) in &arms_with_blocks {
            self.builder.set_current_block(body_block);
            let saved_locals = self.locals.clone();
            let watched_depth = self.watched_locals_stack.len();
            let clause = clause_locals[clause_idx].clone();
            install_clause_locals(self, error_local, &clause);
            self.bind_pattern(error_local, arm.pattern);
            self.lower_expr(arm.body, dest.clone());
            if !self.builder.is_current_terminated() {
                // A `watch let` declared inside a catch-arm body must be
                // torn down on fallthrough.
                self.emit_unwatch_to_depth(watched_depth);
                self.builder.goto(bb_join);
            }
            // Restore name→local map AND truncate the watched stack back to
            // the arm-entry depth (mirrors `lower_scoped_block`).
            self.restore_locals_after_scope(saved_locals, watched_depth);
        }

        self.builder.set_current_block(bb_join);
        self.restore_active_locals(saved_catch_outer_locals);
    }
}

// ─── 3.7: Entry points ────────────────────────────────────────────────────────

/// Lower a top-level let binding's initializer into a `MirFunctionBody`.
///
/// The body has arity 0 and contains only the initializer expression.
/// Used by `compile_init_function` in the emit crate to compile let initializers
/// into bytecode for the `$init` function.
pub fn lower_let_body<'db>(
    db: &'db dyn crate::Db,
    let_loc: LetLoc<'db>,
    opt: crate::OptLevel,
) -> Option<(MirFunctionBody, Vec<MirFunction>)> {
    let body = let_body(db, let_loc);
    let source_map = let_body_source_map(db, let_loc);

    match body.as_ref() {
        LetBody::Expr(expr_body) => {
            let mut ctx =
                LoweringContext::new_for_let(db, let_loc, expr_body.clone(), source_map, opt);
            let mir_body = ctx.lower_let_body_inner();
            let lambdas = std::mem::take(&mut ctx.pending_lambdas);
            Some((mir_body, lambdas))
        }
        LetBody::Missing => None,
    }
}

pub fn lower_function<'db>(
    db: &'db dyn crate::Db,
    func_loc: FunctionLoc<'db>,
    opt: crate::OptLevel,
) -> MirFunction {
    let body = baml_compiler2_ppir::function_body(db, func_loc);
    let source_map = baml_compiler2_ppir::function_body_source_map(db, func_loc);
    let item_ref = def_to_item_ref(
        db,
        baml_compiler2_hir::contributions::Definition::Function(func_loc),
    );
    let sig = baml_compiler2_ppir::function_signature(db, func_loc);
    let arity = sig.params.len();

    match body.as_ref() {
        FunctionBody::Expr(expr_body) => {
            let mut ctx = LoweringContext::new(db, func_loc, expr_body.clone(), source_map, opt);
            let mut mir = ctx.lower_function_body();
            mir.item_ref = item_ref;
            mir
        }
        FunctionBody::Builtin(kind) => {
            use baml_compiler2_ast::BuiltinKind;
            // For IO builtins (`$rust_io_function`), the compiler injects one
            // synthetic trailing value-arg slot for each generic type parameter
            // (e.g. `parse<T>` gets one extra `baml_type::Ty` slot after the
            // regular params).  We must include those synthetic slots in the
            // arity so that `ScheduleFuture` pops the correct number of args
            // from the stack.
            let extra_arity = if matches!(kind, BuiltinKind::Io) {
                // For IO builtins (`$rust_io_function`), the compiler injects
                // one synthetic trailing value-arg slot for each *function-level*
                // generic type parameter.  Class-level generics (from the
                // enclosing class definition) do NOT generate extra slots —
                // `baml_builtins2_codegen` only adds type-arg params for
                // function-level generics.  We therefore only count the
                // function's own generic_params here.
                let item_tree = file_item_tree(db, func_loc.file(db));
                item_tree[func_loc.id(db)].generic_params.len()
            } else {
                0
            };
            MirFunction {
                arity: arity + extra_arity,
                span: None,
                item_ref,
                kind: MirFunctionKind::Builtin(*kind),
                lambdas: vec![],
            }
        }
        FunctionBody::Missing => MirFunction {
            arity,
            span: None,
            item_ref,
            kind: MirFunctionKind::Bytecode(MirFunctionBody {
                blocks: vec![BasicBlock {
                    id: BlockId(0),
                    statements: vec![],
                    terminator: Some(Terminator::Unreachable),
                    span: None,
                    terminator_span: None,
                }],
                entry: BlockId(0),
                locals: (0..=arity)
                    .map(|_| LocalDecl {
                        name: None,
                        ty: baml_type::Ty::Void {
                            attr: baml_type::TyAttr::default(),
                        },
                        is_captured: false,
                        span: None,
                        scope_span: None,
                        is_watched: false,
                    })
                    .collect(),
                catch_regions: vec![],
                viz_nodes: vec![],
            }),
            lambdas: vec![],
        },
    }
}

#[cfg(test)]
mod tests {
    use baml_compiler2_tir::ty::{MediaKind, PrimitiveType};

    use super::*;

    fn type_var(name: &str) -> Tir2Ty {
        Tir2Ty::TypeVar(Name::new(name), baml_compiler2_tir::ty::TyAttr::default())
    }

    fn primitive(primitive: &PrimitiveType) -> Tir2Ty {
        let attr = baml_compiler2_tir::ty::TyAttr::default();
        match primitive {
            PrimitiveType::Int => Tir2Ty::Int { attr },
            PrimitiveType::Bigint => Tir2Ty::Bigint { attr },
            PrimitiveType::Float => Tir2Ty::Float { attr },
            PrimitiveType::String => Tir2Ty::String { attr },
            PrimitiveType::Bool => Tir2Ty::Bool { attr },
            PrimitiveType::Null => Tir2Ty::Null { attr },
            PrimitiveType::Uint8Array => Tir2Ty::Uint8Array { attr },
            PrimitiveType::Image => Tir2Ty::Media(MediaKind::Image, attr),
            PrimitiveType::Audio => Tir2Ty::Media(MediaKind::Audio, attr),
            PrimitiveType::Video => Tir2Ty::Media(MediaKind::Video, attr),
            PrimitiveType::Pdf => Tir2Ty::Media(MediaKind::Pdf, attr),
        }
    }

    #[test]
    fn interface_tir_type_args_match_preserves_type_var_identity() {
        let aliases = HashMap::new();

        assert!(interface_tir_type_args_match_preserving_typevars(
            &[type_var("L"), type_var("R")],
            &[type_var("L"), type_var("R")],
            &aliases,
        ));
        assert!(!interface_tir_type_args_match_preserving_typevars(
            &[type_var("L"), type_var("R")],
            &[type_var("R"), type_var("L")],
            &aliases,
        ));
    }

    #[test]
    fn interface_class_guard_checks_assoc_when_request_omits_generic_args() {
        let aliases = HashMap::new();
        let impl_args = vec![primitive(&PrimitiveType::String)];
        let requested_args = Vec::new();
        let requested_assoc = vec![(Name::new("Value"), primitive(&PrimitiveType::Int))];

        let int_impl_assoc = vec![(Name::new("Value"), primitive(&PrimitiveType::Int))];
        assert!(matches!(
            interface_class_guard_for_args(
                &impl_args,
                &int_impl_assoc,
                &requested_args,
                &requested_assoc,
                &[],
                &aliases,
            ),
            Some(InterfaceClassGuard::Any)
        ));

        let string_impl_assoc = vec![(Name::new("Value"), primitive(&PrimitiveType::String))];
        assert!(
            interface_class_guard_for_args(
                &impl_args,
                &string_impl_assoc,
                &requested_args,
                &requested_assoc,
                &[],
                &aliases,
            )
            .is_none()
        );
    }
}

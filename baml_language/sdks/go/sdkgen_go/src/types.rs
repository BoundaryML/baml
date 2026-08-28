//! The single semantic projection from compiler-owned [`CodegenTy`] values to
//! Go types.
//!
//! Renderers, codecs, documentation, and support filtering consume this graph
//! instead of independently interpreting BAML unions, aliases, literals, or
//! containers. `CodegenTy::canonicalize` has already recursively flattened
//! syntactic unions, removed exact duplicates, collapsed singletons, and moved
//! null last. This layer adds the pool-aware invariants that are necessarily
//! generator-specific: transparent alias expansion, attribute-free structural
//! identity, order-independent union interning, and the typed/dynamic threshold.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use baml_base::{
    Literal, MediaKind, Name as BaseName,
    qualified_name::{AI_FUNCTION_SPEC, AI_STREAM_STREAM, BAML_JSON_JSON},
};
use baml_codegen_types::{CodegenFunctionParamMode, Name, Symbol, SymbolPool, Ty};

/// A canonical, attribute-free Go type identity.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) enum GoTy {
    String,
    Int,
    Bigint,
    Float,
    Bool,
    Null,
    Uint8Array,
    Media(MediaKind),
    /// The exact recursive stdlib alias `baml.json.json`. Its Go surface is
    /// `any`, but unlike a dynamic union its wire decoder accepts only the
    /// canonical JSON value algebra.
    Json,
    /// A first-class reflected BAML type value (`type`). Its Go surface is the
    /// opaque runtime descriptor `baml_go.BAMLType`.
    ReflectedType,
    /// Opaque Rust-managed state (`$rust_type`). The native handle table owns
    /// the underlying value; Go only holds a cloneable lifetime token.
    RustType,
    FunctionSpec {
        output: Box<Self>,
    },
    Stream {
        partial: Box<Self>,
        final_: Box<Self>,
    },
    /// A Go type parameter projected from the compiler-owned BAML `TypeVar`.
    TypeVar(BaseName),
    Literal(GoLiteral),
    Class(Name, Box<[Self]>),
    Enum(Name),
    /// A narrowed enum variant. It renders as the owning Go enum but remains
    /// semantically distinct for descriptors and generic reifiability.
    EnumVariant(Name, BaseName),
    List(Box<Self>),
    Map {
        key: Box<Self>,
        value: Box<Self>,
    },
    Optional(Box<Self>),
    Function(GoFunctionKey),
    TypedUnion(GoUnionKey),
    /// Renders as `any`, but deliberately retains every canonical candidate.
    DynamicUnion {
        key: GoUnionKey,
        nullable: bool,
    },
    Unsupported,
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) enum GoLiteral {
    String(String),
    Int(i64),
    Bigint(String),
    Float(String),
    Bool(bool),
}

/// The identity shared by every occurrence of one structural union shape.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct GoUnionKey(Box<[GoTy]>);

impl GoUnionKey {
    pub(crate) fn members(&self) -> &[GoTy] {
        &self.0
    }
}

/// The structural identity of a callback exposed to Go.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct GoFunctionKey {
    params: Box<[GoFunctionParamKey]>,
    ret: Option<Box<GoTy>>,
    throws: bool,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) enum GoFunctionParamMode {
    Required,
    Optional,
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct GoFunctionParamKey {
    /// Required parameters are positional and deliberately carry no name in
    /// structural identity. Optional parameters are dispatched by this exact
    /// compiler-owned BAML name (or the VM's canonical `arg{index}` fallback).
    name: Option<BaseName>,
    ty: GoTy,
    mode: GoFunctionParamMode,
}

impl GoFunctionParamKey {
    pub(crate) fn name(&self) -> Option<&BaseName> {
        self.name.as_ref()
    }

    pub(crate) fn ty(&self) -> &GoTy {
        &self.ty
    }

    pub(crate) fn mode(&self) -> GoFunctionParamMode {
        self.mode
    }
}

impl GoFunctionKey {
    pub(crate) fn params(&self) -> &[GoFunctionParamKey] {
        &self.params
    }

    pub(crate) fn required_params(&self) -> impl Iterator<Item = &GoFunctionParamKey> {
        self.params
            .iter()
            .filter(|param| param.mode == GoFunctionParamMode::Required)
    }

    pub(crate) fn optional_params(&self) -> impl Iterator<Item = &GoFunctionParamKey> {
        self.params
            .iter()
            .filter(|param| param.mode == GoFunctionParamMode::Optional)
    }

    pub(crate) fn has_optional_params(&self) -> bool {
        self.params
            .iter()
            .any(|param| param.mode == GoFunctionParamMode::Optional)
    }

    pub(crate) fn ret(&self) -> Option<&GoTy> {
        self.ret.as_deref()
    }

    pub(crate) fn throws(&self) -> bool {
        self.throws
    }
}

pub(crate) struct GoTypeProjection<'a> {
    pool: &'a SymbolPool,
    max_typed_union_arity: usize,
    /// Typed unions used by each generated Go package. Placement is a usage
    /// property; identity is exclusively [`GoUnionKey`].
    typed_unions: BTreeMap<BaseName, BTreeSet<GoUnionKey>>,
    callback_options: BTreeMap<BaseName, BTreeSet<GoFunctionKey>>,
}

impl<'a> GoTypeProjection<'a> {
    pub(crate) fn new(pool: &'a SymbolPool, max_typed_union_arity: usize) -> Self {
        let mut projection = Self {
            pool,
            max_typed_union_arity,
            typed_unions: BTreeMap::new(),
            callback_options: BTreeMap::new(),
        };
        projection.collect();
        projection
    }

    pub(crate) fn project(&self, ty: &Ty) -> GoTy {
        self.project_inner(ty, &mut HashSet::new())
    }

    pub(crate) fn typed_unions_in(&self, package: &BaseName) -> impl Iterator<Item = &GoUnionKey> {
        self.typed_unions
            .get(package)
            .into_iter()
            .flat_map(|unions| unions.iter())
    }

    pub(crate) fn callback_options_in(
        &self,
        package: &BaseName,
    ) -> impl Iterator<Item = &GoFunctionKey> {
        self.callback_options
            .get(package)
            .into_iter()
            .flat_map(|callbacks| callbacks.iter())
            .filter(|key| key.has_optional_params())
    }

    fn collect(&mut self) {
        fn function_tys(function: &baml_codegen_types::Function) -> Vec<&Ty> {
            let mut tys = function
                .arguments
                .iter()
                .map(|argument| &argument.ty)
                .chain(std::iter::once(&function.return_type))
                .collect::<Vec<_>>();
            if let Some(spec) = &function.operations.spec {
                tys.push(&spec.return_type);
            }
            if let Some(stream) = &function.operations.stream {
                tys.extend([&stream.return_type, &stream.partial_type, &stream.item_type]);
                tys.extend(stream.control_arguments.iter().map(|argument| &argument.ty));
            }
            tys
        }

        let symbols = self
            .pool
            .iter()
            .map(|(name, symbol)| (name.clone(), symbol))
            .collect::<Vec<_>>();
        for (name, symbol) in symbols {
            let tys: Vec<&Ty> = match &symbol {
                Symbol::Function(function) => function_tys(function),
                Symbol::Class(class) => {
                    let mut tys = class
                        .properties
                        .iter()
                        .map(|field| &field.ty)
                        .collect::<Vec<_>>();
                    for method in class.static_methods.iter().chain(&class.instance_methods) {
                        tys.extend(function_tys(method));
                    }
                    tys
                }
                Symbol::TypeAlias(alias) => vec![&alias.resolves_to],
                Symbol::Enum(_) => Vec::new(),
            };
            let mut found = BTreeSet::new();
            let mut callbacks = BTreeSet::new();
            for ty in tys {
                let projected = self.project(ty);
                collect_typed_unions(&projected, &mut found);
                collect_callback_options(&projected, &mut callbacks);
            }
            self.typed_unions
                .entry(name.package().clone())
                .or_default()
                .extend(found);
            self.callback_options
                .entry(name.package().clone())
                .or_default()
                .extend(callbacks);
        }
    }

    fn project_inner(&self, ty: &Ty, aliases: &mut HashSet<Name>) -> GoTy {
        match ty {
            Ty::String { .. } => GoTy::String,
            Ty::Int { .. } => GoTy::Int,
            Ty::Bigint { .. } => GoTy::Bigint,
            Ty::Float { .. } => GoTy::Float,
            Ty::Bool { .. } => GoTy::Bool,
            Ty::Null { .. } => GoTy::Null,
            Ty::Uint8Array { .. } => GoTy::Uint8Array,
            Ty::Media(MediaKind::Generic, _) => GoTy::Unsupported,
            Ty::Media(kind, _) => GoTy::Media(*kind),
            Ty::Type { .. } => GoTy::ReflectedType,
            Ty::RustType { .. } => GoTy::RustType,
            Ty::Literal(literal, ..) => GoTy::Literal(match literal {
                Literal::String(value) => GoLiteral::String(value.clone()),
                Literal::Int(value) => GoLiteral::Int(*value),
                Literal::Bigint(value) => GoLiteral::Bigint(value.to_string()),
                Literal::Float(value) => GoLiteral::Float(value.clone()),
                Literal::Bool(value) => GoLiteral::Bool(*value),
            }),
            Ty::TypeVar(param, _) => GoTy::TypeVar(param.name().clone()),
            Ty::Class(name, arguments, _) => {
                if is_reflect_kind_type(name) {
                    GoTy::ReflectedType
                } else if name.to_string() == AI_FUNCTION_SPEC && arguments.len() == 1 {
                    GoTy::FunctionSpec {
                        output: Box::new(self.project_inner(&arguments[0], aliases)),
                    }
                } else if name.to_string() == AI_STREAM_STREAM && arguments.len() == 2 {
                    GoTy::Stream {
                        partial: Box::new(self.project_inner(&arguments[0], aliases)),
                        final_: Box::new(self.project_inner(&arguments[1], aliases)),
                    }
                } else {
                    GoTy::Class(
                        name.clone(),
                        arguments
                            .iter()
                            .map(|argument| self.project_inner(argument, aliases))
                            .collect::<Vec<_>>()
                            .into_boxed_slice(),
                    )
                }
            }
            Ty::Enum(name, _) => GoTy::Enum(name.clone()),
            Ty::EnumVariant(name, variant, _) => GoTy::EnumVariant(name.clone(), variant.clone()),
            Ty::List(inner, _) => GoTy::List(Box::new(self.project_inner(inner, aliases))),
            Ty::Map { key, value, .. } => GoTy::Map {
                key: Box::new(self.project_inner(key, aliases)),
                value: Box::new(self.project_inner(value, aliases)),
            },
            Ty::TypeAlias(name, _) => {
                if is_baml_json(name) {
                    return GoTy::Json;
                }
                if !aliases.insert(name.clone()) {
                    return GoTy::Unsupported;
                }
                let projected = match &self.pool[name] {
                    Symbol::TypeAlias(alias) if !alias.recursive => {
                        self.project_inner(&alias.resolves_to, aliases)
                    }
                    _ => GoTy::Unsupported,
                };
                aliases.remove(name);
                projected
            }
            Ty::Union(members, _) => self.project_union(members, aliases),
            Ty::Function {
                params,
                ret,
                throws,
                ..
            } => {
                let throws = if matches!(throws.as_ref(), Ty::Never { .. })
                    || matches!(
                        throws.as_ref(),
                        Ty::TypeVar(name, _) if name.as_str().starts_with("__effect_param_")
                    ) {
                    false
                } else if self.throws_accepts_host_callable(throws, aliases) {
                    true
                } else {
                    // A Go `error` is encoded as baml.errors.HostCallable. Do
                    // not generate a signature that falsely promises it can
                    // construct an arbitrary declared BAML error class.
                    return GoTy::Unsupported;
                };
                let params = params
                    .iter()
                    .enumerate()
                    .map(|(index, param)| {
                        let mode = match param.mode {
                            CodegenFunctionParamMode::Required => GoFunctionParamMode::Required,
                            CodegenFunctionParamMode::Optional => GoFunctionParamMode::Optional,
                        };
                        GoFunctionParamKey {
                            name: (mode == GoFunctionParamMode::Optional).then(|| {
                                param
                                    .name
                                    .clone()
                                    .unwrap_or_else(|| BaseName::new(format!("arg{index}")))
                            }),
                            ty: self.project_inner(&param.ty, aliases),
                            mode,
                        }
                    })
                    .collect::<Vec<_>>();
                let ret = match ret.as_ref() {
                    Ty::Void { .. } | Ty::Never { .. } => None,
                    ret => Some(Box::new(self.project_inner(ret, aliases))),
                };
                if params
                    .iter()
                    .any(|param| callback_component_is_unsupported(&param.ty))
                    || ret
                        .as_deref()
                        .is_some_and(callback_component_is_unsupported)
                {
                    GoTy::Unsupported
                } else {
                    GoTy::Function(GoFunctionKey {
                        params: params.into_boxed_slice(),
                        ret,
                        throws,
                    })
                }
            }
            _ => GoTy::Unsupported,
        }
    }

    fn throws_accepts_host_callable(&self, ty: &Ty, aliases: &mut HashSet<Name>) -> bool {
        match ty {
            Ty::Class(name, arguments, _) => {
                arguments.is_empty()
                    && name
                        == &Name::new(
                            BaseName::new("baml"),
                            vec![BaseName::new("errors")],
                            BaseName::new("HostCallable"),
                        )
            }
            Ty::Union(members, _) => members
                .iter()
                .any(|member| self.throws_accepts_host_callable(member, aliases)),
            Ty::TypeAlias(name, _) => {
                if !aliases.insert(name.clone()) {
                    return false;
                }
                let accepts = match &self.pool[name] {
                    Symbol::TypeAlias(alias) if !alias.recursive => {
                        self.throws_accepts_host_callable(&alias.resolves_to, aliases)
                    }
                    _ => false,
                };
                aliases.remove(name);
                accepts
            }
            _ => false,
        }
    }

    fn project_union(&self, members: &[Ty], aliases: &mut HashSet<Name>) -> GoTy {
        let mut projected = Vec::new();
        for member in members {
            self.project_union_member(member, aliases, &mut projected);
        }
        if projected.iter().any(contains_unsupported) {
            return GoTy::Unsupported;
        }

        // Go identity intentionally ignores BAML member order and attributes.
        projected.sort();
        projected.dedup();
        let nullable = projected
            .iter()
            .position(|member| member == &GoTy::Null)
            .map(|index| {
                projected.remove(index);
                true
            })
            .unwrap_or(false);

        // `baml.json.json` already contains null. Avoid projecting
        // `baml.json.json | null` as `*any`, which would both lose the direct
        // Go value surface and distinguish two semantically identical types.
        let nullable = nullable && !projected.contains(&GoTy::Json);
        // Only a direct JSON arm requires the dynamic representation. Classes
        // and containers still have finite selected-arm descriptors; their
        // nested JSON values are validated by the registered class/container
        // codecs and by BAML's assignability check. Looking through classes
        // here would also make representation depend on implementation detail
        // and can recurse forever through cyclic class graphs.
        let contains_json = projected.contains(&GoTy::Json);
        let contains_type_var = projected.iter().any(contains_type_var);

        let inner = match projected.len() {
            0 => GoTy::Null,
            1 => projected.pop().expect("one projected union member"),
            // The recursive JSON alias has no finite selected-arm descriptor,
            // so a union containing it cannot use the closed-union ABI even
            // when its arity is below the configured threshold. Preserve the
            // canonical candidate metadata but use the established dynamic
            // `any` representation and let BAML validate assignability.
            arity
                if arity <= self.max_typed_union_arity && !contains_json && !contains_type_var =>
            {
                GoTy::TypedUnion(GoUnionKey(projected.into_boxed_slice()))
            }
            _ => GoTy::DynamicUnion {
                key: GoUnionKey(projected.into_boxed_slice()),
                nullable,
            },
        };

        if !nullable || matches!(inner, GoTy::DynamicUnion { .. } | GoTy::Null) {
            inner
        } else {
            GoTy::Optional(Box::new(inner))
        }
    }

    fn project_union_member(
        &self,
        member: &Ty,
        aliases: &mut HashSet<Name>,
        projected: &mut Vec<GoTy>,
    ) {
        match member {
            Ty::Union(nested, _) => {
                for member in nested {
                    self.project_union_member(member, aliases, projected);
                }
            }
            Ty::TypeAlias(name, _) => {
                if is_baml_json(name) {
                    projected.push(GoTy::Json);
                    return;
                }
                if !aliases.insert(name.clone()) {
                    projected.push(GoTy::Unsupported);
                    return;
                }
                match &self.pool[name] {
                    Symbol::TypeAlias(alias) if !alias.recursive => {
                        let start = projected.len();
                        self.project_union_member(&alias.resolves_to, aliases, projected);
                        // A user alias that eventually names the recursive
                        // stdlib JSON alias stays nominal in RuntimeTy. The
                        // current dynamic-union ABI carries no alias resolver
                        // or selected-arm descriptor, so generating this
                        // composition would compile but fail every JSON call.
                        // Omit it until the ABI can canonicalize recursive
                        // alias identity across the boundary.
                        if projected[start..].contains(&GoTy::Json) {
                            projected.truncate(start);
                            projected.push(GoTy::Unsupported);
                        }
                    }
                    _ => projected.push(GoTy::Unsupported),
                }
                aliases.remove(name);
            }
            other => projected.push(self.project_inner(other, aliases)),
        }
    }
}

fn is_reflect_kind_type(name: &Name) -> bool {
    matches!(
        name.namespace().as_slice(),
        [reflect, kind]
            if reflect.as_str() == "reflect"
                && matches!(
                    kind.as_str(),
                    "array"
                        | "class"
                        | "enum"
                        | "function"
                        | "interface"
                        | "literal"
                        | "map"
                        | "primitive"
                        | "union"
                )
    ) && name.package().as_str() == "baml"
        && name.name().as_str() == "Type"
}

fn is_baml_json(name: &Name) -> bool {
    // Parse the canonical compiler-owned FQN into the typed identity rather
    // than recognizing arbitrary recursive aliases by their expansion.
    name == &Name::from_dotted_path(BAML_JSON_JSON)
}

fn collect_typed_unions(ty: &GoTy, found: &mut BTreeSet<GoUnionKey>) {
    match ty {
        GoTy::TypedUnion(key) => {
            if found.insert(key.clone()) {
                for member in key.members() {
                    collect_typed_unions(member, found);
                }
            }
        }
        GoTy::DynamicUnion { key, .. } => {
            // A dynamic outer union may contain typed unions in container arms.
            for member in key.members() {
                collect_typed_unions(member, found);
            }
        }
        GoTy::Class(_, arguments) => {
            for argument in arguments {
                collect_typed_unions(argument, found);
            }
        }
        GoTy::FunctionSpec { output } => collect_typed_unions(output, found),
        GoTy::Stream { partial, final_ } => {
            collect_typed_unions(partial, found);
            collect_typed_unions(final_, found);
        }
        GoTy::List(inner) | GoTy::Optional(inner) => collect_typed_unions(inner, found),
        GoTy::Function(key) => {
            for param in key.params() {
                collect_typed_unions(param.ty(), found);
            }
            if let Some(ret) = key.ret() {
                collect_typed_unions(ret, found);
            }
        }
        GoTy::Map { key, value } => {
            collect_typed_unions(key, found);
            collect_typed_unions(value, found);
        }
        _ => {}
    }
}

fn collect_callback_options(ty: &GoTy, found: &mut BTreeSet<GoFunctionKey>) {
    match ty {
        GoTy::Function(key) => {
            if key.has_optional_params() && found.insert(key.clone()) {
                for param in key.params() {
                    collect_callback_options(param.ty(), found);
                }
                if let Some(ret) = key.ret() {
                    collect_callback_options(ret, found);
                }
            }
        }
        GoTy::Class(_, arguments) => {
            for argument in arguments {
                collect_callback_options(argument, found);
            }
        }
        GoTy::FunctionSpec { output } => collect_callback_options(output, found),
        GoTy::Stream { partial, final_ } => {
            collect_callback_options(partial, found);
            collect_callback_options(final_, found);
        }
        GoTy::List(inner) | GoTy::Optional(inner) => collect_callback_options(inner, found),
        GoTy::Map { key, value } => {
            collect_callback_options(key, found);
            collect_callback_options(value, found);
        }
        GoTy::TypedUnion(key) | GoTy::DynamicUnion { key, .. } => {
            for member in key.members() {
                collect_callback_options(member, found);
            }
        }
        _ => {}
    }
}

fn contains_unsupported(ty: &GoTy) -> bool {
    match ty {
        GoTy::Unsupported => true,
        GoTy::Class(_, arguments) => arguments.iter().any(contains_unsupported),
        GoTy::FunctionSpec { output } => contains_unsupported(output),
        GoTy::Stream { partial, final_ } => {
            contains_unsupported(partial) || contains_unsupported(final_)
        }
        GoTy::List(inner) | GoTy::Optional(inner) => contains_unsupported(inner),
        GoTy::Map { key, value } => contains_unsupported(key) || contains_unsupported(value),
        GoTy::TypedUnion(key) | GoTy::DynamicUnion { key, .. } => {
            key.members().iter().any(contains_unsupported)
        }
        GoTy::Function(key) => {
            key.params()
                .iter()
                .any(|param| contains_unsupported(param.ty()))
                || key.ret().is_some_and(contains_unsupported)
        }
        _ => false,
    }
}

fn contains_type_var(ty: &GoTy) -> bool {
    match ty {
        GoTy::TypeVar(_) => true,
        GoTy::Class(_, arguments) => arguments.iter().any(contains_type_var),
        GoTy::FunctionSpec { output } => contains_type_var(output),
        GoTy::Stream { partial, final_ } => contains_type_var(partial) || contains_type_var(final_),
        GoTy::List(inner) | GoTy::Optional(inner) => contains_type_var(inner),
        GoTy::Map { key, value } => contains_type_var(key) || contains_type_var(value),
        GoTy::TypedUnion(key) | GoTy::DynamicUnion { key, .. } => {
            key.members().iter().any(contains_type_var)
        }
        GoTy::Function(key) => {
            key.params()
                .iter()
                .any(|parameter| contains_type_var(parameter.ty()))
                || key.ret().is_some_and(contains_type_var)
        }
        _ => false,
    }
}

fn callback_component_is_unsupported(ty: &GoTy) -> bool {
    contains_unsupported(ty)
        || contains_function(ty)
        || contains_dynamic_union(ty)
        || contains_type_var(ty)
}

fn contains_function(ty: &GoTy) -> bool {
    match ty {
        GoTy::Function(_) => true,
        GoTy::Class(_, arguments) => arguments.iter().any(contains_function),
        GoTy::FunctionSpec { output } => contains_function(output),
        GoTy::Stream { partial, final_ } => contains_function(partial) || contains_function(final_),
        GoTy::List(inner) | GoTy::Optional(inner) => contains_function(inner),
        GoTy::Map { key, value } => contains_function(key) || contains_function(value),
        GoTy::TypedUnion(key) | GoTy::DynamicUnion { key, .. } => {
            key.members().iter().any(contains_function)
        }
        _ => false,
    }
}

fn contains_dynamic_union(ty: &GoTy) -> bool {
    match ty {
        GoTy::DynamicUnion { .. } => true,
        GoTy::TypedUnion(key) => key.members().iter().any(contains_dynamic_union),
        GoTy::Class(_, arguments) => arguments.iter().any(contains_dynamic_union),
        GoTy::FunctionSpec { output } => contains_dynamic_union(output),
        GoTy::Stream { partial, final_ } => {
            contains_dynamic_union(partial) || contains_dynamic_union(final_)
        }
        GoTy::List(inner) | GoTy::Optional(inner) => contains_dynamic_union(inner),
        GoTy::Map { key, value } => contains_dynamic_union(key) || contains_dynamic_union(value),
        GoTy::Function(key) => {
            key.params()
                .iter()
                .any(|param| contains_dynamic_union(param.ty()))
                || key.ret().is_some_and(contains_dynamic_union)
        }
        _ => false,
    }
}

pub(crate) fn literal_surface(literal: &GoLiteral) -> GoTy {
    match literal {
        GoLiteral::String(_) => GoTy::String,
        GoLiteral::Int(_) => GoTy::Int,
        GoLiteral::Bigint(_) => GoTy::Bigint,
        GoLiteral::Float(_) => GoTy::Float,
        GoLiteral::Bool(_) => GoTy::Bool,
    }
}

#[cfg(test)]
mod tests {
    use baml_codegen_types::{CallableParam, Origin, TypeAlias};
    use baml_type::{Freshness, TyAttr};

    use super::*;

    fn a() -> TyAttr {
        TyAttr::default()
    }

    fn union(members: Vec<Ty>) -> Ty {
        Ty::Union(members, a()).canonicalize()
    }

    fn alias_name(value: &str) -> Name {
        Name::new(BaseName::new("user"), vec![], BaseName::new(value))
    }

    fn callable(params: Vec<CallableParam>, ret: Ty, throws: Ty) -> Ty {
        Ty::Function {
            params,
            ret: Box::new(ret),
            throws: Box::new(throws),
            attr: a(),
        }
    }

    fn callable_param(mode: CodegenFunctionParamMode, ty: Ty) -> CallableParam {
        CallableParam {
            name: None,
            ty,
            mode,
        }
    }

    fn named_callable_param(name: &str, mode: CodegenFunctionParamMode, ty: Ty) -> CallableParam {
        CallableParam {
            name: Some(BaseName::new(name)),
            ty,
            mode,
        }
    }

    fn host_callable_error() -> Ty {
        Ty::Class(
            Name::new(
                BaseName::new("baml"),
                vec![BaseName::new("errors")],
                BaseName::new("HostCallable"),
            ),
            vec![],
            a(),
        )
    }

    #[test]
    fn reordered_unions_share_one_identity_and_null_does_not_count() {
        let pool = SymbolPool::default();
        let projection = GoTypeProjection::new(&pool, 2);
        let left = projection.project(&union(vec![
            Ty::String { attr: a() },
            Ty::Int { attr: a() },
            Ty::Null { attr: a() },
        ]));
        let right = projection.project(&union(vec![
            Ty::Int { attr: a() },
            Ty::Null { attr: a() },
            Ty::String { attr: a() },
        ]));
        assert_eq!(left, right);
        assert!(matches!(left, GoTy::Optional(inner) if matches!(*inner, GoTy::TypedUnion(_))));
    }

    #[test]
    fn threshold_zero_is_dynamic_and_nullable_dynamic_is_never_optional_any() {
        let pool = SymbolPool::default();
        let projection = GoTypeProjection::new(&pool, 0);
        assert!(matches!(
            projection.project(&union(vec![
                Ty::Int { attr: a() },
                Ty::String { attr: a() },
                Ty::Null { attr: a() },
            ])),
            GoTy::DynamicUnion { nullable: true, .. }
        ));
    }

    #[test]
    fn rust_type_projects_as_one_canonical_opaque_leaf_while_resource_stays_deferred() {
        let pool = SymbolPool::default();
        let projection = GoTypeProjection::new(&pool, 3);
        assert_eq!(
            projection.project(&Ty::RustType { attr: a() }),
            GoTy::RustType
        );
        assert_eq!(
            projection.project(&Ty::Resource { attr: a() }),
            GoTy::Unsupported
        );
    }

    #[test]
    fn aliases_flatten_transparently_before_thresholding() {
        let foo = alias_name("FooTools");
        let mut pool = SymbolPool::default();
        pool.insert(
            foo.clone(),
            Symbol::TypeAlias(TypeAlias {
                name: foo.clone(),
                resolves_to: union(vec![Ty::String { attr: a() }, Ty::Int { attr: a() }]),
                recursive: false,
                origin: Origin {
                    source_file_path: "types.baml".to_string(),
                    span_start: 0,
                },
            }),
        );
        let projection = GoTypeProjection::new(&pool, 3);
        let expanded = projection.project(&union(vec![
            Ty::TypeAlias(foo, a()),
            Ty::Bool { attr: a() },
        ]));
        let direct = projection.project(&union(vec![
            Ty::String { attr: a() },
            Ty::Int { attr: a() },
            Ty::Bool { attr: a() },
        ]));
        assert_eq!(expanded, direct);
    }

    #[test]
    fn only_canonical_recursive_json_alias_projects_to_json() {
        let canonical = Name::from_dotted_path(BAML_JSON_JSON);
        let lookalike = Name::new(
            BaseName::new("user"),
            vec![BaseName::new("json")],
            BaseName::new("json"),
        );
        let mut pool = SymbolPool::default();
        for name in [canonical.clone(), lookalike.clone()] {
            pool.insert(
                name.clone(),
                Symbol::TypeAlias(TypeAlias {
                    name: name.clone(),
                    resolves_to: Ty::TypeAlias(name, a()),
                    recursive: true,
                    origin: Origin {
                        source_file_path: "types.baml".to_string(),
                        span_start: 0,
                    },
                }),
            );
        }
        let projection = GoTypeProjection::new(&pool, 3);
        assert_eq!(
            projection.project(&Ty::TypeAlias(canonical, a())),
            GoTy::Json
        );
        assert_eq!(
            projection.project(&Ty::TypeAlias(lookalike, a())),
            GoTy::Unsupported
        );
    }

    #[test]
    fn json_absorbs_redundant_null_in_unions() {
        let canonical = Name::from_dotted_path(BAML_JSON_JSON);
        let mut pool = SymbolPool::default();
        pool.insert(
            canonical.clone(),
            Symbol::TypeAlias(TypeAlias {
                name: canonical.clone(),
                resolves_to: Ty::TypeAlias(canonical.clone(), a()),
                recursive: true,
                origin: Origin {
                    source_file_path: "types.baml".to_string(),
                    span_start: 0,
                },
            }),
        );
        let projection = GoTypeProjection::new(&pool, 3);
        assert_eq!(
            projection.project(&union(vec![
                Ty::TypeAlias(canonical, a()),
                Ty::Null { attr: a() },
            ])),
            GoTy::Json
        );
    }

    #[test]
    fn unions_containing_recursive_json_use_dynamic_representation() {
        let canonical = Name::from_dotted_path(BAML_JSON_JSON);
        let mut pool = SymbolPool::default();
        pool.insert(
            canonical.clone(),
            Symbol::TypeAlias(TypeAlias {
                name: canonical.clone(),
                resolves_to: Ty::TypeAlias(canonical.clone(), a()),
                recursive: true,
                origin: Origin {
                    source_file_path: "types.baml".to_string(),
                    span_start: 0,
                },
            }),
        );
        let projection = GoTypeProjection::new(&pool, 3);
        let projected = projection.project(&union(vec![
            Ty::TypeAlias(canonical, a()),
            Ty::String { attr: a() },
        ]));
        assert!(matches!(projected, GoTy::DynamicUnion { .. }));
    }

    #[test]
    fn user_alias_to_recursive_json_is_omitted_inside_a_union() {
        let canonical = Name::from_dotted_path(BAML_JSON_JSON);
        let alias = alias_name("JsonAlias");
        let mut pool = SymbolPool::default();
        pool.insert(
            canonical.clone(),
            Symbol::TypeAlias(TypeAlias {
                name: canonical.clone(),
                resolves_to: Ty::TypeAlias(canonical.clone(), a()),
                recursive: true,
                origin: Origin {
                    source_file_path: "types.baml".to_string(),
                    span_start: 0,
                },
            }),
        );
        pool.insert(
            alias.clone(),
            Symbol::TypeAlias(TypeAlias {
                name: alias.clone(),
                resolves_to: Ty::TypeAlias(canonical, a()),
                recursive: false,
                origin: Origin {
                    source_file_path: "types.baml".to_string(),
                    span_start: 0,
                },
            }),
        );
        let projection = GoTypeProjection::new(&pool, 3);
        assert_eq!(
            projection.project(&Ty::TypeAlias(alias.clone(), a())),
            GoTy::Json
        );
        assert_eq!(
            projection.project(&union(vec![
                Ty::TypeAlias(alias, a()),
                Ty::String { attr: a() },
            ])),
            GoTy::Unsupported
        );
    }

    #[test]
    fn literals_remain_semantic_until_surface_rendering() {
        let pool = SymbolPool::default();
        let projection = GoTypeProjection::new(&pool, 3);
        let projected = projection.project(&union(vec![
            Ty::Literal(Literal::String("draft".into()), Freshness::Regular, a()),
            Ty::String { attr: a() },
        ]));
        let GoTy::TypedUnion(key) = projected else {
            panic!("two literal-distinct members should remain a typed union")
        };
        assert!(key.members().contains(&GoTy::String));
        assert!(
            key.members()
                .contains(&GoTy::Literal(GoLiteral::String("draft".into())))
        );
    }

    #[test]
    fn concrete_media_kinds_project_canonically_and_generic_media_is_deferred() {
        let pool = SymbolPool::default();
        let projection = GoTypeProjection::new(&pool, 3);
        for kind in [
            MediaKind::Image,
            MediaKind::Audio,
            MediaKind::Video,
            MediaKind::Pdf,
        ] {
            assert_eq!(projection.project(&Ty::Media(kind, a())), GoTy::Media(kind));
        }
        assert_eq!(
            projection.project(&Ty::Media(MediaKind::Generic, a())),
            GoTy::Unsupported
        );
    }

    #[test]
    fn required_only_callables_project_structurally_with_declared_throws() {
        let pool = SymbolPool::default();
        let projection = GoTypeProjection::new(&pool, 3);
        let projected = projection.project(&callable(
            vec![
                callable_param(CodegenFunctionParamMode::Required, Ty::Int { attr: a() }),
                callable_param(
                    CodegenFunctionParamMode::Required,
                    Ty::List(Box::new(Ty::String { attr: a() }), a()),
                ),
            ],
            Ty::Bool { attr: a() },
            host_callable_error(),
        ));
        let GoTy::Function(key) = projected else {
            panic!("supported callback should project to a Go function")
        };
        assert_eq!(key.params()[0].ty(), &GoTy::Int);
        assert_eq!(key.params()[1].ty(), &GoTy::List(Box::new(GoTy::String)));
        assert_eq!(key.ret(), Some(&GoTy::Bool));
        assert!(key.throws());
    }

    #[test]
    fn callback_projection_preserves_optional_mode_and_wire_name() {
        let pool = SymbolPool::default();
        let projection = GoTypeProjection::new(&pool, 3);
        let projected = projection.project(&callable(
            vec![
                named_callable_param(
                    "ignored_required_name",
                    CodegenFunctionParamMode::Required,
                    Ty::Int { attr: a() },
                ),
                named_callable_param(
                    "wire_name",
                    CodegenFunctionParamMode::Optional,
                    union(vec![Ty::String { attr: a() }, Ty::Null { attr: a() }]),
                ),
            ],
            Ty::Bool { attr: a() },
            Ty::Never { attr: a() },
        ));
        let GoTy::Function(key) = projected else {
            panic!("optional callback should project to a Go function")
        };
        assert_eq!(key.params()[0].mode(), GoFunctionParamMode::Required);
        assert_eq!(key.params()[0].name(), None);
        assert_eq!(key.params()[1].mode(), GoFunctionParamMode::Optional);
        assert_eq!(key.params()[1].name(), Some(&BaseName::new("wire_name")));
        assert_eq!(
            key.params()[1].ty(),
            &GoTy::Optional(Box::new(GoTy::String))
        );
    }

    #[test]
    fn callback_identity_uses_optional_names_but_not_required_names() {
        let pool = SymbolPool::default();
        let projection = GoTypeProjection::new(&pool, 3);
        let project = |required: &str, optional: &str| {
            projection.project(&callable(
                vec![
                    named_callable_param(
                        required,
                        CodegenFunctionParamMode::Required,
                        Ty::Int { attr: a() },
                    ),
                    named_callable_param(
                        optional,
                        CodegenFunctionParamMode::Optional,
                        Ty::String { attr: a() },
                    ),
                ],
                Ty::Bool { attr: a() },
                Ty::Never { attr: a() },
            ))
        };
        assert_eq!(project("left", "value"), project("right", "value"));
        assert_ne!(project("left", "first"), project("left", "second"));
    }

    #[test]
    fn callback_projection_supports_closed_unions_and_omits_unsound_shapes() {
        let pool = SymbolPool::default();
        let projection = GoTypeProjection::new(&pool, 3);
        let never = || Ty::Never { attr: a() };
        let closed_union = callable(
            vec![callable_param(
                CodegenFunctionParamMode::Required,
                union(vec![Ty::Int { attr: a() }, Ty::String { attr: a() }]),
            )],
            union(vec![Ty::Bool { attr: a() }, Ty::String { attr: a() }]),
            never(),
        );
        let GoTy::Function(closed_union) = projection.project(&closed_union) else {
            panic!("concrete closed-union callback should project to a Go function")
        };
        assert!(matches!(closed_union.params()[0].ty(), GoTy::TypedUnion(_)));
        assert!(matches!(closed_union.ret(), Some(GoTy::TypedUnion(_))));

        let unsupported = [
            callable(
                vec![callable_param(
                    CodegenFunctionParamMode::Required,
                    Ty::TypeVar(baml_codegen_types::ParamTy::new(0, BaseName::new("T")), a()),
                )],
                Ty::String { attr: a() },
                never(),
            ),
            callable(
                vec![callable_param(
                    CodegenFunctionParamMode::Required,
                    callable(vec![], Ty::String { attr: a() }, never()),
                )],
                Ty::String { attr: a() },
                never(),
            ),
            // Arity above the configured threshold projects dynamically as
            // `any`, which cannot preserve selected-arm identity in a host
            // callback adapter.
            callable(
                vec![callable_param(
                    CodegenFunctionParamMode::Required,
                    union(vec![
                        Ty::Int { attr: a() },
                        Ty::String { attr: a() },
                        Ty::Bool { attr: a() },
                        Ty::Float { attr: a() },
                    ]),
                )],
                Ty::String { attr: a() },
                never(),
            ),
        ];
        for ty in unsupported {
            assert_eq!(projection.project(&ty), GoTy::Unsupported);
        }
    }

    #[test]
    fn callback_projection_rejects_incompatible_declared_throws() {
        let validation_error = Ty::Class(alias_name("ValidationError"), vec![], a());
        let pool = SymbolPool::default();
        let projection = GoTypeProjection::new(&pool, 3);
        assert_eq!(
            projection.project(&callable(
                vec![callable_param(
                    CodegenFunctionParamMode::Required,
                    Ty::Int { attr: a() },
                )],
                Ty::String { attr: a() },
                validation_error.clone(),
            )),
            GoTy::Unsupported
        );
        assert!(matches!(
            projection.project(&callable(
                vec![],
                Ty::String { attr: a() },
                union(vec![validation_error, host_callable_error()]),
            )),
            GoTy::Function(key) if key.throws()
        ));

        assert!(matches!(
            projection.project(&callable(
                vec![],
                Ty::String { attr: a() },
                Ty::TypeVar(
                    baml_codegen_types::ParamTy::new(0, BaseName::new("__effect_param_0")),
                    a(),
                ),
            )),
            GoTy::Function(key) if !key.throws()
        ));
    }
}

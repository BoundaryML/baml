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

use baml_base::{Literal, MediaKind, Name as BaseName, qualified_name::BAML_JSON_JSON};
use baml_sdkgen_types::{
    ClassCrossing, CodegenFunctionParamMode, Name, Symbol, SymbolPool, Ty,
    is_synthetic_effect_param,
};

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
    /// `ai.stream.Stream<T>`: partials and the settled value share `value`.
    Stream {
        value: Box<Self>,
    },
    /// The class whose crossing is `Prompt` (`ai.Prompt`): a rendered prompt,
    /// carried by its own wire arm. Its Go surface is `baml_go.Prompt`.
    Prompt(Name),
    /// A class whose crossing is `Opaque` (`reflect.Session`, ...): an
    /// instance only the engine builds, which Go holds as the reference it
    /// crossed as, a `baml_go.OpaqueHandle`, and passes back.
    Opaque(Name),
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
    returns: GoCallbackReturn,
    throws: GoCallbackThrows,
}

/// What a callback's declared return type becomes on the Go side.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) enum GoCallbackReturn {
    /// The Go surface returns a value of this type.
    Value(Box<GoTy>),
    /// Declared `null` or `void`: the Go surface returns nothing and the
    /// adapter returns `null`.
    Null,
    /// Declared `never`: the Go surface returns nothing, and a callback that
    /// returns anyway broke its contract.
    Never,
}

/// What a callback's declared `throws` becomes on the Go side: the throws type
/// its binding states and whether its Go surface returns an `error`.
///
/// Whatever the surface, the outcome of a failure is decided by what fails: a
/// Go error or panic of the host's own is `raised` (the engine builds the
/// `baml.errors.HostCallable` around it), a BAML error or panic the callback
/// received and returns (or, without an `error` result, panics with) is
/// `thrown` on, and the engine holds either to the stated throws type.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) enum GoCallbackThrows {
    /// Declared `throws never`. The callback must not fail, so any failure is a
    /// contract violation, exactly as the declaration says.
    Never,
    /// The slot's throws is its open effect parameter, stated `unknown`. The
    /// Go surface returns no `error`; a failure is an ordinary catchable BAML
    /// error rather than a contract violation.
    Open,
    /// The declared throws is `baml.errors.HostCallable`: a returned Go
    /// `error` is raised, so the Go surface returns an `error`.
    HostCallable,
    /// The declared throws names BAML values. The Go surface returns an
    /// `error`; a callback throws a value of `thrown`, the declared throws
    /// type less `baml.errors.HostCallable`, as `baml_go.Throw(value)`.
    /// `host_callable` is whether the declaration also admits
    /// `baml.errors.HostCallable`, so a raised Go error fits it.
    Values {
        thrown: Box<GoTy>,
        host_callable: bool,
    },
}

impl GoCallbackThrows {
    /// Whether the Go surface of the callback returns an `error`.
    pub(crate) fn returns_error(&self) -> bool {
        match self {
            Self::HostCallable | Self::Values { .. } => true,
            Self::Never | Self::Open => false,
        }
    }

    /// The Go type a callback throws with `baml_go.Throw`, if its declaration
    /// names BAML values.
    pub(crate) fn thrown(&self) -> Option<&GoTy> {
        match self {
            Self::Values { thrown, .. } => Some(thrown),
            Self::Never | Self::Open | Self::HostCallable => None,
        }
    }
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

    /// The Go type the callback's surface returns, if it returns a value.
    pub(crate) fn ret(&self) -> Option<&GoTy> {
        match &self.returns {
            GoCallbackReturn::Value(ret) => Some(ret),
            GoCallbackReturn::Null | GoCallbackReturn::Never => None,
        }
    }

    pub(crate) fn returns(&self) -> &GoCallbackReturn {
        &self.returns
    }

    pub(crate) fn throws(&self) -> &GoCallbackThrows {
        &self.throws
    }

    /// Every Go type the callback's surface spells: its parameters', its
    /// result's and the type it throws with `baml_go.Throw`. Walks over a
    /// callback visit these and nothing else.
    pub(crate) fn component_tys(&self) -> impl Iterator<Item = &GoTy> {
        self.params
            .iter()
            .map(GoFunctionParamKey::ty)
            .chain(self.ret())
            .chain(self.throws.thrown())
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
        fn function_tys(function: &baml_sdkgen_types::Function) -> Vec<&Ty> {
            function
                .arguments
                .iter()
                .map(|argument| &argument.ty)
                .chain(std::iter::once(&function.return_type))
                .collect()
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

    /// How an instance of the class `name` crosses. A class the pool does not
    /// declare has no crossing of its own, so it crosses by its fields (and
    /// the support filter omits whatever names it).
    fn crossing(&self, name: &Name) -> ClassCrossing {
        match self.pool.get(name) {
            Some(Symbol::Class(class)) => class.crossing,
            Some(Symbol::Enum(_) | Symbol::Function(_) | Symbol::TypeAlias(_)) | None => {
                ClassCrossing::Fields
            }
        }
    }

    fn project_inner(&self, ty: &Ty, aliases: &mut HashSet<Name>) -> GoTy {
        match ty {
            Ty::String => GoTy::String,
            Ty::Int => GoTy::Int,
            Ty::Bigint => GoTy::Bigint,
            Ty::Float => GoTy::Float,
            Ty::Bool => GoTy::Bool,
            Ty::Null => GoTy::Null,
            Ty::Uint8Array => GoTy::Uint8Array,
            Ty::Media(MediaKind::Generic) => GoTy::Unsupported,
            Ty::Media(kind) => GoTy::Media(*kind),
            Ty::Type => GoTy::ReflectedType,
            Ty::RustType => GoTy::RustType,
            Ty::Literal(literal, ..) => GoTy::Literal(match literal {
                Literal::String(value) => GoLiteral::String(value.clone()),
                Literal::Int(value) => GoLiteral::Int(*value),
                Literal::Bigint(value) => GoLiteral::Bigint(value.to_string()),
                Literal::Float(value) => GoLiteral::Float(value.clone()),
                Literal::Bool(value) => GoLiteral::Bool(*value),
            }),
            Ty::TypeVar(param) => GoTy::TypeVar(param.name().clone()),
            Ty::Class(name, arguments) if is_reflect_kind_type(name) => match arguments.as_ref() {
                [] => GoTy::ReflectedType,
                [_, ..] => GoTy::Unsupported,
            },
            // How an instance crosses is the compiler's decision, made once per
            // declaration; the Go surface follows it.
            Ty::Class(name, arguments) => match (self.crossing(name), arguments.as_ref()) {
                (ClassCrossing::FunctionSpec, [output]) => GoTy::FunctionSpec {
                    output: Box::new(self.project_inner(output, aliases)),
                },
                (ClassCrossing::Stream, [value]) => GoTy::Stream {
                    value: Box::new(self.project_inner(value, aliases)),
                },
                (ClassCrossing::Prompt, []) => GoTy::Prompt(name.clone()),
                (ClassCrossing::Opaque, []) => GoTy::Opaque(name.clone()),
                (ClassCrossing::Media(MediaKind::Generic), []) => GoTy::Unsupported,
                (ClassCrossing::Media(kind), []) => GoTy::Media(kind),
                (ClassCrossing::Fields, arguments) => GoTy::Class(
                    name.clone(),
                    arguments
                        .iter()
                        .map(|argument| self.project_inner(argument, aliases))
                        .collect::<Vec<_>>()
                        .into_boxed_slice(),
                ),
                (
                    ClassCrossing::FunctionSpec
                    | ClassCrossing::Stream
                    | ClassCrossing::Prompt
                    | ClassCrossing::Opaque
                    | ClassCrossing::Media(_),
                    _,
                ) => GoTy::Unsupported,
            },
            Ty::Enum(name) => GoTy::Enum(name.clone()),
            Ty::EnumVariant(name, variant) => GoTy::EnumVariant(name.clone(), variant.clone()),
            Ty::List(inner) => GoTy::List(Box::new(self.project_inner(inner, aliases))),
            Ty::Map { key, value, .. } => GoTy::Map {
                key: Box::new(self.project_inner(key, aliases)),
                value: Box::new(self.project_inner(value, aliases)),
            },
            Ty::TypeAlias(name) => {
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
            Ty::Union(members) => self.project_union(members, aliases),
            Ty::Function {
                params,
                ret,
                throws,
                ..
            } => {
                let throws = if matches!(throws.as_ref(), Ty::Never) {
                    GoCallbackThrows::Never
                } else if matches!(
                    throws.as_ref(),
                    Ty::TypeVar(param) if is_synthetic_effect_param(param.name())
                ) {
                    GoCallbackThrows::Open
                } else {
                    let mut thrown = Vec::new();
                    let mut host_callable = false;
                    if !self.split_host_callable(throws, aliases, &mut thrown, &mut host_callable) {
                        return GoTy::Unsupported;
                    }
                    let thrown = match thrown.len() {
                        0 => None,
                        1 => thrown.pop(),
                        _ => Some(Ty::Union(thrown.into())),
                    };
                    match thrown {
                        None => GoCallbackThrows::HostCallable,
                        Some(thrown) => GoCallbackThrows::Values {
                            thrown: Box::new(self.project_inner(&thrown, aliases)),
                            host_callable,
                        },
                    }
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
                let returns = match ret.as_ref() {
                    Ty::Never => GoCallbackReturn::Never,
                    ret if ret.is_unit() => GoCallbackReturn::Null,
                    ret => GoCallbackReturn::Value(Box::new(self.project_inner(ret, aliases))),
                };
                let key = GoFunctionKey {
                    params: params.into_boxed_slice(),
                    returns,
                    throws,
                };
                if key.component_tys().any(callback_component_is_unsupported) {
                    GoTy::Unsupported
                } else {
                    GoTy::Function(key)
                }
            }
            _ => GoTy::Unsupported,
        }
    }

    /// Splits a declared throws type into the members a Go callback throws
    /// with `baml_go.Throw` (`thrown`) and whether it admits
    /// `baml.errors.HostCallable`, looking through unions and non-recursive
    /// aliases of unions. Reports false for a declaration Go cannot split (a
    /// recursive alias).
    fn split_host_callable(
        &self,
        ty: &Ty,
        aliases: &mut HashSet<Name>,
        thrown: &mut Vec<Ty>,
        host_callable: &mut bool,
    ) -> bool {
        match ty {
            Ty::Class(name, arguments) if arguments.is_empty() && is_host_callable_error(name) => {
                *host_callable = true;
                true
            }
            Ty::Union(members) => members
                .iter()
                .all(|member| self.split_host_callable(member, aliases, thrown, host_callable)),
            Ty::TypeAlias(name) if !is_baml_json(name) => {
                if !aliases.insert(name.clone()) {
                    return false;
                }
                let split = match &self.pool[name] {
                    Symbol::TypeAlias(alias) if !alias.recursive => {
                        self.split_host_callable(&alias.resolves_to, aliases, thrown, host_callable)
                    }
                    _ => false,
                };
                aliases.remove(name);
                split
            }
            other => {
                thrown.push(other.clone());
                true
            }
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
            Ty::Union(nested) => {
                for member in nested {
                    self.project_union_member(member, aliases, projected);
                }
            }
            Ty::TypeAlias(name) => {
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

fn is_host_callable_error(name: &Name) -> bool {
    name == &Name::new(
        BaseName::new("baml"),
        vec![BaseName::new("errors")],
        BaseName::new("HostCallable"),
    )
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
        GoTy::Stream { value } => collect_typed_unions(value, found),
        GoTy::List(inner) | GoTy::Optional(inner) => collect_typed_unions(inner, found),
        GoTy::Function(key) => {
            for component in key.component_tys() {
                collect_typed_unions(component, found);
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
                for component in key.component_tys() {
                    collect_callback_options(component, found);
                }
            }
        }
        GoTy::Class(_, arguments) => {
            for argument in arguments {
                collect_callback_options(argument, found);
            }
        }
        GoTy::FunctionSpec { output } => collect_callback_options(output, found),
        GoTy::Stream { value } => collect_callback_options(value, found),
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
        GoTy::Stream { value } => contains_unsupported(value),
        GoTy::List(inner) | GoTy::Optional(inner) => contains_unsupported(inner),
        GoTy::Map { key, value } => contains_unsupported(key) || contains_unsupported(value),
        GoTy::TypedUnion(key) | GoTy::DynamicUnion { key, .. } => {
            key.members().iter().any(contains_unsupported)
        }
        GoTy::Function(key) => key.component_tys().any(contains_unsupported),
        _ => false,
    }
}

fn contains_type_var(ty: &GoTy) -> bool {
    match ty {
        GoTy::TypeVar(_) => true,
        GoTy::Class(_, arguments) => arguments.iter().any(contains_type_var),
        GoTy::FunctionSpec { output } => contains_type_var(output),
        GoTy::Stream { value } => contains_type_var(value),
        GoTy::List(inner) | GoTy::Optional(inner) => contains_type_var(inner),
        GoTy::Map { key, value } => contains_type_var(key) || contains_type_var(value),
        GoTy::TypedUnion(key) | GoTy::DynamicUnion { key, .. } => {
            key.members().iter().any(contains_type_var)
        }
        GoTy::Function(key) => key.component_tys().any(contains_type_var),
        _ => false,
    }
}

// BUG: a callback cannot receive a BAML closure. Any function type inside a
// callback's parameters or result makes the whole callback `Unsupported`, so a
// BAML function such as `hof(cb: (f: (x: int) -> int) -> int)` is omitted from
// the Go SDK entirely. Rendering is direction-blind: a function value BAML
// hands to the host must surface as `ReturnedFunction[...]` (the decoder the
// callback adapter would already call), while one the host hands to BAML is a
// plain Go `func`, and every nested function flips that direction. Lifting this
// needs that direction threaded through `render_projected_go_type`, the
// callback options structs, and `supported_go_type`.
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
        GoTy::Stream { value } => contains_function(value),
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
        GoTy::Stream { value } => contains_dynamic_union(value),
        GoTy::List(inner) | GoTy::Optional(inner) => contains_dynamic_union(inner),
        GoTy::Map { key, value } => contains_dynamic_union(key) || contains_dynamic_union(value),
        GoTy::Function(key) => key.component_tys().any(contains_dynamic_union),
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
    use baml_sdkgen_types::{CallableParam, Origin, TypeAlias};
    use baml_type::Freshness;

    use super::*;

    fn union(members: Vec<Ty>) -> Ty {
        Ty::Union(members.into()).canonicalize()
    }

    fn alias_name(value: &str) -> Name {
        Name::new(BaseName::new("user"), vec![], BaseName::new(value))
    }

    fn callable(params: Vec<CallableParam>, ret: Ty, throws: Ty) -> Ty {
        Ty::Function {
            params: params.into(),
            ret: Box::new(ret),
            throws: Box::new(throws),
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
            Box::new([]),
        )
    }

    #[test]
    fn reordered_unions_share_one_identity_and_null_does_not_count() {
        let pool = SymbolPool::default();
        let projection = GoTypeProjection::new(&pool, 2);
        let left = projection.project(&union(vec![Ty::String, Ty::Int, Ty::Null]));
        let right = projection.project(&union(vec![Ty::Int, Ty::Null, Ty::String]));
        assert_eq!(left, right);
        assert!(matches!(left, GoTy::Optional(inner) if matches!(*inner, GoTy::TypedUnion(_))));
    }

    #[test]
    fn threshold_zero_is_dynamic_and_nullable_dynamic_is_never_optional_any() {
        let pool = SymbolPool::default();
        let projection = GoTypeProjection::new(&pool, 0);
        assert!(matches!(
            projection.project(&union(vec![Ty::Int, Ty::String, Ty::Null,])),
            GoTy::DynamicUnion { nullable: true, .. }
        ));
    }

    #[test]
    fn rust_type_projects_as_one_canonical_opaque_leaf_while_resource_stays_deferred() {
        let pool = SymbolPool::default();
        let projection = GoTypeProjection::new(&pool, 3);
        assert_eq!(projection.project(&Ty::RustType), GoTy::RustType);
        assert_eq!(projection.project(&Ty::Resource), GoTy::Unsupported);
    }

    #[test]
    fn aliases_flatten_transparently_before_thresholding() {
        let foo = alias_name("FooTools");
        let mut pool = SymbolPool::default();
        pool.insert(
            foo.clone(),
            Symbol::TypeAlias(TypeAlias {
                name: foo.clone(),
                resolves_to: union(vec![Ty::String, Ty::Int]),
                recursive: false,
                origin: Origin {
                    source_file_path: "types.baml".to_string(),
                    span_start: 0,
                },
            }),
        );
        let projection = GoTypeProjection::new(&pool, 3);
        let expanded = projection.project(&union(vec![Ty::TypeAlias(foo), Ty::Bool]));
        let direct = projection.project(&union(vec![Ty::String, Ty::Int, Ty::Bool]));
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
                    resolves_to: Ty::TypeAlias(name),
                    recursive: true,
                    origin: Origin {
                        source_file_path: "types.baml".to_string(),
                        span_start: 0,
                    },
                }),
            );
        }
        let projection = GoTypeProjection::new(&pool, 3);
        assert_eq!(projection.project(&Ty::TypeAlias(canonical)), GoTy::Json);
        assert_eq!(
            projection.project(&Ty::TypeAlias(lookalike)),
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
                resolves_to: Ty::TypeAlias(canonical.clone()),
                recursive: true,
                origin: Origin {
                    source_file_path: "types.baml".to_string(),
                    span_start: 0,
                },
            }),
        );
        let projection = GoTypeProjection::new(&pool, 3);
        assert_eq!(
            projection.project(&union(vec![Ty::TypeAlias(canonical), Ty::Null,])),
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
                resolves_to: Ty::TypeAlias(canonical.clone()),
                recursive: true,
                origin: Origin {
                    source_file_path: "types.baml".to_string(),
                    span_start: 0,
                },
            }),
        );
        let projection = GoTypeProjection::new(&pool, 3);
        let projected = projection.project(&union(vec![Ty::TypeAlias(canonical), Ty::String]));
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
                resolves_to: Ty::TypeAlias(canonical.clone()),
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
                resolves_to: Ty::TypeAlias(canonical),
                recursive: false,
                origin: Origin {
                    source_file_path: "types.baml".to_string(),
                    span_start: 0,
                },
            }),
        );
        let projection = GoTypeProjection::new(&pool, 3);
        assert_eq!(
            projection.project(&Ty::TypeAlias(alias.clone())),
            GoTy::Json
        );
        assert_eq!(
            projection.project(&union(vec![Ty::TypeAlias(alias), Ty::String,])),
            GoTy::Unsupported
        );
    }

    #[test]
    fn literals_remain_semantic_until_surface_rendering() {
        let pool = SymbolPool::default();
        let projection = GoTypeProjection::new(&pool, 3);
        let projected = projection.project(&union(vec![
            Ty::Literal(Literal::String("draft".into()), Freshness::Regular),
            Ty::String,
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
            assert_eq!(projection.project(&Ty::Media(kind)), GoTy::Media(kind));
        }
        assert_eq!(
            projection.project(&Ty::Media(MediaKind::Generic)),
            GoTy::Unsupported
        );
    }

    #[test]
    fn required_only_callables_project_structurally_with_declared_throws() {
        let pool = SymbolPool::default();
        let projection = GoTypeProjection::new(&pool, 3);
        let projected = projection.project(&callable(
            vec![
                callable_param(CodegenFunctionParamMode::Required, Ty::Int),
                callable_param(
                    CodegenFunctionParamMode::Required,
                    Ty::List(Box::new(Ty::String)),
                ),
            ],
            Ty::Bool,
            host_callable_error(),
        ));
        let GoTy::Function(key) = projected else {
            panic!("supported callback should project to a Go function")
        };
        assert_eq!(key.params()[0].ty(), &GoTy::Int);
        assert_eq!(key.params()[1].ty(), &GoTy::List(Box::new(GoTy::String)));
        assert_eq!(key.ret(), Some(&GoTy::Bool));
        assert_eq!(key.throws(), &GoCallbackThrows::HostCallable);
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
                    Ty::Int,
                ),
                named_callable_param(
                    "wire_name",
                    CodegenFunctionParamMode::Optional,
                    union(vec![Ty::String, Ty::Null]),
                ),
            ],
            Ty::Bool,
            Ty::Never,
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
                    named_callable_param(required, CodegenFunctionParamMode::Required, Ty::Int),
                    named_callable_param(optional, CodegenFunctionParamMode::Optional, Ty::String),
                ],
                Ty::Bool,
                Ty::Never,
            ))
        };
        assert_eq!(project("left", "value"), project("right", "value"));
        assert_ne!(project("left", "first"), project("left", "second"));
    }

    #[test]
    fn callback_projection_supports_closed_unions_and_omits_unsound_shapes() {
        let pool = SymbolPool::default();
        let projection = GoTypeProjection::new(&pool, 3);
        let never = || Ty::Never;
        let closed_union = callable(
            vec![callable_param(
                CodegenFunctionParamMode::Required,
                union(vec![Ty::Int, Ty::String]),
            )],
            union(vec![Ty::Bool, Ty::String]),
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
                    Ty::TypeVar(baml_sdkgen_types::ParamTy::new(0, BaseName::new("T"))),
                )],
                Ty::String,
                never(),
            ),
            callable(
                vec![callable_param(
                    CodegenFunctionParamMode::Required,
                    callable(vec![], Ty::String, never()),
                )],
                Ty::String,
                never(),
            ),
            // Arity above the configured threshold projects dynamically as
            // `any`, which cannot preserve selected-arm identity in a host
            // callback adapter.
            callable(
                vec![callable_param(
                    CodegenFunctionParamMode::Required,
                    union(vec![Ty::Int, Ty::String, Ty::Bool, Ty::Float]),
                )],
                Ty::String,
                never(),
            ),
        ];
        for ty in unsupported {
            assert_eq!(projection.project(&ty), GoTy::Unsupported);
        }
    }

    #[test]
    fn callback_projection_states_declared_throws_and_what_go_throws() {
        let validation_error = Ty::Class(alias_name("ValidationError"), Box::new([]));
        let projected_error = GoTy::Class(alias_name("ValidationError"), Box::new([]));
        let pool = SymbolPool::default();
        let projection = GoTypeProjection::new(&pool, 3);
        let throws_of = |throws: Ty| match projection.project(&callable(
            vec![callable_param(CodegenFunctionParamMode::Required, Ty::Int)],
            Ty::String,
            throws,
        )) {
            GoTy::Function(key) => key.throws().clone(),
            other => panic!("callback should project to a Go function, got {other:?}"),
        };
        // A declared BAML error is a value the callback throws with
        // `baml_go.Throw`; a raised Go error does not fit it.
        assert_eq!(
            throws_of(validation_error.clone()),
            GoCallbackThrows::Values {
                thrown: Box::new(projected_error.clone()),
                host_callable: false,
            }
        );
        // Beside `baml.errors.HostCallable`, both fit: the declaration is
        // stated whole, and Go throws only the BAML members.
        assert_eq!(
            throws_of(union(vec![validation_error, host_callable_error()])),
            GoCallbackThrows::Values {
                thrown: Box::new(projected_error),
                host_callable: true,
            }
        );
        assert_eq!(
            throws_of(host_callable_error()),
            GoCallbackThrows::HostCallable
        );
    }

    #[test]
    fn classes_project_by_their_declared_crossing() {
        let declared = |name: &str, crossing: ClassCrossing| {
            let name = Name::from_dotted_path(name);
            (
                name.clone(),
                Symbol::Class(baml_sdkgen_types::Class {
                    crossing,
                    name,
                    generic_params: vec![],
                    docstring: None,
                    properties: vec![],
                    static_methods: vec![],
                    instance_methods: vec![],
                    origin: Origin {
                        source_file_path: "test.baml".to_string(),
                        span_start: 0,
                    },
                }),
            )
        };
        let pool = SymbolPool::from([
            declared("reflect.Session", ClassCrossing::Opaque),
            declared("ai.Prompt", ClassCrossing::Prompt),
            declared("ai.FunctionSpec", ClassCrossing::FunctionSpec),
            declared("user.Plain", ClassCrossing::Fields),
        ]);
        let projection = GoTypeProjection::new(&pool, 3);
        let class = |name: &str, arguments: Vec<Ty>| {
            projection.project(&Ty::Class(
                Name::from_dotted_path(name),
                arguments.into_boxed_slice(),
            ))
        };
        assert_eq!(
            class("reflect.Session", vec![]),
            GoTy::Opaque(Name::from_dotted_path("reflect.Session"))
        );
        assert_eq!(
            class("ai.Prompt", vec![]),
            GoTy::Prompt(Name::from_dotted_path("ai.Prompt"))
        );
        assert_eq!(
            class("ai.FunctionSpec", vec![Ty::String]),
            GoTy::FunctionSpec {
                output: Box::new(GoTy::String)
            }
        );
        assert_eq!(
            class("user.Plain", vec![]),
            GoTy::Class(Name::from_dotted_path("user.Plain"), Box::new([]))
        );
    }

    #[test]
    fn callback_projection_distinguishes_never_from_null_returns() {
        let pool = SymbolPool::default();
        let projection = GoTypeProjection::new(&pool, 3);
        let returns_of = |ret: Ty| match projection.project(&callable(vec![], ret, Ty::Never)) {
            GoTy::Function(key) => key.returns().clone(),
            other => panic!("callback should project to a Go function, got {other:?}"),
        };
        assert_eq!(returns_of(Ty::Never), GoCallbackReturn::Never);
        assert_eq!(returns_of(Ty::Null), GoCallbackReturn::Null);
        assert_eq!(
            returns_of(Ty::Int),
            GoCallbackReturn::Value(Box::new(GoTy::Int))
        );
    }

    #[test]
    fn callback_projection_keeps_open_effects_open_and_never_closed() {
        let pool = SymbolPool::default();
        let projection = GoTypeProjection::new(&pool, 3);
        let throws_of = |throws: Ty| match projection.project(&callable(vec![], Ty::String, throws))
        {
            GoTy::Function(key) => key.throws().clone(),
            other => panic!("callback should project to a Go function, got {other:?}"),
        };

        // An omitted `throws` is an open effect parameter: a Go failure must stay
        // catchable, so the binding states `unknown`, not `never`.
        assert_eq!(
            throws_of(Ty::TypeVar(baml_sdkgen_types::ParamTy::new(
                0,
                BaseName::new("__effect_param_0")
            ))),
            GoCallbackThrows::Open
        );
        assert_eq!(throws_of(Ty::Never), GoCallbackThrows::Never);
        assert!(!GoCallbackThrows::Open.returns_error());
        assert!(!GoCallbackThrows::Never.returns_error());
        assert!(GoCallbackThrows::HostCallable.returns_error());
    }
}

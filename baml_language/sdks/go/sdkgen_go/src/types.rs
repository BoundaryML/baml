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

use baml_base::{Literal, MediaKind, Name as BaseName};
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
    Literal(GoLiteral),
    Class(Name),
    Enum(Name),
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

/// The structural identity of the required-only callback subset exposed to Go.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct GoFunctionKey {
    params: Box<[GoTy]>,
    ret: Option<Box<GoTy>>,
    throws: bool,
}

impl GoFunctionKey {
    pub(crate) fn params(&self) -> &[GoTy] {
        &self.params
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
}

impl<'a> GoTypeProjection<'a> {
    pub(crate) fn new(pool: &'a SymbolPool, max_typed_union_arity: usize) -> Self {
        let mut projection = Self {
            pool,
            max_typed_union_arity,
            typed_unions: BTreeMap::new(),
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

    pub(crate) fn max_typed_union_arity(&self) -> usize {
        self.max_typed_union_arity
    }

    fn collect(&mut self) {
        let symbols = self
            .pool
            .iter()
            .map(|(name, symbol)| (name.clone(), symbol))
            .collect::<Vec<_>>();
        for (name, symbol) in symbols {
            let tys: Vec<&Ty> = match &symbol {
                Symbol::Function(function) => function
                    .arguments
                    .iter()
                    .map(|argument| &argument.ty)
                    .chain(std::iter::once(&function.return_type))
                    .collect(),
                Symbol::Class(class) => class
                    .properties
                    .iter()
                    .map(|field| &field.ty)
                    .chain(
                        class
                            .static_methods
                            .iter()
                            .chain(&class.instance_methods)
                            .flat_map(|method| {
                                method
                                    .arguments
                                    .iter()
                                    .map(|argument| &argument.ty)
                                    .chain(std::iter::once(&method.return_type))
                            }),
                    )
                    .collect(),
                Symbol::TypeAlias(alias) => vec![&alias.resolves_to],
                Symbol::Enum(_) => Vec::new(),
            };
            let mut found = BTreeSet::new();
            for ty in tys {
                collect_typed_unions(&self.project(ty), &mut found);
            }
            self.typed_unions
                .entry(name.package().clone())
                .or_default()
                .extend(found);
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
            Ty::Literal(literal, ..) => GoTy::Literal(match literal {
                Literal::String(value) => GoLiteral::String(value.clone()),
                Literal::Int(value) => GoLiteral::Int(*value),
                Literal::Bigint(value) => GoLiteral::Bigint(value.to_string()),
                Literal::Float(value) => GoLiteral::Float(value.clone()),
                Literal::Bool(value) => GoLiteral::Bool(*value),
            }),
            Ty::Class(name, arguments, _) if arguments.is_empty() => GoTy::Class(name.clone()),
            Ty::Enum(name, _) | Ty::EnumVariant(name, _, _) => GoTy::Enum(name.clone()),
            Ty::List(inner, _) => GoTy::List(Box::new(self.project_inner(inner, aliases))),
            Ty::Map { key, value, .. } => GoTy::Map {
                key: Box::new(self.project_inner(key, aliases)),
                value: Box::new(self.project_inner(value, aliases)),
            },
            Ty::TypeAlias(name, _) => {
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
                if params
                    .iter()
                    .any(|param| param.mode != CodegenFunctionParamMode::Required)
                {
                    return GoTy::Unsupported;
                }
                let throws = if matches!(throws.as_ref(), Ty::Never { .. }) {
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
                    .map(|param| self.project_inner(&param.ty, aliases))
                    .collect::<Vec<_>>();
                let ret = match ret.as_ref() {
                    Ty::Void { .. } | Ty::Never { .. } => None,
                    ret => Some(Box::new(self.project_inner(ret, aliases))),
                };
                if params.iter().any(callback_component_is_unsupported)
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

        let inner = match projected.len() {
            0 => GoTy::Null,
            1 => projected.pop().expect("one projected union member"),
            arity if arity <= self.max_typed_union_arity => {
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
                if !aliases.insert(name.clone()) {
                    projected.push(GoTy::Unsupported);
                    return;
                }
                match &self.pool[name] {
                    Symbol::TypeAlias(alias) if !alias.recursive => {
                        self.project_union_member(&alias.resolves_to, aliases, projected);
                    }
                    _ => projected.push(GoTy::Unsupported),
                }
                aliases.remove(name);
            }
            other => projected.push(self.project_inner(other, aliases)),
        }
    }
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
        GoTy::List(inner) | GoTy::Optional(inner) => collect_typed_unions(inner, found),
        GoTy::Function(key) => {
            for param in key.params() {
                collect_typed_unions(param, found);
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

fn contains_unsupported(ty: &GoTy) -> bool {
    match ty {
        GoTy::Unsupported => true,
        GoTy::List(inner) | GoTy::Optional(inner) => contains_unsupported(inner),
        GoTy::Map { key, value } => contains_unsupported(key) || contains_unsupported(value),
        GoTy::TypedUnion(key) | GoTy::DynamicUnion { key, .. } => {
            key.members().iter().any(contains_unsupported)
        }
        GoTy::Function(key) => {
            key.params().iter().any(contains_unsupported)
                || key.ret().is_some_and(contains_unsupported)
        }
        _ => false,
    }
}

fn callback_component_is_unsupported(ty: &GoTy) -> bool {
    contains_unsupported(ty) || contains_function(ty) || contains_union(ty)
}

fn contains_function(ty: &GoTy) -> bool {
    match ty {
        GoTy::Function(_) => true,
        GoTy::List(inner) | GoTy::Optional(inner) => contains_function(inner),
        GoTy::Map { key, value } => contains_function(key) || contains_function(value),
        GoTy::TypedUnion(key) | GoTy::DynamicUnion { key, .. } => {
            key.members().iter().any(contains_function)
        }
        _ => false,
    }
}

fn contains_union(ty: &GoTy) -> bool {
    match ty {
        GoTy::TypedUnion(_) | GoTy::DynamicUnion { .. } => true,
        GoTy::List(inner) | GoTy::Optional(inner) => contains_union(inner),
        GoTy::Map { key, value } => contains_union(key) || contains_union(value),
        GoTy::Function(key) => {
            key.params().iter().any(contains_union) || key.ret().is_some_and(contains_union)
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
        assert_eq!(
            key.params(),
            &[GoTy::Int, GoTy::List(Box::new(GoTy::String))]
        );
        assert_eq!(key.ret(), Some(&GoTy::Bool));
        assert!(key.throws());
    }

    #[test]
    fn callback_projection_omits_optional_generic_nested_and_union_shapes() {
        let pool = SymbolPool::default();
        let projection = GoTypeProjection::new(&pool, 3);
        let never = || Ty::Never { attr: a() };
        let unsupported = [
            callable(
                vec![callable_param(
                    CodegenFunctionParamMode::Optional,
                    Ty::Int { attr: a() },
                )],
                Ty::String { attr: a() },
                never(),
            ),
            callable(
                vec![callable_param(
                    CodegenFunctionParamMode::Required,
                    Ty::TypeVar(BaseName::new("T"), a()),
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
            callable(
                vec![callable_param(
                    CodegenFunctionParamMode::Required,
                    union(vec![Ty::Int { attr: a() }, Ty::String { attr: a() }]),
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
    }
}

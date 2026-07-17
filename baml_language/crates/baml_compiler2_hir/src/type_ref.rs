//! Span-free type references — the HIR's own type-expression representation.
//!
//! `ast::TypeExpr` carries a `TextRange` on every node. That is correct for the
//! AST (spans are the syntax layer's job) but fatal for anything the HIR stores:
//! Salsa only overwrites a memoized value when it compares unequal, so a value
//! whose `PartialEq` ignores spans keeps the *old* spans forever after a
//! whitespace-only edit. A value cannot simultaneously be span-carrying,
//! compared semantically, and span-fresh.
//!
//! So the HIR stores `TypeRef` instead: the same tree, flattened into a
//! per-owner arena, with no spans anywhere. Spans live in a parallel
//! [`TypeRefSourceMap`] keyed by [`TypeRefId`], and are only reachable through
//! it. Attributes are the span-free [`Attribute`], not `ast::RawAttribute`
//! (whose span *is* part of its `PartialEq`, and which therefore leaks position
//! into type identity).
//!
//! The arena is scoped to **one item**. A file-wide arena would renumber every
//! later item's ids whenever an item was added, which would defeat the per-item
//! early cutoff this representation exists to enable.

use std::ops::Index;

use baml_base::{Literal, MediaKind, Name};
use la_arena::{Arena, Idx};
use text_size::TextRange;

use crate::item_tree::Attribute;

/// Identity of a type-reference node within its owning item's [`TypeRefStore`].
pub type TypeRefId = Idx<TypeRef>;

/// A type reference before name resolution — structurally identical to
/// `ast::TypeExprKind`, but with children addressed by [`TypeRefId`] and with
/// every span removed.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TypeRef {
    pub kind: TypeRefKind,
    /// Type-level attributes. Uniform across every kind, so — unlike
    /// `ast::TypeExprKind` — it is stored once here rather than repeated in
    /// all 24 variants.
    pub attrs: Box<[Attribute]>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TypeRefKind {
    /// Named type path: `User`, `baml.http.Request`, `Stream<T>`.
    Path {
        segments: Vec<Name>,
        /// Generic type arguments (`<T>` in `Stream<T>`). Empty for non-generic paths.
        generic_args: Box<[TypeRefId]>,
        /// Named associated type bindings, e.g. `Iterator<Item = int>`.
        associated_type_bindings: Box<[AssociatedTypeBindingRef]>,
    },
    /// Associated type projection: `Base.Item` or `(Base as Interface).Item`.
    AssociatedTypeProjection {
        base: TypeRefId,
        interface: Option<TypeRefId>,
        member: Name,
    },
    Int,
    Bigint,
    Float,
    String,
    Bool,
    Null,
    Never,
    /// The `void` type — valid only as a function return type.
    Void,
    /// `Uint8Array` (binary data).
    Uint8Array,
    Media {
        kind: MediaKind,
    },
    /// `T?`
    Optional {
        inner: TypeRefId,
    },
    /// `T[]`
    List {
        inner: TypeRefId,
    },
    /// `map<K, V>`
    Map {
        key: TypeRefId,
        value: TypeRefId,
    },
    /// `A | B | C`
    Union {
        variants: Box<[TypeRefId]>,
    },
    /// Literal types in unions: `"user"`, `200`, `3.14`, `true`.
    Literal {
        value: Literal,
    },
    /// Function type: `(params) -> return throws E`.
    Function {
        params: Box<[FunctionTypeParamRef]>,
        ret: TypeRefId,
        throws: Option<TypeRefId>,
    },
    /// The `unknown` keyword type.
    BuiltinUnknown,
    /// The `type` meta-type keyword.
    Type,
    /// `$rust_type` — opaque Rust-managed state field type.
    Rust,
    /// Error-recovery sentinel.
    Error,
    /// Missing/omitted type.
    Unknown,
    /// The wildcard `_` — an inference hole.
    Infer,
}

/// A parameter of a function type.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FunctionTypeParamRef {
    pub name: Option<Name>,
    pub optional: bool,
    pub ty: TypeRefId,
}

/// A named associated-type binding inside a type application.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AssociatedTypeBindingRef {
    pub name: Name,
    pub ty: TypeRefId,
}

/// The span-free type-reference arena owned by a single item.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TypeRefStore {
    types: Arena<TypeRef>,
}

impl TypeRefStore {
    pub fn get(&self, id: TypeRefId) -> &TypeRef {
        &self.types[id]
    }

    pub fn iter(&self) -> impl Iterator<Item = (TypeRefId, &TypeRef)> {
        self.types.iter()
    }

    pub fn len(&self) -> usize {
        self.types.len()
    }

    pub fn is_empty(&self) -> bool {
        self.types.is_empty()
    }
}

impl Index<TypeRefId> for TypeRefStore {
    type Output = TypeRef;
    fn index(&self, id: TypeRefId) -> &TypeRef {
        &self.types[id]
    }
}

/// Spans for one item's [`TypeRefStore`], allocated in lockstep with it, so the
/// two arenas share indices. Deliberately kept out of `TypeRefStore` — see the
/// module docs.
///
/// Unlike the store, this *does* compare spans: it is meant to invalidate on a
/// whitespace edit, precisely so the store doesn't have to.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TypeRefSourceMap {
    spans: Arena<TextRange>,
}

impl TypeRefSourceMap {
    /// The source span of `id`.
    ///
    /// Synthesized nodes (which have no source text) are allocated with an empty
    /// range; callers that need a user-visible anchor should fall back to the
    /// owning item's span.
    pub fn span(&self, id: TypeRefId) -> TextRange {
        // The two arenas are filled in lockstep by `TypeRefBuilder::alloc`, so a
        // `TypeRefId` indexes both. O(1) — do not reintroduce a scan here.
        self.spans[Idx::from_raw(id.into_raw())]
    }
}

/// Lowers `ast::TypeExpr` trees into one item's [`TypeRefStore`], recording each
/// node's span in the parallel [`TypeRefSourceMap`].
///
/// One builder per item: ids are only meaningful relative to the store they were
/// allocated in.
#[derive(Debug, Default)]
pub struct TypeRefBuilder {
    store: TypeRefStore,
    source_map: TypeRefSourceMap,
}

impl TypeRefBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Allocate a node and its span together. The lockstep is the invariant that
    /// makes `TypeRefSourceMap::span` a plain index rather than a lookup.
    fn alloc(&mut self, ty: TypeRef, span: TextRange) -> TypeRefId {
        let id = self.store.types.alloc(ty);
        let span_id = self.source_map.spans.alloc(span);
        debug_assert_eq!(
            id.into_raw(),
            span_id.into_raw(),
            "TypeRef and span arenas must stay in lockstep"
        );
        id
    }

    /// Lower a type expression and all of its children. Children are allocated
    /// before their parent (post-order), so ids are a pure function of tree
    /// shape — stable under whitespace edits.
    pub fn lower(&mut self, te: &baml_compiler2_ast::ast::TypeExpr) -> TypeRefId {
        use baml_compiler2_ast::ast::TypeExprKind as K;

        let attrs: Box<[Attribute]> = te.kind.attrs().iter().map(Attribute::from).collect();

        let kind = match &te.kind {
            K::Path {
                segments,
                generic_args,
                associated_type_bindings,
                ..
            } => TypeRefKind::Path {
                segments: segments.clone(),
                generic_args: generic_args.iter().map(|arg| self.lower(arg)).collect(),
                associated_type_bindings: associated_type_bindings
                    .iter()
                    .map(|binding| AssociatedTypeBindingRef {
                        name: binding.name.clone(),
                        ty: self.lower(&binding.ty),
                    })
                    .collect(),
            },
            K::AssociatedTypeProjection {
                base,
                interface,
                member,
                ..
            } => TypeRefKind::AssociatedTypeProjection {
                base: self.lower(base),
                interface: interface.as_ref().map(|iface| self.lower(iface)),
                member: member.clone(),
            },
            K::Int { .. } => TypeRefKind::Int,
            K::Bigint { .. } => TypeRefKind::Bigint,
            K::Float { .. } => TypeRefKind::Float,
            K::String { .. } => TypeRefKind::String,
            K::Bool { .. } => TypeRefKind::Bool,
            K::Null { .. } => TypeRefKind::Null,
            K::Never { .. } => TypeRefKind::Never,
            K::Void { .. } => TypeRefKind::Void,
            K::Uint8Array { .. } => TypeRefKind::Uint8Array,
            K::Media { kind, .. } => TypeRefKind::Media { kind: *kind },
            K::Optional { inner, .. } => TypeRefKind::Optional {
                inner: self.lower(inner),
            },
            K::List { inner, .. } => TypeRefKind::List {
                inner: self.lower(inner),
            },
            K::Map { key, value, .. } => TypeRefKind::Map {
                key: self.lower(key),
                value: self.lower(value),
            },
            K::Union { variants, .. } => TypeRefKind::Union {
                variants: variants.iter().map(|v| self.lower(v)).collect(),
            },
            K::Literal { value, .. } => TypeRefKind::Literal {
                value: value.clone(),
            },
            K::Function {
                params,
                ret,
                throws,
                ..
            } => TypeRefKind::Function {
                params: params
                    .iter()
                    .map(|p| FunctionTypeParamRef {
                        name: p.name.clone(),
                        optional: p.optional,
                        ty: self.lower(&p.ty),
                    })
                    .collect(),
                ret: self.lower(ret),
                throws: throws.as_ref().map(|t| self.lower(t)),
            },
            K::BuiltinUnknown { .. } => TypeRefKind::BuiltinUnknown,
            K::Type { .. } => TypeRefKind::Type,
            K::Rust { .. } => TypeRefKind::Rust,
            K::Error { .. } => TypeRefKind::Error,
            K::Unknown { .. } => TypeRefKind::Unknown,
            K::Infer { .. } => TypeRefKind::Infer,
        };

        self.alloc(TypeRef { kind, attrs }, te.span)
    }

    /// Allocate a node that has no source text (a compiler-synthesized type).
    /// Its span is empty; diagnostics should anchor to the owning item instead.
    pub fn alloc_synthetic(&mut self, kind: TypeRefKind) -> TypeRefId {
        self.alloc(
            TypeRef {
                kind,
                attrs: Box::new([]),
            },
            TextRange::default(),
        )
    }

    pub fn finish(self) -> (TypeRefStore, TypeRefSourceMap) {
        (self.store, self.source_map)
    }
}

#[cfg(test)]
mod tests {
    use baml_compiler2_ast::ast::{RawAttribute, RawAttributeArg, TypeExpr, TypeExprKind};
    use text_size::TextSize;

    use super::*;

    fn span(start: u32, end: u32) -> TextRange {
        TextRange::new(TextSize::from(start), TextSize::from(end))
    }

    fn attr(name: &str, value: &str, at: TextRange) -> RawAttribute {
        RawAttribute {
            name: Name::new(name),
            args: vec![RawAttributeArg {
                key: None,
                value: value.to_string(),
                span: at,
            }],
            span: at,
        }
    }

    /// `map<string, int[]>`, with every node's span offset by `shift`.
    fn map_of_string_to_int_list(shift: u32, attrs: Vec<RawAttribute>) -> TypeExpr {
        let s = |a: u32, b: u32| span(a + shift, b + shift);
        TypeExprKind::Map {
            key: Box::new(TypeExprKind::String { attrs: vec![] }.at(s(4, 10))),
            value: Box::new(
                TypeExprKind::List {
                    inner: Box::new(TypeExprKind::Int { attrs: vec![] }.at(s(12, 15))),
                    attrs: vec![],
                }
                .at(s(12, 17)),
            ),
            attrs,
        }
        .at(s(0, 18))
    }

    fn lower(te: &TypeExpr) -> (TypeRefStore, TypeRefSourceMap, TypeRefId) {
        let mut builder = TypeRefBuilder::new();
        let root = builder.lower(te);
        let (store, source_map) = builder.finish();
        (store, source_map, root)
    }

    #[test]
    fn children_are_allocated_before_parents() {
        let (store, _, root) = lower(&map_of_string_to_int_list(0, vec![]));

        // Post-order: the root is allocated last.
        assert_eq!(root, store.iter().last().expect("non-empty store").0);

        // string, int, int[], map — post-order.
        assert_eq!(store.len(), 4);
        let kinds: Vec<_> = store.iter().map(|(_, t)| t.kind.clone()).collect();
        assert!(matches!(kinds[0], TypeRefKind::String));
        assert!(matches!(kinds[1], TypeRefKind::Int));
        assert!(matches!(kinds[2], TypeRefKind::List { .. }));
        assert!(matches!(kinds[3], TypeRefKind::Map { .. }));
    }

    /// The property the whole firewall rests on: shifting every span (a
    /// whitespace-only edit) must leave the semantic value byte-for-byte equal,
    /// so Salsa cuts off — while the source map still reports the *new* spans.
    #[test]
    fn shifting_spans_does_not_change_the_store() {
        let (unshifted, map_a, root_a) = lower(&map_of_string_to_int_list(0, vec![]));
        let (shifted, map_b, root_b) = lower(&map_of_string_to_int_list(100, vec![]));

        assert_eq!(
            unshifted, shifted,
            "a whitespace-only edit must not change the semantic type refs"
        );

        assert_eq!(map_a.span(root_a), span(0, 18));
        assert_eq!(
            map_b.span(root_b),
            span(100, 118),
            "the source map must still track the new positions"
        );
    }

    /// `ast::RawAttribute` puts its span in its own `PartialEq`, so attribute
    /// spans leak into `TypeExpr` equality and silently destroy cutoff near any
    /// `@description`. `TypeRef` carries the span-free `Attribute` instead.
    #[test]
    fn shifting_attribute_spans_does_not_change_the_store() {
        let a = map_of_string_to_int_list(0, vec![attr("description", "hi", span(19, 40))]);
        let b = map_of_string_to_int_list(0, vec![attr("description", "hi", span(219, 240))]);

        assert_ne!(
            a, b,
            "precondition: the AST does compare attribute spans (this is the leak)"
        );

        let (store_a, _, _) = lower(&a);
        let (store_b, _, _) = lower(&b);
        assert_eq!(
            store_a, store_b,
            "moving an attribute must not change the semantic type refs"
        );
    }

    /// A real structural change must still compare unequal, or we would cut off
    /// edits that actually matter.
    #[test]
    fn structural_changes_do_change_the_store() {
        let (int_keyed, _, _) = lower(&map_of_string_to_int_list(0, vec![]));
        let (bool_keyed, _, _) = lower(
            &TypeExprKind::Map {
                key: Box::new(TypeExprKind::Bool { attrs: vec![] }.at(span(4, 10))),
                value: Box::new(
                    TypeExprKind::List {
                        inner: Box::new(TypeExprKind::Int { attrs: vec![] }.at(span(12, 15))),
                        attrs: vec![],
                    }
                    .at(span(12, 17)),
                ),
                attrs: vec![],
            }
            .at(span(0, 18)),
        );

        assert_ne!(int_keyed, bool_keyed);
    }
}

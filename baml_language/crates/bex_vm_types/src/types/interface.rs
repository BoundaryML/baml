use borsh::{BorshDeserialize, BorshSerialize};
use indexmap::IndexMap;

use crate::{HeapPtr, ObjectIndex};

#[derive(Debug, Clone, BorshSerialize, BorshDeserialize)]
pub struct InterfaceDef {
    // Signature
    pub name: baml_type::TypeName,

    /// This interface's head identity, content-addressed from its
    /// fully-qualified name at emit time — the identity a `TypeHead` referring
    /// to this interface compares by.
    ///
    /// Interfaces have no dispatch tag: an interface is an existential, so no
    /// *value* is ever "of" an interface in the sense `TypeTag` reports. This is
    /// identity only.
    pub type_tag: baml_type::typetag::TypeTag,
    pub args: Vec<(baml_type::Name, Vec<crate::RuntimeInterface>)>,
    pub requires: Vec<crate::RuntimeInterface>,

    // Member Types
    pub assoc: Vec<(baml_type::Name, crate::RuntimeInterface)>,
    /// The interface's declared fields, in declaration order. **Position is
    /// identity**: a field's index here is the index every implementation's
    /// [`RuntimeImplRule::field_links`] is baked against, and the index a
    /// `VirtualLoadField` / `VirtualStoreField` carries. Entries are therefore never
    /// dropped, reordered, or deduplicated.
    pub fields: Vec<InterfaceFieldDef>,
    pub methods: Vec<InterfaceMethodDef>,

    /// Runtime package that declared this interface; null for a static
    /// declaration. A member back-edge: reaching the interface keeps its
    /// package alive, the same ownership shape ``Class::owner``
    /// gives classes and enums. This is a GC edge, never serialized.
    #[borsh(skip)]
    pub owner: HeapPtr,
}

/// One field an interface declares, at its dispatch index (its position in
/// [`InterfaceDef::fields`]).
#[derive(Debug, Clone, BorshSerialize, BorshDeserialize)]
pub struct InterfaceFieldDef {
    pub name: baml_type::Name,
    /// The field's declared type, exactly as written. An interface field's type may
    /// depend on the implementor (`key: Self.Key`); such a type stays a structural
    /// `TypeVar`/`AssociatedTypeProjection` here — the `RuntimeTy` family carries
    /// both — and is resolved dynamically against the receiver's impl, never
    /// pre-collapsed to a frame slot (an interface *declaration* has no frame).
    pub ty: crate::RuntimeTy,
}

#[derive(Debug, Clone, BorshSerialize, BorshDeserialize)]
pub struct InterfaceMethodDef {
    pub name: baml_type::Name,
    pub args: Vec<crate::RuntimeTy>,
    pub kwargs: Vec<(baml_type::Name, crate::RuntimeTy)>,
    pub returns: crate::RuntimeTy,
    pub errors: crate::RuntimeTy,
    /// The default body's pooled function, when the interface supplies one;
    /// `None` for a required method. This is the wire form: an [`ObjectIndex`]
    /// into the program's object pool, relocated by the linker like any other
    /// cross-object operand (see `relink::visit_object_operands`) and bound to
    /// [`default_fn`](Self::default_fn) at load — exactly as
    /// [`ProgramMethodImpl::fqn`](super::ProgramMethodImpl::fqn) becomes
    /// [`MethodImpl::fqn`](MethodImpl::fqn).
    pub default: Option<ObjectIndex>,
    /// The loaded default body: null for a required method, or for an
    /// interface whose pool has not been bound yet. Never serialized —
    /// pointers are runtime-only. Runtime consumers (a witness synthesizing an
    /// implementor's rule) read this and never a name.
    #[borsh(skip)]
    pub default_fn: HeapPtr,
}

/// A single interface bound on an impl's generic parameter — `T extends I`, or a
/// generic / associated-bound form (`T extends Container<U>`, `T extends
/// Iterator<Item = int>`). `args` and `assoc` are `TyTemplate`s over the impl's
/// params (`U` → `TypeArgRef`); the resolver substitutes them with the match
/// bindings and then checks the bound type argument implements `interface` *at
/// those args/assoc* — the runtime twin of a `T: Iface<Args, Assoc = …>`
/// predicate instantiated with the impl substitutions (rustc). Bounds are
/// interfaces, not types, so an intersection of bounds is a *set* of these.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct InterfaceBound {
    pub interface: crate::TypeHead,
    pub args: Vec<crate::TyTemplate>,
    pub assoc: Vec<(baml_type::Name, crate::TyTemplate)>,
}

/// A resolved interface-method implementation in a [`RuntimeImplRule`]: the
/// callee's body pointer plus the `frame` it must be invoked with.
///
/// THE FRAME CONVENTION. Every interface-machinery body expects the frame
/// `[owner ++ own generics]`, and there are exactly two owner shapes:
///
/// - a **provided method** (the impl block declares it) was compiled against
///   the impl's generic params — owner = the impl's generics, in declaration
///   order;
/// - an **adopted default** (the impl declares nothing; the interface's
///   default body serves — Rust-trait adoption, not inheritance) was compiled
///   against the interface's frame — owner = `[Self, interface generic
///   args..]`. Associated types are NOT frame slots: an assoc is determined
///   by the `(Self, interface, member)` triple, so a default body's
///   `Self.Assoc` is a projection template that reduces through the rule's
///   `interface_assoc` bindings at realization.
///
/// `frame` here is the owner half as templates (De Bruijn over the impl's
/// generic params), realized against the impl's bound type args at dispatch;
/// the call site appends the method's own type args after it. The supplier is
/// determined by HOW the callee was selected — through a rule, these
/// templates realized against the match; statically, the frame the compile-
/// time resolution carried — never re-derived from a name. Realizing this
/// frame is what lets a default like `Iterator.collect` resolve `Self` and
/// its projections under an open-world virtual call.
// No `PartialEq`/`Eq`: `fqn` is a `HeapPtr`, so a derived `Eq` would be pointer
// identity rather than structural equality — a footgun with no current caller.
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub struct MethodImpl {
    pub fqn: HeapPtr,
    pub frame: Vec<crate::TyTemplate>,
}

/// One interface implementation, baked for the runtime resolver
/// (`resolve_implements_rule`) — the analog of a rustc `ImplSource` plus its
/// resolved method `Instance`s. Mirrors the compiler's `InterfaceImplRule`
/// (`baml_compiler2_hir_ty::interfaces`) with the method handles attached.
// No `PartialEq`/`Eq`: `interface_head` (and `methods`' `fqn`) is a `HeapPtr`, so
// a derived `Eq` would be pointer identity, not structural equality.
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub struct RuntimeImplRule {
    /// Points at the [`InterfaceDef`] for this implementation.
    pub interface_head: HeapPtr,
    /// The implementor pattern; a `TyTemplate::TypeArgRef(n)` leaf is the impl's
    /// n-th generic parameter (de Bruijn). E.g. `implement<T> I for Wrap<T>` →
    /// `Class(Wrap, [TypeArgRef(0)])`, `implement I for Foo` →
    /// `Concrete(Class(Foo, []))`, `implement<T> I for T[]` →
    /// `Array(TypeArgRef(0))`.
    pub for_ty_pattern: crate::TyTemplate,
    /// Per impl generic parameter (de Bruijn-indexed), the *set* of interface
    /// bounds it must satisfy. `T extends A & B` is the set `{A, B}`; an empty set
    /// is unbounded (an intersection of bounds is a set of interfaces, not a
    /// type, since interfaces aren't types). A bound may be generic or carry
    /// associated bindings ([`InterfaceBound`]). The resolver discharges each as
    /// a nested obligation — the bound type argument must implement *every*
    /// interface in its set, at the bound's substituted args/assoc (rustc's
    /// where-clause-as-obligation) — so a bounded impl never matches a
    /// non-satisfying argument.
    pub generic_param_bounds: Vec<Vec<InterfaceBound>>,
    /// Type args of the implemented interface (for generic interfaces such as
    /// `Container<T>`; empty for interfaces with no type parameters). Lets
    /// reflection distinguish instantiations.
    pub interface_args: Vec<crate::TyTemplate>,
    /// Associated-type bindings of the implemented interface.
    pub interface_assoc: Vec<(baml_type::Name, crate::TyTemplate)>,
    /// Method name → its [`MethodImpl`] (the callee's `Object::Function` pointer +
    /// invocation frame), resolved at dispatch time. Complete: the methods this
    /// impl provides *plus* the interface's default methods it adopts (the bake
    /// merges them in, a provided method winning over the default), so a lookup
    /// resolves any interface method.
    pub methods: IndexMap<baml_type::Name, MethodImpl>,
    /// Interface field index → the implementor's physical slot in
    /// [`Instance::fields`](super::Instance::fields). Indexed by position in the
    /// implemented [`InterfaceDef::fields`], so a virtual field access carries only
    /// that index — no name on the operand stack, no lookup at dispatch.
    ///
    /// **Positional and total**: exactly one entry per field the interface declares,
    /// with the explicit `field as class_field` link applied and the same-name
    /// default filled in. E0124 (every interface field covered) is what makes
    /// totality a fact about well-formed programs rather than a runtime check, so
    /// resolution is an in-bounds index and never fails.
    ///
    /// A class slot index is well-defined here because E0126 confines every
    /// field-bearing impl to an in-body `implements` block: `for_ty_pattern`'s head
    /// is then exactly one class, whose layout is fixed and lives in the same
    /// package as this rule. Empty for an impl of a field-less interface.
    pub field_links: Box<[u32]>,
}

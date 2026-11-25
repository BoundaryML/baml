// Copied from crates/bex_vm_types/src/types/interface.rs: `InterfaceBound`, `MethodImpl` and
// `RuntimeImplRule` (here `ImplRule`), with layouts the compiler can write. A change to either
// file must be made to both.

use std::ptr::NonNull;

use super::{
    AssocBinding, CowSlice, InterfaceObject, PackageObject, RawFunctionObject, SlotIndex,
    TyTemplate,
};

/// A single interface bound on an impl's generic parameter — `T extends I`, or a
/// generic / associated-bound form (`T extends Container<U>`, `T extends
/// Iterator<Item = int>`). `args` and `assoc` are `TyTemplate`s over the impl's
/// params (`U` → `TypeArgRef`); the resolver substitutes them with the match
/// bindings and then checks the bound type argument implements `interface` *at
/// those args/assoc* — the runtime twin of a `T: Iface<Args, Assoc = …>`
/// predicate instantiated with the impl substitutions (rustc). Bounds are
/// interfaces, not types, so an intersection of bounds is a *set* of these.
///
/// Over `RealizedTy`, the same shape is the realized interface constraint
/// the original spells `Interface` (the family's satellite): what a
/// projection's qualifier realizes to and what the type algebra asks its
/// context about.
#[repr(C)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InterfaceBound<T = TyTemplate> {
    pub interface: NonNull<InterfaceObject>,
    pub args: CowSlice<T>,
    pub assoc: CowSlice<AssocBinding<T>>,
}

/// A resolved interface-method implementation in an [`ImplRule`]: the
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
// No `PartialEq`/`Eq`: `function` is an object's address, so a derived `Eq` would be pointer
// identity rather than structural equality — a footgun with no current caller.
#[repr(C)]
#[derive(Clone, Debug)]
pub struct MethodImpl {
    /// A heap object like any other function object.
    pub function: NonNull<RawFunctionObject>,
    /// The type arguments the method was compiled against, over the rule's own type parameters.
    pub frame: CowSlice<TyTemplate>,
}

/// One interface implementation, baked for the runtime resolver
/// (`resolve_implements_rule`) — the analog of a rustc `ImplSource` plus its
/// resolved method `Instance`s. Mirrors the compiler's `InterfaceImplRule`
/// (`baml_compiler2_hir_ty::interfaces`) with the method handles attached.
///
/// Not a heap object and never a value: no header, no tag. The same struct
/// whether the compiler emitted it or the program added it: the lists of an
/// emitted one are static, those of an added one owned. A rule lives in its
/// interface's `InterfaceObject`, which is how it names the interface.
// No `PartialEq`/`Eq`: `owner` (and `methods`' `function`) is an object's address, so
// a derived `Eq` would be pointer identity, not structural equality.
#[repr(C)]
#[derive(Clone, Debug)]
pub struct ImplRule {
    /// The package that declared the rule, which may not be the interface's.
    pub owner: NonNull<PackageObject>,
    /// The implementor pattern; a `TyTemplate::TypeArgRef(n)` leaf is the impl's
    /// n-th generic parameter (de Bruijn). E.g. `implement<T> I for Wrap<T>` →
    /// `Class(Wrap, [TypeArgRef(0)])`, `implement I for Foo` →
    /// `Concrete(Class(Foo, []))`, `implement<T> I for T[]` →
    /// `Array(TypeArgRef(0))`.
    pub for_ty_pattern: TyTemplate,
    /// Per impl generic parameter (de Bruijn-indexed), the *set* of interface
    /// bounds it must satisfy. `T extends A & B` is the set `{A, B}`; an empty set
    /// is unbounded (an intersection of bounds is a set of interfaces, not a
    /// type, since interfaces aren't types). A bound may be generic or carry
    /// associated bindings ([`InterfaceBound`]). The resolver discharges each as
    /// a nested obligation — the bound type argument must implement *every*
    /// interface in its set, at the bound's substituted args/assoc (rustc's
    /// where-clause-as-obligation) — so a bounded impl never matches a
    /// non-satisfying argument.
    pub generic_param_bounds: CowSlice<CowSlice<InterfaceBound>>,
    /// Type args of the implemented interface (for generic interfaces such as
    /// `Container<T>`; empty for interfaces with no type parameters). Lets
    /// reflection distinguish instantiations.
    pub interface_args: CowSlice<TyTemplate>,
    /// Associated-type bindings of the implemented interface: what `Self.Item`
    /// reduces through.
    pub interface_assoc: CowSlice<AssocBinding<TyTemplate>>,
    /// Interface method index → its [`MethodImpl`] (the callee's function object +
    /// invocation frame), resolved at dispatch time. PROVIDED-ONLY: exactly the
    /// methods this impl block declares; `None` at the index of one it does not.
    /// A method the impl does not provide is
    /// ADOPTED from the interface at resolution — the resolver falls back to
    /// the interface's bound `InterfaceMethod::default_fn` with the
    /// adopted-default frame `[Self, interface args..]` (spelled over this
    /// rule's `for_ty_pattern` ++ `interface_args`) — so the table is a pure
    /// function of the impl block, stable under interface evolution.
    pub methods: CowSlice<Option<MethodImpl>>,
    /// Interface field index → the implementor's physical slot in
    /// `InstanceObject::slots`. Indexed by position in the
    /// implemented interface's fields, so a virtual field access carries only
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
    pub field_links: CowSlice<SlotIndex>,
}

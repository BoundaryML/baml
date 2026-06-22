use borsh::{BorshDeserialize, BorshSerialize};
use indexmap::IndexMap;

pub type InterfaceAssociatedBindings = Vec<(baml_type::Name, baml_type::RuntimeTy)>;
pub type InterfaceImplementorEntry = (
    baml_type::TypeName,
    Vec<baml_type::RuntimeTy>,
    InterfaceAssociatedBindings,
);

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
    pub interface: baml_type::TypeName,
    pub args: Vec<baml_type::TyTemplate>,
    pub assoc: Vec<(baml_type::Name, baml_type::TyTemplate)>,
}

/// A resolved interface-method implementation in a [`RuntimeImplRule`]: the
/// callee's fully-qualified name plus the `frame` it must be invoked with.
///
/// `frame` is the callee's type-argument layout as templates (De Bruijn over the
/// impl's generic params), realized against the impl's bound type args at
/// dispatch. For an impl's **own** method this is the impl's own generics; for an
/// **inherited default** it is the interface's generic args followed by its
/// associated types in *declared* order — the default was compiled against the
/// interface's frame (its body refers to the interface's associated types), not
/// the implementor's generics. Realizing this frame is what lets a default like
/// `Iterator.collect` resolve `Item`/`Error` under an open-world virtual call.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct MethodImpl {
    pub fqn: String,
    pub frame: Vec<baml_type::TyTemplate>,
}

/// One interface implementation, baked for the runtime resolver
/// (`resolve_implements_rule`) — the analog of a rustc `ImplSource` plus its
/// resolved method `Instance`s. Mirrors the compiler's `InterfaceImplRule`
/// (`baml_compiler2_tir::interfaces`) with the method handles attached.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct RuntimeImplRule {
    /// The implementor pattern; a `TyTemplate::TypeArgRef(n)` leaf is the impl's
    /// n-th generic parameter (de Bruijn). E.g. `implement<T> I for Wrap<T>` →
    /// `Class(Wrap, [TypeArgRef(0)])`, `implement I for Foo` →
    /// `Concrete(Class(Foo, []))`, `implement<T> I for T[]` →
    /// `Array(TypeArgRef(0))`.
    pub for_ty_pattern: baml_type::TyTemplate,
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
    pub interface_args: Vec<baml_type::TyTemplate>,
    /// Associated-type bindings of the implemented interface.
    pub interface_assoc: Vec<(baml_type::Name, baml_type::TyTemplate)>,
    /// Method name → its [`MethodImpl`] (callee FQN + invocation frame), resolved
    /// to a callee at dispatch time. Complete: the methods this impl overrides
    /// *plus* the interface's inherited default methods (the bake merges them in,
    /// an override winning over the default), so a lookup resolves any interface
    /// method. (A direct global index/handle would be faster and may replace the
    /// FQN later.)
    pub methods: IndexMap<baml_type::Name, MethodImpl>,
}

/// A single package's interface-implementation table, keyed by the implemented
/// interface's base `TypeName`; each value lists the impls of that interface
/// **declared in this package**. The runtime resolver selects the rule whose
/// `for_ty_pattern` matches a value's concrete type (with bounds satisfied),
/// mirroring rustc trait selection. See `Program::interface_impls` for how these
/// per-package tables are combined.
pub type InterfaceImpls = IndexMap<baml_type::TypeName, Vec<RuntimeImplRule>>;

/// The whole-program interface registry: package name → that package's
/// [`InterfaceImpls`]. Split by package so a dynamically-loaded package adds an
/// entry without rebuilding the others.
pub type InterfaceImplsByPackage = IndexMap<baml_type::Name, InterfaceImpls>;

//! The `Ty` type family, generated from a single tagged definition.
//!
//! [`ty_family!`](baml_type_macros::ty_family) expands the master `Ty` enum
//! below into the whole family — `Ty`, [`RuntimeTy`], [`CodegenTy`],
//! [`RealizedTy`], [`ConcreteTy`], [`ConcreteRealizedTy`], [`TyTemplate`] —
//! plus the per-member `FunctionParamTy` companion structs. Membership is by
//! axis: each variant is tagged with exactly one `#[axis(..)]`, and each
//! member includes a set of axes (a variant is present iff its axis is
//! included). Nested positions are
//! retargeted per member: deep members (`child: Self`) recurse into themselves;
//! the shallow `Concrete*` members nest their declared `child`.
//!
//! Every member is parameterized by `N`, the representation of a *nominal type
//! head* — the name a `Class`, `Enum`, `Interface`, or `TypeAlias` refers to.
//! It defaults to [`TypeName`], so `Ty` still means `Ty<TypeName>` wherever the
//! compiler writes it bare; the parameter exists so the runtime can carry
//! interned or heap-anchored heads instead, without a parallel type family.
//! Note that `N` covers *heads only*: [`Name`] positions (a field, an enum
//! variant, an associated-type binding) are member names, not type references,
//! and stay concrete.
//!
//! `N: Clone` is required at the declaration because a family member is a
//! cloneable value tree — the derived `Clone` needs it, and so does
//! `BorshDeserialize` for the `Box<Ty<N>>` positions (via `ToOwned`). Any head
//! representation worth having (a name, an interned id, a heap handle) is
//! cloneable, so this rules out nothing real.
//!
//! The semantic impls (`render_with`, `Display`, `validate_runtime`, the
//! conversions, and the `lower_to_runtime` boundary)
//! stay hand-written in `lib.rs`, `runtime_ty.rs`, and `realized_ty.rs`.

use baml_type_macros::ty_family;
use borsh::{BorshDeserialize, BorshSerialize};

use crate::{Freshness, FunctionParamMode, Literal, MediaKind, Name, ParamTy, TyAttr, TypeName};

ty_family! {
    axes { concrete, abstract, literal, never, typevar, projection, tir, special, frame }

    type Ty                 { includes: [concrete, abstract, literal, never, typevar, projection, tir, special], child: Self }
    type RuntimeTy          { includes: [concrete, abstract, literal, never, typevar, projection, special],      child: Self }
    // A deep, generator-independent public API type. Unlike `RuntimeTy`, this
    // excludes unresolved associated-type projections; unlike `RealizedTy`, it
    // retains named type variables for generic declarations. Type aliases are
    // deliberately retained as nominal references at public use sites.
    type CodegenTy          { includes: [concrete, abstract, literal, never, typevar, special],                  child: Self }
    type RealizedTy         { includes: [concrete, abstract, literal, never, special],                           child: Self }
    type ConcreteTy         { includes: [concrete, never],                                                       child: RuntimeTy }
    type ConcreteRealizedTy { includes: [concrete, never],                                                       child: RealizedTy }
    // A *complete* `Ty`-shaped template: every leaf is either realized or a
    // positional `frame`-axis reference (`TypeArgRef`), so `substitute` always
    // has a single concrete answer per position. It swaps the `typevar` axis's
    // name-based `TypeVar` for `TypeArgRef`, while keeping the `projection`
    // axis so a symbolic associated projection can still be carried
    // structurally. `RealizedTy` is its fully-resolved subset (every
    // `RealizedTy` is a valid `TyTemplate`; narrowing back proves no template
    // leaf survives), giving the "is fully realized" check for free.
    type TyTemplate         { includes: [concrete, abstract, literal, never, projection, special, frame],        child: Self }

    satellite FunctionParamTy<N: Clone = TypeName> {
        pub name: Option<Name>,
        pub ty: Ty<N>,
        pub mode: FunctionParamMode,
    } methods {
        pub fn required(name: Option<Name>, ty: Ty<N>) -> Self {
            Self {
                name,
                ty,
                mode: FunctionParamMode::Required,
            }
        }

        pub fn optional(name: Option<Name>, ty: Ty<N>) -> Self {
            Self {
                name,
                ty,
                mode: FunctionParamMode::Optional,
            }
        }

        pub fn is_required(&self) -> bool {
            matches!(self.mode, FunctionParamMode::Required)
        }

        pub fn is_optional(&self) -> bool {
            matches!(self.mode, FunctionParamMode::Optional)
        }
    }

    // An interface *constraint*: a reference to interface `name` at the given
    // input `generics`, plus any `associated_types` it pins. This is the
    // declaring interface of an `AssociatedTypeProjection` and the subject of a
    // generic bound — distinct from `Ty::Interface`, the interface *existential*
    // type, which requires every associated type to be specified. (A `//` comment,
    // not `///`: the `ty_family!` satellite grammar has no slot for a leading doc.)
    satellite Interface<N: Clone = TypeName> {
        pub name: N,
        /// The interface's generic *input* arguments, in declaration order.
        /// Resolved from context; never defaulted away — they are part of the
        /// interface's identity (`Converter<int>` ≠ `Converter<float>`).
        pub generics: Vec<Ty<N>>,
        /// Associated-type bindings that further constrain the implementor (the
        /// `Item = int` in `Iterator<Item = int>`). Real constraints carried as
        /// part of the interface, never stripped; sorted by name for a
        /// deterministic order.
        pub associated_types: Vec<(Name, Ty<N>)>,
    } methods {
        /// Build an interface constraint, sorting `associated_types` by name so the
        /// invariant the field documents holds and the derived `Eq`/`Hash`/`Ord`
        /// are order-insensitive. Normalization sorts bindings identically, so a
        /// constraint built here compares equal to its normalized form. Generated
        /// for every family member (`Interface`, `RuntimeInterface`, …), so every
        /// construction site — including the untrusted ctypes decode boundary —
        /// can route through it instead of sorting by hand.
        pub fn new(name: N, generics: Vec<Ty<N>>, mut associated_types: Vec<(Name, Ty<N>)>) -> Self {
            associated_types.sort_by(|(a, _), (b, _)| a.cmp(b));
            Self {
                name,
                generics,
                associated_types,
            }
        }
    }

    /// The unified type representation for BAML, used from VIR through runtime.
    ///
    /// Contains both core runtime variants and compiler-only variants.
    /// Runtime code should use `unreachable!()` for compiler-only variants.
    /// Runtime code should call `validate_runtime()` to catch any that leak.
    ///
    /// Every variant carries an `attr: TyAttr` (or trailing `TyAttr` for tuple
    /// variants) that holds SAP streaming annotations. All existing code uses
    /// `TyAttr::default()` — only stream type generation (HIR lowering) will populate
    /// non-default values.
    #[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, BorshSerialize, BorshDeserialize)]
    #[borsh(use_discriminant = true)]
    pub enum Ty<N: Clone = TypeName> {
        #[axis(concrete)]
        Int {
            attr: TyAttr,
        } = 0,
        #[axis(concrete)]
        Bigint {
            attr: TyAttr,
        } = 1,
        #[axis(concrete)]
        Float {
            attr: TyAttr,
        } = 2,
        #[axis(concrete)]
        String {
            attr: TyAttr,
        } = 3,
        #[axis(concrete)]
        Bool {
            attr: TyAttr,
        } = 4,
        #[axis(concrete)]
        Null {
            attr: TyAttr,
        } = 5,
        #[axis(concrete)]
        Uint8Array {
            attr: TyAttr,
        } = 6,
        #[axis(concrete)]
        Media(MediaKind, TyAttr) = 7,
        /// A literal type — a single value (`1`, `"hi"`, `true`) as a type. The
        /// [`Freshness`] flag is compiler-only (fresh literals widen at mutable
        /// binding sites); it is normalized to `Regular` at the runtime boundary.
        #[axis(literal)]
        Literal(Literal, Freshness, TyAttr) = 8,
        #[axis(concrete)]
        Class(N, Vec<Ty<N>>, TyAttr) = 9,
        /// An interface existential type, equivalent to Rust `dyn Trait`.
        /// Must specify all generic type args and all associated types.
        #[axis(abstract)]
        Interface(N, Vec<Ty<N>>, Vec<(Name, Ty<N>)>, TyAttr) = 10,
        #[axis(concrete)]
        Enum(N, TyAttr) = 11,
        /// A specific enum variant — `Status.HttpError`.
        #[axis(literal)]
        EnumVariant(N, Name, TyAttr) = 12,
        #[axis(concrete)]
        List(Box<Ty<N>>, TyAttr) = 13,
        #[axis(concrete)]
        Map {
            key: Box<Ty<N>>,
            value: Box<Ty<N>>,
            attr: TyAttr,
        } = 14,
        #[axis(abstract)]
        Union(Vec<Ty<N>>, TyAttr) = 15,

        /// Function/arrow type: `(T1, T2, ...) -> R throws E`.
        #[axis(concrete)]
        Function {
            params: Vec<FunctionParamTy<N>>,
            ret: Box<Ty<N>>,
            throws: Box<Ty<N>>,
            attr: TyAttr,
        } = 16,
        /// A future handle — the result of `schedule_future` or `spawn`
        /// before `await`.
        ///
        /// Carries both the value type the future resolves to and the error
        /// type the future may throw. A future whose body statically cannot
        /// throw has error type `never`.
        #[axis(concrete)]
        Future(Box<Ty<N>>, Box<Ty<N>>, TyAttr) = 17,
        /// Opaque Rust-managed state (`$rust_type` fields in builtin class stubs,
        /// e.g. `Media._data`). A leaf concrete type with no inner structure.
        ///
        /// Renders as `$rust_type` (qualified name `baml.rust.RustType`).
        #[axis(concrete)]
        RustType {
            attr: TyAttr,
        } = 18,
        /// The `type` metatype keyword — a runtime value that wraps a `Ty`
        /// (reflection). A leaf concrete type.
        ///
        /// Renders as the `type` keyword (qualified name `baml.reflect.Type`).
        #[axis(concrete)]
        Type {
            attr: TyAttr,
        } = 19,
        /// Opaque resource handle — file, socket, or HTTP response body. A leaf
        /// concrete type whose *values* are concrete Rust types on the VM heap; the
        /// type system treats it nominally (no structural decomposition).
        ///
        /// Renders as its qualified name `ai.Resource`.
        #[axis(concrete)]
        Resource {
            attr: TyAttr,
        } = 20,
        /// Opaque structured prompt tree for LLM calls. A leaf concrete type whose
        /// *values* are concrete Rust types on the VM heap; the type system treats
        /// it nominally (no structural decomposition).
        ///
        /// Renders as its qualified name `ai.Prompt`.
        #[axis(concrete)]
        PromptAst {
            attr: TyAttr,
        } = 21,

        /// Void type — the type of effectful expressions (was VIR `Unit`).
        #[axis(special)]
        Void {
            attr: TyAttr,
        } = 22,
        // reserved = 23
        /// Only recursive aliases survive lower_ty; non-recursive are expanded.
        #[axis(special)]
        TypeAlias(N, TyAttr) = 24,
        /// A type variable (generic parameter) — e.g. `T` in `Array<T>`. Bound
        /// during inference; can survive at runtime only inside reflective generic
        /// metadata.
        #[axis(typevar)]
        TypeVar(ParamTy, TyAttr) = 25,
        /// Associated type projection, e.g. `P.Output` or `(T as Iterator).Item`. Bound
        /// during inference; can survive at runtime only inside reflective generic
        /// metadata. Split into its own `projection` axis (distinct from `typevar`)
        /// so a template can carry an unresolved projection without also admitting
        /// a name-based `TypeVar`.
        #[axis(projection)]
        AssociatedTypeProjection {
            base: Box<Ty<N>>,
            /// The declaring interface of this projection — always known: the TIR
            /// resolves `(base as I).member` to its interface `I` (or lowers to
            /// `Ty::Error` when it cannot be determined), so a resolved projection
            /// never lacks its qualifier. This is what lets a realized-base
            /// projection reduce to the impl's binding at substitution time.
            interface: Box<Interface<N>>,
            member: Name,
            attr: TyAttr,
        } = 26,
        /// The top type - may have any concrete value.
        ///
        /// Similar to TypeScript's `unknown` - any value can be passed where
        /// `BuiltinUnknown` is expected, but `BuiltinUnknown` cannot be used
        /// where a specific type is required.
        ///
        /// Used in llm.baml for functions like:
        /// ```baml
        /// function render_prompt(function_name: string, args: map<string, unknown>) -> PromptAst
        /// ```
        #[axis(abstract)]
        BuiltinUnknown {
            attr: TyAttr,
        } = 27,
        /// The bottom type — an expression that never produces a value (`return`,
        /// `break`, `continue`, diverging blocks). A subtype of every type.
        #[axis(never)]
        Never {
            attr: TyAttr,
        } = 28,

        // --- TIR-only: present during type checking, erased at the runtime
        // boundary (`lower_to_runtime`). Carried only by `Ty` (the `tir` axis).
        /// Error-recovery sentinel: the type is structurally unknown (e.g. an
        /// unresolved name). Distinct from `BuiltinUnknown` (a well-formed top type).
        #[axis(tir)]
        Unknown {
            attr: TyAttr,
        } = 29,
        /// Error sentinel: a hard type error was emitted for this expression.
        #[axis(tir)]
        Error {
            attr: TyAttr,
        } = 30,
        /// Evolving list — an empty `[]` literal at a mutable binding whose element
        /// type is refined by mutations. Frozen to `List` at the runtime boundary.
        #[axis(tir)]
        EvolvingList(Box<Ty<N>>, TyAttr) = 31,
        /// Evolving map — the map analogue of [`Ty::EvolvingList`].
        #[axis(tir)]
        EvolvingMap(Box<Ty<N>>, Box<Ty<N>>, TyAttr) = 32,
        /// Inference hole — the wildcard `_` written in a type-argument or
        /// `throws`-clause position. A leaf placeholder that asks the checker to
        /// infer the type at this slot from surrounding context (the initializer
        /// of a `let`, or the inferred effective throw set). Filled during TIR
        /// checking; like the other `tir`-axis sentinels it must never survive to
        /// the runtime boundary (`lower_to_runtime` rejects it).
        #[axis(tir)]
        Infer {
            attr: TyAttr,
        } = 33,

        // --- Template-only: a positional reference into an enclosing frame's
        // type arguments, present only in `TyTemplate` (the `frame` axis). It
        // always materializes to exactly one type. It carries no `TyAttr` — it
        // is pure structure — so the generated `attr()`/`with_attr()`
        // accessors fall back to `TyAttr::EMPTY`.
        /// A De Bruijn reference to the n-th type argument of the enclosing
        /// call frame. Materialized to a concrete type by `TyTemplate::substitute`
        /// against the frame's `type_args`; the template-space replacement for a
        /// name-based `TypeVar`.
        #[axis(frame)]
        TypeArgRef(u32) = 34,
        // reserved = 35 (the removed `TypeArgRefOrWildcard` dispatch-guard ref)
        // reserved = 36 (the removed `Wildcard` match-any hole)
    }
}

#[cfg(test)]
mod tests {
    use borsh::BorshDeserialize;

    use crate::{
        CodegenTy, ConcreteRealizedTy, ConcreteTy, FunctionParamTy, MediaKind, Name, NotCodegenTy,
        NotRealizedTy, NotRuntimeTy, RealizedTy, RuntimeTy, Ty, TyAttr, TyTemplate, TypeName,
    };

    fn a() -> TyAttr {
        TyAttr::default()
    }

    fn qtn(s: &str) -> TypeName {
        TypeName::local(Name::new(s))
    }

    /// A deeply-nested type that is realized (no type variables) and has a
    /// concrete top-level variant, so it is representable in every member.
    /// Exercises `Map`/`List`/`Union`, the `Function` satellite +
    /// `Vec<Option<_>>` bound, and the `Interface` `Vec<(Name, _)>` binding.
    fn deep_concrete() -> Ty {
        Ty::Map {
            key: Box::new(Ty::String { attr: a() }),
            value: Box::new(Ty::Function {
                params: vec![
                    FunctionParamTy::required(Some(Name::new("x")), Ty::Int { attr: a() }),
                    FunctionParamTy::optional(
                        Some(Name::new("y")),
                        Ty::List(Box::new(Ty::Bool { attr: a() }), a()),
                    ),
                ],
                ret: Box::new(Ty::Interface(
                    qtn("Iterator"),
                    vec![Ty::Union(
                        vec![Ty::Int { attr: a() }, Ty::Null { attr: a() }],
                        a(),
                    )],
                    vec![(Name::new("Item"), Ty::String { attr: a() })],
                    a(),
                )),
                throws: Box::new(Ty::Void { attr: a() }),
                attr: a(),
            }),
            attr: a(),
        }
    }

    /// A stand-in for the interned/heap-anchored head the runtime will carry:
    /// a `Copy` id, laid out nothing like the default [`TypeName`].
    type Interned = Ty<u32>;

    /// The family is genuinely parameterized, not `TypeName` with a parameter
    /// bolted on. Worth its own test because the guarantees the generated code
    /// rests on are *per monomorphization*: each conversion carries a `const`
    /// size + align assert that is only evaluated at the instantiations actually
    /// used, so a family that silently only worked at `TypeName` would look
    /// perfectly healthy until the runtime introduced its own head. Exercising a
    /// second `N` end-to-end — every conversion shape, in both directions — is
    /// what makes that failure a compile error here rather than there.
    #[test]
    fn conversions_hold_at_a_non_default_head() {
        let t: Interned = Ty::Map {
            key: Box::new(Ty::Class(7, vec![Ty::Int { attr: a() }], a())),
            value: Box::new(Ty::Function {
                params: vec![FunctionParamTy::required(
                    Some(Name::new("x")),
                    Ty::Enum(9, a()),
                )],
                ret: Box::new(Ty::Interface(
                    11,
                    vec![Ty::Bool { attr: a() }],
                    vec![(Name::new("Item"), Ty::String { attr: a() })],
                    a(),
                )),
                throws: Box::new(Ty::Void { attr: a() }),
                attr: a(),
            }),
            attr: a(),
        };

        // Deep, equal-size pairs: the reinterpreting conversions and the
        // borrowed upcast.
        let rt = RuntimeTy::<u32>::try_from(&t).unwrap();
        let rz = RealizedTy::<u32>::try_from(&t).unwrap();
        assert_eq!(Ty::from(&rt), t);
        assert_eq!(Ty::from(rt.clone()), t);
        assert_eq!(rt.as_ty(), &t);
        assert_eq!(rz.as_runtime_ty(), &rt);
        assert_eq!(<&RealizedTy<u32>>::try_from(&t), Ok(&rz));

        // A shallow member, whose conversions take the structural walk instead.
        let ct = ConcreteTy::<u32>::try_from(&rt).unwrap();
        assert_eq!(RuntimeTy::from(&ct), rt);
        assert_eq!(Ty::from(&ct), t);

        // Narrowing still rejects by name at a nested depth.
        let bad: Interned = Ty::List(Box::new(Ty::Unknown { attr: a() }), a());
        assert_eq!(
            RuntimeTy::<u32>::try_from(&bad),
            Err(NotRuntimeTy { variant: "Unknown" })
        );

        // And the wire format round-trips over the substituted head.
        assert_eq!(
            Ty::<u32>::try_from_slice(&borsh::to_vec(&t).unwrap()).unwrap(),
            t
        );
    }

    /// Every head is reachable through `visit_heads`, at every nesting depth and
    /// through every shape that can carry one.
    ///
    /// This is the property a relocating collector rests on: a head the walk
    /// misses is a pointer that never gets forwarded, i.e. a dangling reference
    /// after the next move. The walk is generated from the same variant list as
    /// the enum, so it cannot drift — this test pins that the *shapes* are all
    /// descended into (behind `Box`, through `Vec`, through the recursive field
    /// of a satellite, and through the `Vec<(Name, _)>` associated-type binding,
    /// whose head sits in a tuple element).
    #[test]
    fn visit_heads_reaches_every_head() {
        let t: Interned = Ty::Map {
            // Behind a `Box`, with a head nested inside its generic arguments.
            key: Box::new(Ty::Class(1, vec![Ty::Enum(2, a())], a())),
            value: Box::new(Ty::Function {
                // Through a satellite's recursive field.
                params: vec![FunctionParamTy::required(
                    Some(Name::new("x")),
                    Ty::EnumVariant(3, Name::new("V"), a()),
                )],
                ret: Box::new(Ty::Interface(
                    4,
                    // Through a `Vec` of nested types...
                    vec![Ty::TypeAlias(5, a())],
                    // ...and through the tuple element of a binding list.
                    vec![(Name::new("Item"), Ty::Class(6, vec![], a()))],
                    a(),
                )),
                throws: Box::new(Ty::Void { attr: a() }),
                attr: a(),
            }),
            attr: a(),
        };

        let mut seen = Vec::new();
        t.visit_heads(&mut |head| seen.push(*head));
        assert_eq!(seen, vec![1, 2, 3, 4, 5, 6]);

        // The unique-borrow walk reaches exactly the same positions, and writes
        // through them — the forwarding step a moving collector performs.
        let mut forwarded = t.clone();
        forwarded.visit_heads_mut(&mut |head| *head += 100);
        let mut seen_after = Vec::new();
        forwarded.visit_heads(&mut |head| seen_after.push(*head));
        assert_eq!(seen_after, vec![101, 102, 103, 104, 105, 106]);

        // A head-free type yields nothing rather than being skipped entirely.
        let leaf: Interned = Ty::List(Box::new(Ty::Int { attr: a() }), a());
        let mut none = Vec::new();
        leaf.visit_heads(&mut |head| none.push(*head));
        assert!(none.is_empty());
    }

    /// A type variable nested inside a concrete container: representable in
    /// `RuntimeTy` but not `RealizedTy`.
    fn with_typevar() -> Ty {
        Ty::List(Box::new(Ty::type_var("T")), a())
    }

    /// Widening (`From`) reaches every member above `deep_concrete()`, by ref
    /// and by owned move, and narrowing inverts it.
    #[test]
    fn conversion_matrix_round_trips() {
        let t = deep_concrete();
        let rt = RuntimeTy::try_from(&t).unwrap();
        let cg = CodegenTy::try_from(&t).unwrap();
        let rz = RealizedTy::try_from(&t).unwrap();
        let ct = ConcreteTy::try_from(&rt).unwrap();
        let crz = ConcreteRealizedTy::try_from(&rz).unwrap();

        // RealizedTy ≤ RuntimeTy ≤ Ty (deep), by ref + owned move.
        assert_eq!(Ty::from(&rt), t);
        assert_eq!(Ty::from(rt.clone()), t);
        assert_eq!(RuntimeTy::from(&cg), rt);
        assert_eq!(RuntimeTy::from(cg.clone()), rt);
        assert_eq!(CodegenTy::try_from(&rt).unwrap(), cg);
        assert_eq!(Ty::from(&rz), t);
        assert_eq!(RuntimeTy::from(&rz), rt);
        assert_eq!(RuntimeTy::from(rz.clone()), rt);

        // ConcreteTy ≤ RuntimeTy ≤ Ty (ConcreteTy→RuntimeTy is shallow: same child).
        assert_eq!(RuntimeTy::from(&ct), rt);
        assert_eq!(RuntimeTy::from(ct.clone()), rt);
        assert_eq!(Ty::from(&ct), t);

        // ConcreteRealizedTy ≤ {ConcreteTy, RealizedTy, RuntimeTy, Ty}.
        assert_eq!(RealizedTy::from(&crz), rz);
        assert_eq!(ConcreteTy::from(&crz), ct);
        assert_eq!(ConcreteTy::from(crz.clone()), ct);
        assert_eq!(RuntimeTy::from(&crz), rt);
        assert_eq!(Ty::from(&crz), t);

        // Narrowing inverts widening for the remaining edges.
        assert_eq!(ConcreteTy::try_from(&t).unwrap(), ct);
        assert_eq!(ConcreteRealizedTy::try_from(&rt).unwrap(), crz);
        assert_eq!(ConcreteRealizedTy::try_from(&ct).unwrap(), crz);
        assert_eq!(ConcreteRealizedTy::try_from(&t).unwrap(), crz);
    }

    /// `RealizedTy` deeply rejects type variables (by name); `RuntimeTy` keeps
    /// them.
    #[test]
    fn realized_rejects_type_variables() {
        let t = with_typevar();
        assert!(RuntimeTy::try_from(&t).is_ok());
        assert_eq!(
            RealizedTy::try_from(&t),
            Err(NotRealizedTy { variant: "TypeVar" })
        );
    }

    /// Codegen types retain generic parameters but require associated
    /// projections to have been resolved at the compiler boundary.
    #[test]
    fn codegen_accepts_type_variables_and_rejects_projections() {
        let typevar = with_typevar();
        let runtime = RuntimeTy::try_from(&typevar).unwrap();
        let codegen = CodegenTy::try_from(&runtime).unwrap();
        assert_eq!(RuntimeTy::from(&codegen), runtime);

        let projection = Ty::AssociatedTypeProjection {
            base: Box::new(Ty::type_var("T")),
            interface: Box::new(crate::Interface::new(
                qtn("Iterator"),
                Vec::new(),
                Vec::new(),
            )),
            member: Name::new("Item"),
            attr: a(),
        };
        assert_eq!(
            CodegenTy::try_from(&projection),
            Err(NotCodegenTy {
                variant: "AssociatedTypeProjection"
            })
        );
    }

    /// Borsh serialization round-trips for every member.
    #[test]
    fn borsh_round_trips() {
        let t = deep_concrete();
        let rt = RuntimeTy::try_from(&t).unwrap();
        let cg = CodegenTy::try_from(&t).unwrap();
        let rz = RealizedTy::try_from(&t).unwrap();
        let ct = ConcreteTy::try_from(&rt).unwrap();
        let crz = ConcreteRealizedTy::try_from(&rz).unwrap();

        assert_eq!(Ty::try_from_slice(&borsh::to_vec(&t).unwrap()).unwrap(), t);
        assert_eq!(
            RuntimeTy::try_from_slice(&borsh::to_vec(&rt).unwrap()).unwrap(),
            rt
        );
        assert_eq!(
            CodegenTy::try_from_slice(&borsh::to_vec(&cg).unwrap()).unwrap(),
            cg
        );
        assert_eq!(
            RealizedTy::try_from_slice(&borsh::to_vec(&rz).unwrap()).unwrap(),
            rz
        );
        assert_eq!(
            ConcreteTy::try_from_slice(&borsh::to_vec(&ct).unwrap()).unwrap(),
            ct
        );
        assert_eq!(
            ConcreteRealizedTy::try_from_slice(&borsh::to_vec(&crz).unwrap()).unwrap(),
            crz
        );
    }

    /// Serialize at an explicitly named member. Variant construction alone
    /// leaves the head parameter `N` open, and these assertions are about one
    /// member at the default head — so each names the member it locks.
    fn tag<T: borsh::BorshSerialize>(value: T) -> u8 {
        borsh::to_vec(&value).unwrap()[0]
    }

    /// Lock the Borsh wire format. Every family member uses the explicit master
    /// discriminants, with slot 23 reserved for the removed `WatchAccessor`.
    #[test]
    fn borsh_uses_explicit_discriminants() {
        assert_eq!(tag::<Ty>(Ty::Int { attr: a() }), 0);
        assert_eq!(tag::<Ty>(Ty::Media(MediaKind::Image, a())), 7);
        assert_eq!(
            tag::<Ty>(Ty::List(Box::new(Ty::Bool { attr: a() }), a())),
            13
        );
        assert_eq!(
            tag::<Ty>(Ty::EvolvingMap(
                Box::new(Ty::Never { attr: a() }),
                Box::new(Ty::Never { attr: a() }),
                a()
            )),
            32
        );
        assert_eq!(tag::<Ty>(Ty::Infer { attr: a() }), 33);
        assert_eq!(
            tag::<RuntimeTy>(RuntimeTy::TypeAlias(qtn("Alias"), a())),
            24
        );
        // Filtered family members use the same master tags rather than local
        // declaration-order indices.
        assert_eq!(
            tag::<RealizedTy>(RealizedTy::BuiltinUnknown { attr: a() }),
            27
        );
        assert_eq!(
            tag::<TyTemplate>(TyTemplate::TypeAlias(qtn("Alias"), a())),
            24
        );
        assert_eq!(tag::<TyTemplate>(TyTemplate::TypeArgRef(0)), 34);
        assert_eq!(tag::<ConcreteTy>(ConcreteTy::Never { attr: a() }), 28);
    }

    /// The leading byte of a `#[repr(C, u8)]` value is its discriminant. Reading
    /// it directly lets us assert a logical variant carries the same *in-memory*
    /// tag in every member — the premise the zero-cost `transmute` upcasts rest
    /// on. Borsh uses these same explicit discriminants for its wire tags.
    fn in_memory_tag<T>(v: &T) -> u8 {
        // SAFETY: every family member is `#[repr(C, u8)]`, so its first byte is
        // the `u8` discriminant.
        unsafe { *(v as *const T as *const u8) }
    }

    #[test]
    fn in_memory_discriminants_are_consistent_across_members() {
        // `BuiltinUnknown` is master variant #27; `RealizedTy` drops the
        // `typevar` and `projection` variants before it, yet its tag stays 27.
        assert_eq!(in_memory_tag::<Ty>(&Ty::BuiltinUnknown { attr: a() }), 27);
        assert_eq!(
            in_memory_tag::<RuntimeTy>(&RuntimeTy::BuiltinUnknown { attr: a() }),
            27
        );
        assert_eq!(
            in_memory_tag::<CodegenTy>(&CodegenTy::BuiltinUnknown { attr: a() }),
            27
        );
        assert_eq!(
            in_memory_tag::<RealizedTy>(&RealizedTy::BuiltinUnknown { attr: a() }),
            27
        );
        // `Never` (#28) is shared and tag-stable across the deep members.
        assert_eq!(in_memory_tag::<Ty>(&Ty::Never { attr: a() }), 28);
        assert_eq!(
            in_memory_tag::<CodegenTy>(&CodegenTy::Never { attr: a() }),
            28
        );
        assert_eq!(
            in_memory_tag::<RealizedTy>(&RealizedTy::Never { attr: a() }),
            28
        );
        // A leaf concrete variant present in every member, shallow ones included.
        assert_eq!(in_memory_tag::<Ty>(&Ty::Int { attr: a() }), 0);
        assert_eq!(in_memory_tag::<CodegenTy>(&CodegenTy::Int { attr: a() }), 0);
        assert_eq!(
            in_memory_tag::<ConcreteTy>(&ConcreteTy::Int { attr: a() }),
            0
        );
        assert_eq!(
            in_memory_tag::<ConcreteRealizedTy>(&ConcreteRealizedTy::Int { attr: a() }),
            0
        );
        // The template-only frame leaf keeps its master tag.
        assert_eq!(in_memory_tag::<TyTemplate>(&TyTemplate::TypeArgRef(0)), 34);
    }

    /// The borrowed upcast (`RuntimeTy::as_ty`, `RealizedTy::as_runtime_ty`, …)
    /// reinterprets in place and yields a `&Super` structurally equal to the
    /// owned widening — proving the `transmute` produces the right *value*, not
    /// merely a same-sized one, at every nesting depth of `deep_concrete()`.
    #[test]
    fn borrowed_upcast_matches_owned_widening() {
        let t = deep_concrete();
        let rt = RuntimeTy::try_from(&t).unwrap();
        let cg = CodegenTy::try_from(&t).unwrap();
        let rz = RealizedTy::try_from(&t).unwrap();

        // Reinterpreting the narrower value yields the wider value by reference.
        assert_eq!(rt.as_ty(), &t);
        assert_eq!(cg.as_ty(), &t);
        assert_eq!(cg.as_runtime_ty(), &rt);
        assert_eq!(rz.as_ty(), &t);
        assert_eq!(rz.as_codegen_ty(), &cg);
        assert_eq!(rz.as_runtime_ty(), &rt);

        // And it agrees with the owned `From` (also a transmute) on equal input.
        assert_eq!(rt.as_ty(), &Ty::from(rt.clone()));
        assert_eq!(cg.as_runtime_ty(), &RuntimeTy::from(cg.clone()));
        assert_eq!(rz.as_runtime_ty(), &RuntimeTy::from(rz.clone()));
    }

    /// Narrowing a representable value: the borrow-to-borrow `TryFrom<&Super>
    /// for &Sub` validates in place and reinterprets the borrow without copying;
    /// the owned `TryFrom` validates then moves the tree. Both agree with the
    /// narrower value at every depth.
    #[test]
    fn downcast_validates_then_reinterprets() {
        let t = deep_concrete();
        let rt = RuntimeTy::try_from(&t).unwrap();
        let cg = CodegenTy::try_from(&t).unwrap();
        let rz = RealizedTy::try_from(&t).unwrap();

        // Borrow-to-borrow narrowing yields `Ok(&narrower)` at every depth.
        assert_eq!(<&RuntimeTy>::try_from(&t), Ok(&rt));
        assert_eq!(<&CodegenTy>::try_from(&t), Ok(&cg));
        assert_eq!(<&CodegenTy>::try_from(&rt), Ok(&cg));
        assert_eq!(<&RealizedTy>::try_from(&t), Ok(&rz));
        assert_eq!(<&RealizedTy>::try_from(&rt), Ok(&rz));

        // Owned `TryFrom` (validate + move-transmute, no rebuild) agrees.
        assert_eq!(RuntimeTy::try_from(t.clone()).unwrap(), rt);
        assert_eq!(CodegenTy::try_from(t.clone()).unwrap(), cg);
        assert_eq!(CodegenTy::try_from(rt.clone()).unwrap(), cg);
        assert_eq!(RealizedTy::try_from(t.clone()).unwrap(), rz);
        assert_eq!(RealizedTy::try_from(rt.clone()).unwrap(), rz);
    }

    /// The validation walk is complete: an unrepresentable variant nested at any
    /// depth makes the narrowing fail — with the offending variant named, since
    /// `TryFrom` carries an error — rather than transmute into an invalid
    /// discriminant.
    #[test]
    fn downcast_rejects_unrepresentable_at_depth() {
        // `TypeVar` under a `List`: a valid `RuntimeTy` (keeps `typevar`), not a
        // valid `RealizedTy` (drops it) — the error names the culprit.
        let tv = with_typevar();
        let tv_rt = <&RuntimeTy>::try_from(&tv).unwrap();
        assert_eq!(
            <&RealizedTy>::try_from(&tv),
            Err(NotRealizedTy { variant: "TypeVar" })
        );
        assert_eq!(
            <&RealizedTy>::try_from(tv_rt),
            Err(NotRealizedTy { variant: "TypeVar" })
        );

        // A `tir`-only `Unknown` buried in a map value: not even a `RuntimeTy`.
        let bad = Ty::Map {
            key: Box::new(Ty::String { attr: a() }),
            value: Box::new(Ty::List(Box::new(Ty::Unknown { attr: a() }), a())),
            attr: a(),
        };
        assert_eq!(
            <&RuntimeTy>::try_from(&bad),
            Err(NotRuntimeTy { variant: "Unknown" })
        );
        assert_eq!(
            <&CodegenTy>::try_from(&bad),
            Err(NotCodegenTy { variant: "Unknown" })
        );
        assert_eq!(
            <&RealizedTy>::try_from(&bad),
            Err(NotRealizedTy { variant: "Unknown" })
        );
        assert_eq!(
            RuntimeTy::try_from(bad),
            Err(NotRuntimeTy { variant: "Unknown" })
        );
    }
}

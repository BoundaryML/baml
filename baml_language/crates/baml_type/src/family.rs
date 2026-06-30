//! The `Ty` type family, generated from a single tagged definition.
//!
//! [`ty_family!`](baml_type_macros::ty_family) expands the master `Ty` enum
//! below into the whole family — `Ty`, [`RuntimeTy`], [`RealizedTy`],
//! [`ConcreteTy`], [`ConcreteRealizedTy`] — plus the per-member
//! `FunctionParamTy` companion structs. Membership is by axis: each variant is
//! tagged with exactly one `#[axis(..)]`, and each member includes a set of
//! axes (a variant is present iff its axis is included). Nested positions are
//! retargeted per member: deep members (`child: Self`) recurse into themselves;
//! the shallow `Concrete*` members nest their declared `child`.
//!
//! The semantic impls (`render_with`, `is_subtype_of`, `Display`,
//! `validate_runtime`, the conversions, and the `lower_to_runtime` boundary)
//! stay hand-written in `lib.rs`, `runtime_ty.rs`, and `realized_ty.rs`.

use baml_type_macros::ty_family;
use borsh::{BorshDeserialize, BorshSerialize};

use crate::{Freshness, FunctionParamMode, Literal, MediaKind, Name, TyAttr, TypeName};

ty_family! {
    axes { concrete, abstract, literal, never, typevar, tir, special }

    type Ty                 { includes: [concrete, abstract, literal, never, typevar, tir, special], child: Self }
    type RuntimeTy          { includes: [concrete, abstract, literal, never, typevar, special],      child: Self }
    type RealizedTy         { includes: [concrete, abstract, literal, never, special],               child: Self }
    type ConcreteTy         { includes: [concrete, never],                                           child: RuntimeTy }
    type ConcreteRealizedTy { includes: [concrete, never],                                           child: RealizedTy }

    satellite FunctionParamTy {
        pub name: Option<Name>,
        pub ty: Ty,
        pub mode: FunctionParamMode,
    } methods {
        pub fn required(name: Option<Name>, ty: Ty) -> Self {
            Self {
                name,
                ty,
                mode: FunctionParamMode::Required,
            }
        }

        pub fn optional(name: Option<Name>, ty: Ty) -> Self {
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
    satellite Interface {
        pub name: TypeName,
        /// The interface's generic *input* arguments, in declaration order.
        /// Resolved from context; never defaulted away — they are part of the
        /// interface's identity (`Converter<int>` ≠ `Converter<float>`).
        pub generics: Vec<Ty>,
        /// Associated-type bindings that further constrain the implementor (the
        /// `Item = int` in `Iterator<Item = int>`). Real constraints carried as
        /// part of the interface, never stripped; sorted by name for a
        /// deterministic order.
        pub associated_types: Vec<(Name, Ty)>,
    } methods {
        /// Build an interface constraint, sorting `associated_types` by name so the
        /// invariant the field documents holds and the derived `Eq`/`Hash`/`Ord`
        /// are order-insensitive. Normalization sorts bindings identically, so a
        /// constraint built here compares equal to its normalized form. Generated
        /// for every family member (`Interface`, `RuntimeInterface`, …), so every
        /// construction site — including the untrusted ctypes decode boundary —
        /// can route through it instead of sorting by hand.
        pub fn new(name: TypeName, generics: Vec<Ty>, mut associated_types: Vec<(Name, Ty)>) -> Self {
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
    pub enum Ty {
        #[axis(concrete)]
        Int {
            attr: TyAttr,
        },
        #[axis(concrete)]
        Bigint {
            attr: TyAttr,
        },
        #[axis(concrete)]
        Float {
            attr: TyAttr,
        },
        #[axis(concrete)]
        String {
            attr: TyAttr,
        },
        #[axis(concrete)]
        Bool {
            attr: TyAttr,
        },
        #[axis(concrete)]
        Null {
            attr: TyAttr,
        },
        #[axis(concrete)]
        Uint8Array {
            attr: TyAttr,
        },
        #[axis(concrete)]
        Media(MediaKind, TyAttr),
        /// A literal type — a single value (`1`, `"hi"`, `true`) as a type. The
        /// [`Freshness`] flag is compiler-only (fresh literals widen at mutable
        /// binding sites); it is normalized to `Regular` at the runtime boundary.
        #[axis(literal)]
        Literal(Literal, Freshness, TyAttr),
        #[axis(concrete)]
        Class(TypeName, Vec<Ty>, TyAttr),
        /// An interface existential type, equivalent to Rust `dyn Trait`.
        /// Must specify all generic type args and all associated types.
        #[axis(abstract)]
        Interface(TypeName, Vec<Ty>, Vec<(Name, Ty)>, TyAttr),
        #[axis(concrete)]
        Enum(TypeName, TyAttr),
        /// A specific enum variant — `Status.HttpError`.
        #[axis(literal)]
        EnumVariant(TypeName, Name, TyAttr),
        #[axis(concrete)]
        List(Box<Ty>, TyAttr),
        #[axis(concrete)]
        Map {
            key: Box<Ty>,
            value: Box<Ty>,
            attr: TyAttr,
        },
        #[axis(abstract)]
        Union(Vec<Ty>, TyAttr),

        /// Function/arrow type: `(T1, T2, ...) -> R throws E`.
        #[axis(concrete)]
        Function {
            params: Vec<FunctionParamTy>,
            ret: Box<Ty>,
            throws: Box<Ty>,
            attr: TyAttr,
        },
        /// A future handle — the result of `schedule_future` or `spawn`
        /// before `await`.
        ///
        /// Carries both the value type the future resolves to and the error
        /// type the future may throw. The error type approximates `never` as
        /// `Null` when the body of the future statically cannot throw.
        #[axis(concrete)]
        Future(Box<Ty>, Box<Ty>, TyAttr),
        /// Opaque Rust-managed state (`$rust_type` fields in builtin class stubs,
        /// e.g. `Media._data`). A leaf concrete type with no inner structure.
        ///
        /// Renders as `$rust_type` (qualified name `baml.rust.RustType`).
        #[axis(concrete)]
        RustType {
            attr: TyAttr,
        },
        /// The `type` metatype keyword — a runtime value that wraps a `Ty`
        /// (reflection). A leaf concrete type.
        ///
        /// Renders as the `type` keyword (qualified name `baml.reflect.Type`).
        #[axis(concrete)]
        Type {
            attr: TyAttr,
        },
        /// Opaque resource handle — file, socket, or HTTP response body. A leaf
        /// concrete type whose *values* are concrete Rust types on the VM heap; the
        /// type system treats it nominally (no structural decomposition).
        ///
        /// Renders as its qualified name `baml.llm.Resource`.
        #[axis(concrete)]
        Resource {
            attr: TyAttr,
        },
        /// Opaque structured prompt tree for LLM calls. A leaf concrete type whose
        /// *values* are concrete Rust types on the VM heap; the type system treats
        /// it nominally (no structural decomposition).
        ///
        /// Renders as its qualified name `baml.llm.PromptAst`.
        #[axis(concrete)]
        PromptAst {
            attr: TyAttr,
        },

        /// Void type — the type of effectful expressions (was VIR `Unit`).
        #[axis(special)]
        Void {
            attr: TyAttr,
        },
        /// Watch accessor type: represents `x.$watch` on a watched variable.
        #[axis(special)]
        WatchAccessor(Box<Ty>, TyAttr),

        /// Only recursive aliases survive lower_ty; non-recursive are expanded.
        #[axis(special)]
        TypeAlias(TypeName, TyAttr),
        /// A type variable (generic parameter) — e.g. `T` in `Array<T>`. Bound
        /// during inference; can survive at runtime only inside reflective generic
        /// metadata.
        #[axis(typevar)]
        TypeVar(Name, TyAttr),
        /// Associated type projection, e.g. `P.Output` or `(T as Iterator).Item`. Bound
        /// during inference; can survive at runtime only inside reflective generic
        /// metadata.
        #[axis(typevar)]
        AssociatedTypeProjection {
            base: Box<Ty>,
            /// TODO: Should not be an `Option`; fix once the TIR is able to determine correctly.
            interface: Option<Box<Interface>>,
            member: Name,
            attr: TyAttr,
        },
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
        },
        /// The bottom type — an expression that never produces a value (`return`,
        /// `break`, `continue`, diverging blocks). A subtype of every type.
        #[axis(never)]
        Never {
            attr: TyAttr,
        },

        // --- TIR-only: present during type checking, erased at the runtime
        // boundary (`lower_to_runtime`). Carried only by `Ty` (the `tir` axis).
        /// Error-recovery sentinel: the type is structurally unknown (e.g. an
        /// unresolved name). Distinct from `BuiltinUnknown` (a well-formed top type).
        #[axis(tir)]
        Unknown {
            attr: TyAttr,
        },
        /// Error sentinel: a hard type error was emitted for this expression.
        #[axis(tir)]
        Error {
            attr: TyAttr,
        },
        /// Evolving list — an empty `[]` literal at a mutable binding whose element
        /// type is refined by mutations. Frozen to `List` at the runtime boundary.
        #[axis(tir)]
        EvolvingList(Box<Ty>, TyAttr),
        /// Evolving map — the map analogue of [`Ty::EvolvingList`].
        #[axis(tir)]
        EvolvingMap(Box<Ty>, Box<Ty>, TyAttr),
        /// Inference hole — the wildcard `_` written in a type-argument or
        /// `throws`-clause position. A leaf placeholder that asks the checker to
        /// infer the type at this slot from surrounding context (the initializer
        /// of a `let`, or the inferred effective throw set). Filled during TIR
        /// checking; like the other `tir`-axis sentinels it must never survive to
        /// the runtime boundary (`lower_to_runtime` rejects it).
        #[axis(tir)]
        Infer {
            attr: TyAttr,
        },
    }
}

#[cfg(test)]
mod tests {
    use borsh::BorshDeserialize;

    use crate::{
        ConcreteRealizedTy, ConcreteTy, FunctionParamTy, MediaKind, Name, NotRealizedTy,
        RealizedTy, RuntimeTy, Ty, TyAttr, TypeName,
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

    /// A type variable nested inside a concrete container: representable in
    /// `RuntimeTy` but not `RealizedTy`.
    fn with_typevar() -> Ty {
        Ty::List(Box::new(Ty::TypeVar(Name::new("T"), a())), a())
    }

    /// Widening (`From`) reaches every member above `deep_concrete()`, by ref
    /// and by owned move, and narrowing inverts it.
    #[test]
    fn conversion_matrix_round_trips() {
        let t = deep_concrete();
        let rt = RuntimeTy::try_from(&t).unwrap();
        let rz = RealizedTy::try_from(&t).unwrap();
        let ct = ConcreteTy::try_from(&rt).unwrap();
        let crz = ConcreteRealizedTy::try_from(&rz).unwrap();

        // RealizedTy ≤ RuntimeTy ≤ Ty (deep), by ref + owned move.
        assert_eq!(Ty::from(&rt), t);
        assert_eq!(Ty::from(rt.clone()), t);
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

    /// Borsh serialization round-trips for every member.
    #[test]
    fn borsh_round_trips() {
        let t = deep_concrete();
        let rt = RuntimeTy::try_from(&t).unwrap();
        let rz = RealizedTy::try_from(&t).unwrap();
        let ct = ConcreteTy::try_from(&rt).unwrap();
        let crz = ConcreteRealizedTy::try_from(&rz).unwrap();

        assert_eq!(Ty::try_from_slice(&borsh::to_vec(&t).unwrap()).unwrap(), t);
        assert_eq!(
            RuntimeTy::try_from_slice(&borsh::to_vec(&rt).unwrap()).unwrap(),
            rt
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

    /// Lock the Borsh wire format: variants are tagged by declaration-order
    /// index (a leading `u8`). A reorder of the master enum — or a member that
    /// re-indexes its variants — would change these and break persisted bytes.
    #[test]
    fn borsh_discriminants_are_declaration_order() {
        let tag = |bytes: Vec<u8>| bytes[0];
        assert_eq!(tag(borsh::to_vec(&Ty::Int { attr: a() }).unwrap()), 0);
        assert_eq!(
            tag(borsh::to_vec(&Ty::Media(MediaKind::Image, a())).unwrap()),
            7
        );
        assert_eq!(
            tag(borsh::to_vec(&Ty::List(Box::new(Ty::Bool { attr: a() }), a())).unwrap()),
            13
        );
        assert_eq!(
            tag(borsh::to_vec(&Ty::EvolvingMap(
                Box::new(Ty::Never { attr: a() }),
                Box::new(Ty::Never { attr: a() }),
                a()
            ))
            .unwrap()),
            32
        );
        // `Infer` is the last (34th) `Ty` variant.
        assert_eq!(tag(borsh::to_vec(&Ty::Infer { attr: a() }).unwrap()), 33);
        // `RuntimeTy` keeps `Ty`'s order through the non-`tir` variants.
        assert_eq!(
            tag(borsh::to_vec(&RuntimeTy::Int { attr: a() }).unwrap()),
            0
        );
        // `RealizedTy` drops the two `typevar` variants, so its tail shifts up:
        // `BuiltinUnknown` is 25 here (vs 27 in `Ty`).
        assert_eq!(
            tag(borsh::to_vec(&RealizedTy::BuiltinUnknown { attr: a() }).unwrap()),
            25
        );
    }
}

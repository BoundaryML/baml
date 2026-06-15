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

        /// Function/arrow type: `<G…>(T1, T2, ...) -> R throws E`.
        ///
        /// `generic_params`/`generic_param_bounds` carry the function's declared
        /// type parameters and their bounds (kept at runtime for reflection, even
        /// though body `TypeVar`s are erased at the runtime boundary).
        #[axis(concrete)]
        Function {
            generic_params: Vec<Name>,
            generic_param_bounds: Vec<Option<Ty>>,
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
            interface: Option<Box<Ty>>,
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
    }
}

//! Unified type system for BAML.
//!
//! `baml_type::Ty` is the canonical type representation used from VIR through runtime.
//! TIR keeps its own `Ty` with `QualifiedName` and `TypeAlias` — this crate
//! provides the single conversion point from TIR types.

use std::fmt;

// Re-export core baml_base types so downstream crates can depend on baml_type
// instead of baml_base directly.
pub use baml_base::{Literal, MediaKind, Name, Span};
use borsh::{BorshDeserialize, BorshSerialize};

mod attr;
mod defs;
mod names;
mod primitive;
pub mod simplify_sap;
pub mod template;
pub mod typetag;
pub use attr::*;
pub use defs::*;
pub use names::*;
pub use primitive::*;
use subenum::subenum;
pub use template::TyTemplate;

/// Upper bound on the bit-length of a `bigint` value we are willing to
/// materialize at runtime. ~268 million bits ≈ 80 million decimal digits ≈ 32
/// MiB of digits. Operations that would produce a larger result raise
/// `baml.panics.AllocFailure` instead of either succeeding (and starving the
/// rest of the runtime) or aborting the process outright.
///
/// Shared by the VM's allocation guard (`bex_vm::package_baml::bigint`),
/// the FFI decoder's pre-allocation cap
/// (`bridge_ctypes::value_decode::MAX_BIGINT_HEX_LEN`), and TIR's
/// constant-folding refusal threshold.
pub const MAX_BIGINT_BITS: u64 = 1 << 28;

/// Permissive upper bound on the number of base-ten digits a `bigint` may
/// have before it cannot possibly fit in [`MAX_BIGINT_BITS`].
///
/// Each base-ten digit carries `log2(10) ≈ 3.32` bits, so any decimal string
/// longer than `MAX_BIGINT_BITS / 3 + 2` is guaranteed to overflow the cap.
/// Used as a cheap pre-flight reject before `BigInt::parse_bytes`; callers
/// follow up with an exact `bits()` check for borderline inputs. Shared by
/// SAP deserialization, the jsonish number visitor, and `bigint.parse`.
#[allow(clippy::cast_possible_truncation)] // MAX_BIGINT_BITS is 2^28; fits in usize on 32/64-bit
pub const MAX_BIGINT_DECIMAL_DIGITS: usize = (MAX_BIGINT_BITS / 3 + 2) as usize;

/// Transitional alias for [`QualifiedTypeName`], the single qualified-name type
/// for class/enum/type-alias references. The legacy public fields
/// (`name`/`module_path`/`display_name`) are now methods: [`QualifiedTypeName::name`],
/// [`QualifiedTypeName::module_path`], [`QualifiedTypeName::display_name`].
pub use crate::QualifiedTypeName as TypeName;

/// Freshness flag for literal types.
///
/// Modeled after TypeScript's fresh/regular literal type distinction.
/// - **Fresh**: produced by literal expressions (`1`, `"hello"`). Widens to
///   the base primitive at mutable binding sites (`let x = 1` → `int`).
/// - **Regular**: produced by type annotations (`let x: 1 = 1`) or contextual
///   typing. Preserved through mutable bindings.
///
/// Freshness is **ignored** by the subtype checker — `Literal(1, Fresh)` and
/// `Literal(1, Regular)` are structurally identical for assignability.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, BorshSerialize, BorshDeserialize,
)]
pub enum Freshness {
    Fresh,
    Regular,
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
#[subenum(ConcreteTy, RuntimeTy)]
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, BorshSerialize, BorshDeserialize)]
pub enum Ty {
    // --- Core: used by all VIR+ stages ---
    #[subenum(ConcreteTy, RuntimeTy)]
    Int { attr: TyAttr },
    #[subenum(ConcreteTy, RuntimeTy)]
    Bigint { attr: TyAttr },
    #[subenum(ConcreteTy, RuntimeTy)]
    Float { attr: TyAttr },
    #[subenum(ConcreteTy, RuntimeTy)]
    String { attr: TyAttr },
    #[subenum(ConcreteTy, RuntimeTy)]
    Bool { attr: TyAttr },
    #[subenum(ConcreteTy, RuntimeTy)]
    Null { attr: TyAttr },
    #[subenum(ConcreteTy, RuntimeTy)]
    Uint8Array { attr: TyAttr },
    #[subenum(ConcreteTy, RuntimeTy)]
    Media(MediaKind, TyAttr),
    /// A literal type — a single value (`1`, `"hi"`, `true`) as a type. The
    /// [`Freshness`] flag is compiler-only (fresh literals widen at mutable
    /// binding sites); it is normalized to `Regular` at the runtime boundary.
    #[subenum(RuntimeTy)]
    Literal(Literal, Freshness, TyAttr),
    #[subenum(ConcreteTy, RuntimeTy)]
    Class(TypeName, Vec<Ty>, TyAttr),
    #[subenum(RuntimeTy)]
    Interface(TypeName, Vec<Ty>, Vec<(Name, Ty)>, TyAttr),
    #[subenum(ConcreteTy, RuntimeTy)]
    Enum(TypeName, TyAttr),
    /// A specific enum variant — `Status.HttpError`.
    /// Compiler-only: should not reach runtime.
    #[subenum(RuntimeTy)]
    EnumVariant(TypeName, Name, TyAttr),
    #[subenum(ConcreteTy, RuntimeTy)]
    List(Box<Ty>, TyAttr),
    #[subenum(ConcreteTy, RuntimeTy)]
    Map {
        key: Box<Ty>,
        value: Box<Ty>,
        attr: TyAttr,
    },
    #[subenum(RuntimeTy)]
    Union(Vec<Ty>, TyAttr),

    // --- Runtime-only: present at runtime, not in user-facing type syntax ---
    /// Opaque runtime-only type, identified by its qualified name.
    ///
    /// Used for types that the type system treats generically (nominal equality,
    /// no structural decomposition, infinite for exhaustiveness) but whose
    /// *values* are concrete Rust types on the VM heap.
    ///
    /// Well-known opaque types:
    /// - `baml.llm.Resource` — file/socket/HTTP response handles
    /// - `baml.llm.PromptAst` — structured prompt trees for LLM calls
    /// - `type` — meta-type wrapping a `Ty` for reflection
    ///
    /// Use the convenience constructors `Ty::resource()`, `Ty::prompt_ast()`,
    /// `Ty::type_type()` instead of constructing directly.
    #[subenum(ConcreteTy, RuntimeTy)]
    Opaque(TypeName, TyAttr),

    // --- Compiler-specific: present in VIR/MIR, absent at runtime ---
    /// Only recursive aliases survive lower_ty; non-recursive are expanded.
    TypeAlias(TypeName, TyAttr),
    /// Function/arrow type: `(T1, T2, ...) -> R`
    #[subenum(ConcreteTy, RuntimeTy)]
    Function {
        params: Vec<Ty>,
        ret: Box<Ty>,
        throws: Box<Ty>,
        attr: TyAttr,
    },
    /// Void type — the type of effectful expressions (was VIR `Unit`).
    /// Also used for diverging expressions (return, break, continue) since
    /// MIR encodes divergence via control flow terminators, not the type.
    #[subenum(RuntimeTy)]
    Void { attr: TyAttr },
    /// Watch accessor type: represents `x.$watch` on a watched variable.
    #[subenum(RuntimeTy)]
    WatchAccessor(Box<Ty>, TyAttr),
    /// Internal-only type for builtin functions that accept any argument.
    ///
    /// Similar to TypeScript's `unknown` - any value can be passed where
    /// `BuiltinUnknown` is expected, but `BuiltinUnknown` cannot be used
    /// where a specific type is required.
    ///
    /// Used in llm.baml for functions like:
    /// ```baml
    /// function render_prompt(function_name: string, args: map<string, unknown>) -> PromptAst
    /// ```
    ///
    /// This is a compiler-only variant that should never reach runtime.
    #[subenum(RuntimeTy)]
    BuiltinUnknown { attr: TyAttr },
    /// A future handle — the result of `schedule_future` or `spawn`
    /// before `await`.
    ///
    /// Carries both the value type the future resolves to and the error
    /// type the future may throw. The error type approximates `never` as
    /// `Null` when the body of the future statically cannot throw.
    #[subenum(ConcreteTy, RuntimeTy)]
    Future(Box<Ty>, Box<Ty>, TyAttr),

    // --- TIR-only: present during type checking, erased at the runtime
    // boundary (`convert_tir2_ty`). Excluded from `ConcreteTy`; only the ones
    // that can legitimately nest in a runtime type carry `RuntimeTy`.
    /// A type variable (generic parameter) — e.g. `T` in `Array<T>`. Bound
    /// during inference; can survive at runtime only inside reflective generic
    /// metadata.
    #[subenum(RuntimeTy)]
    TypeVar(Name, TyAttr),
    /// Associated type projection, e.g. `P.Output` or `(T as Iterator).Item`.
    /// Resolved before the runtime boundary.
    AssociatedTypeProjection {
        base: Box<Ty>,
        interface: Option<Box<Ty>>,
        member: Name,
        attr: TyAttr,
    },
    /// The bottom type — an expression that never produces a value (`return`,
    /// `break`, `continue`, diverging blocks). A subtype of every type.
    #[subenum(RuntimeTy)]
    Never { attr: TyAttr },
    /// Error-recovery sentinel: the type is structurally unknown (e.g. an
    /// unresolved name). Distinct from `BuiltinUnknown` (a well-formed top type).
    Unknown { attr: TyAttr },
    /// Error sentinel: a hard type error was emitted for this expression.
    Error { attr: TyAttr },
    /// Evolving list — an empty `[]` literal at a mutable binding whose element
    /// type is refined by mutations. Frozen to `List` at the runtime boundary.
    EvolvingList(Box<Ty>, TyAttr),
    /// Evolving map — the map analogue of [`Ty::EvolvingList`].
    EvolvingMap(Box<Ty>, Box<Ty>, TyAttr),
    /// Opaque Rust-managed state (`$rust_type` fields in builtin class stubs,
    /// e.g. `Media._data`). A leaf concrete type with no inner structure.
    #[subenum(ConcreteTy, RuntimeTy)]
    RustType { attr: TyAttr },
    /// The `type` metatype keyword — a runtime value that wraps a `Ty`
    /// (reflection). A leaf concrete type.
    #[subenum(ConcreteTy, RuntimeTy)]
    Type { attr: TyAttr },
}

impl Ty {
    // --- TyAttr accessor ---

    /// Replace the TyAttr on this type, returning a new Ty with the given attr.
    ///
    /// Used during HIR lowering to apply SAP attributes (sap_in_progress) to the
    /// resolved type of generated stream_* class fields.
    pub fn with_attr(self, attr: TyAttr) -> Ty {
        match self {
            Ty::Int { .. } => Ty::Int { attr },
            Ty::Bigint { .. } => Ty::Bigint { attr },
            Ty::Float { .. } => Ty::Float { attr },
            Ty::String { .. } => Ty::String { attr },
            Ty::Bool { .. } => Ty::Bool { attr },
            Ty::Null { .. } => Ty::Null { attr },
            Ty::Void { .. } => Ty::Void { attr },
            Ty::BuiltinUnknown { .. } => Ty::BuiltinUnknown { attr },
            Ty::Uint8Array { .. } => Ty::Uint8Array { attr },
            Ty::Media(kind, _) => Ty::Media(kind, attr),
            Ty::Literal(lit, freshness, _) => Ty::Literal(lit, freshness, attr),
            Ty::Class(tn, args, _) => Ty::Class(tn, args, attr),
            Ty::Interface(tn, args, associated_bindings, _) => {
                Ty::Interface(tn, args, associated_bindings, attr)
            }
            Ty::Enum(tn, _) => Ty::Enum(tn, attr),
            Ty::EnumVariant(tn, v, _) => Ty::EnumVariant(tn, v, attr),
            Ty::List(inner, _) => Ty::List(inner, attr),
            Ty::Map { key, value, .. } => Ty::Map { key, value, attr },
            Ty::Union(members, _) => Ty::Union(members, attr),
            Ty::Opaque(tn, _) => Ty::Opaque(tn, attr),
            Ty::TypeAlias(tn, _) => Ty::TypeAlias(tn, attr),
            Ty::Function {
                params,
                ret,
                throws,
                ..
            } => Ty::Function {
                params,
                ret,
                throws,
                attr,
            },
            Ty::WatchAccessor(inner, _) => Ty::WatchAccessor(inner, attr),
            Ty::Future(value, error, _) => Ty::Future(value, error, attr),
            Ty::TypeVar(name, _) => Ty::TypeVar(name, attr),
            Ty::AssociatedTypeProjection {
                base,
                interface,
                member,
                ..
            } => Ty::AssociatedTypeProjection {
                base,
                interface,
                member,
                attr,
            },
            Ty::Never { .. } => Ty::Never { attr },
            Ty::Unknown { .. } => Ty::Unknown { attr },
            Ty::Error { .. } => Ty::Error { attr },
            Ty::EvolvingList(inner, _) => Ty::EvolvingList(inner, attr),
            Ty::EvolvingMap(key, value, _) => Ty::EvolvingMap(key, value, attr),
            Ty::RustType { .. } => Ty::RustType { attr },
            Ty::Type { .. } => Ty::Type { attr },
        }
    }

    /// Get the TyAttr for this type.
    pub fn attr(&self) -> &TyAttr {
        match self {
            Ty::Int { attr }
            | Ty::Bigint { attr }
            | Ty::Float { attr }
            | Ty::String { attr }
            | Ty::Bool { attr }
            | Ty::Null { attr }
            | Ty::Void { attr }
            | Ty::BuiltinUnknown { attr }
            | Ty::Uint8Array { attr }
            | Ty::Map { attr, .. }
            | Ty::Function { attr, .. }
            | Ty::AssociatedTypeProjection { attr, .. }
            | Ty::Never { attr }
            | Ty::Unknown { attr }
            | Ty::Error { attr }
            | Ty::RustType { attr }
            | Ty::Type { attr } => attr,
            Ty::Media(_, attr)
            | Ty::Literal(_, _, attr)
            | Ty::Class(_, _, attr)
            | Ty::Interface(_, _, _, attr)
            | Ty::Enum(_, attr)
            | Ty::EnumVariant(_, _, attr)
            | Ty::List(_, attr)
            | Ty::Union(_, attr)
            | Ty::Opaque(_, attr)
            | Ty::TypeAlias(_, attr)
            | Ty::WatchAccessor(_, attr)
            | Ty::Future(_, _, attr)
            | Ty::TypeVar(_, attr)
            | Ty::EvolvingList(_, attr)
            | Ty::EvolvingMap(_, _, attr) => attr,
        }
    }

    // --- Primitive constructors (default TyAttr) ---

    /// `int` with default attributes.
    pub fn int() -> Self {
        Ty::Int {
            attr: TyAttr::default(),
        }
    }

    /// `bigint` with default attributes.
    pub fn bigint() -> Self {
        Ty::Bigint {
            attr: TyAttr::default(),
        }
    }

    /// `float` with default attributes.
    pub fn float() -> Self {
        Ty::Float {
            attr: TyAttr::default(),
        }
    }

    /// `string` with default attributes.
    pub fn string() -> Self {
        Ty::String {
            attr: TyAttr::default(),
        }
    }

    /// `bool` with default attributes.
    pub fn bool() -> Self {
        Ty::Bool {
            attr: TyAttr::default(),
        }
    }

    /// `null` with default attributes.
    pub fn null() -> Self {
        Ty::Null {
            attr: TyAttr::default(),
        }
    }

    /// `uint8array` with default attributes.
    pub fn uint8array() -> Self {
        Ty::Uint8Array {
            attr: TyAttr::default(),
        }
    }

    // --- Compound constructors (default TyAttr) ---

    /// `T?` (optional) — sugar for `T | null`.
    ///
    /// `?` is not its own type: it lowers to a union that includes `null`.
    /// The result is flattened and idempotent — `(A | B)?` becomes a flat
    /// `A | B | null`, `T??` stays `T?`, and `null?` is just `null`.
    pub fn optional(inner: Ty) -> Self {
        match inner {
            Ty::Union(mut members, attr) => {
                if !members.iter().any(Ty::is_null) {
                    members.push(Ty::null());
                }
                Ty::Union(members, attr)
            }
            n @ Ty::Null { .. } => n,
            other => Ty::Union(vec![other, Ty::null()], TyAttr::default()),
        }
    }

    /// True if this is exactly the `null` type.
    pub fn is_null(&self) -> bool {
        matches!(self, Ty::Null { .. })
    }

    /// True if this is a union that includes `null` — i.e. an optional type
    /// after `?` lowering. This is the canonical "is this nullable" predicate;
    /// it replaces matching on the old `Ty::Optional` variant.
    pub fn is_nullable_union(&self) -> bool {
        matches!(self, Ty::Union(members, _) if members.iter().any(Ty::is_null))
    }

    /// Remove `null` from a nullable union, collapsing the result: `T | null`
    /// → `T`, `A | B | null` → `A | B`, a non-nullable type → unchanged. The
    /// inverse direction of [`Ty::optional`]; used where the non-null payload
    /// of an optional is needed (e.g. union-member metadata).
    pub fn strip_null(&self) -> Ty {
        match self {
            Ty::Union(members, attr) => {
                let non_null: Vec<Ty> = members.iter().filter(|m| !m.is_null()).cloned().collect();
                match non_null.len() {
                    0 => self.clone(),
                    1 => non_null.into_iter().next().expect("len checked"),
                    _ => Ty::Union(non_null, attr.clone()),
                }
            }
            _ => self.clone(),
        }
    }

    /// `T[]` (list) with default attributes.
    pub fn list(inner: Ty) -> Self {
        Ty::List(Box::new(inner), TyAttr::default())
    }

    /// `A | B | ...` (union) with default attributes.
    pub fn union(members: impl IntoIterator<Item = Ty>) -> Self {
        Ty::Union(members.into_iter().collect(), TyAttr::default())
    }

    /// `Class(name)` with default attributes (local module path), no type args.
    pub fn class(name: &str) -> Self {
        Ty::Class(TypeName::local(name.into()), Vec::new(), TyAttr::default())
    }

    /// `Class(name, args)` — a parametric class instantiation.
    pub fn class_with_args(name: TypeName, args: Vec<Ty>) -> Self {
        Ty::Class(name, args, TyAttr::default())
    }

    /// `Class(name)` under the `"user"` package (matches compiler2 output for user-defined classes).
    pub fn user_class(name: &str) -> Self {
        Ty::Class(
            QualifiedTypeName::local(Name::new(name)),
            Vec::new(),
            TyAttr::default(),
        )
    }

    /// `Class(name, args)` under the `"user"` package (matches compiler2 output for user-defined classes).
    pub fn user_class_with_args(name: &str, args: Vec<Ty>) -> Self {
        Ty::Class(
            QualifiedTypeName::local(Name::new(name)),
            args,
            TyAttr::default(),
        )
    }

    /// `unknown` with default attributes.
    pub fn unknown() -> Self {
        Ty::BuiltinUnknown {
            attr: TyAttr::default(),
        }
    }

    // --- Opaque type constructors ---

    /// Helper to build an opaque type from a dotted `qualified_name` like
    /// `"baml.llm.Resource"` (first segment = package, last = short name).
    fn opaque_builtin(qualified_name: &str, attr: TyAttr) -> Self {
        Ty::Opaque(QualifiedTypeName::from_dotted_path(qualified_name), attr)
    }

    /// Opaque resource handle type (file, socket, HTTP response body).
    /// NOTE: Uses TyAttr::default(). Callers with a source attr should use opaque_builtin() directly.
    pub fn resource() -> Self {
        Self::opaque_builtin("baml.llm.Resource", TyAttr::default())
    }

    /// Opaque structured prompt tree type for LLM calls.
    /// NOTE: Uses TyAttr::default(). Callers with a source attr should use opaque_builtin() directly.
    pub fn prompt_ast() -> Self {
        Self::opaque_builtin("baml.llm.PromptAst", TyAttr::default())
    }

    /// Meta-type — a runtime value that wraps a `Ty`. Renders as the `type`
    /// keyword (see `Display`) though its qualified name is `baml.reflect.Type`.
    /// NOTE: Uses TyAttr::default(). Callers with a source attr should use opaque_builtin() directly.
    pub fn type_type() -> Self {
        Self::opaque_builtin("baml.reflect.Type", TyAttr::default())
    }

    /// Check if this is an opaque type with the given qualified name
    /// (e.g. `"baml.llm.PromptAst"`).
    pub fn is_opaque(&self, qualified_name: &str) -> bool {
        match self {
            Ty::Opaque(tn, _) => tn.render_dotted(false) == qualified_name,
            _ => false,
        }
    }

    /// If this is an opaque type, return its TypeName.
    pub fn as_opaque(&self) -> Option<&TypeName> {
        match self {
            Ty::Opaque(tn, _) => Some(tn),
            _ => None,
        }
    }

    /// Check if this is the void type.
    pub fn is_void(&self) -> bool {
        matches!(self, Ty::Void { .. })
    }

    /// Check if this is a primitive type (including literals of primitive types).
    pub fn is_primitive(&self) -> bool {
        matches!(
            self,
            Ty::Int { .. }
                | Ty::Bigint { .. }
                | Ty::Float { .. }
                | Ty::String { .. }
                | Ty::Bool { .. }
                | Ty::Null { .. }
                | Ty::Uint8Array { .. }
                | Ty::Literal(..)
        )
    }

    /// Check if this type is a subtype of another.
    ///
    /// Returns true if `self` can be used where `other` is expected.
    /// Ported from VIR `ty.rs:93-140` with literal subtyping rules.
    ///
    /// Note: TyAttr does NOT affect subtyping. Two types with different
    /// attrs are not subtypes of each other (they're different types via
    /// PartialEq), but attr content isn't checked for subtype relationships.
    ///
    /// Note: Unknown/Error/Never handling is not needed here because:
    /// - Unknown/Error are mapped to Null during TIR→baml_type conversion
    /// - Never is mapped to Void during VIR lowering
    /// - All real type checking (where those variants matter) happens in TIR
    ///
    /// Structural subtyping for `Ty`. This is the runtime / SAP analogue of
    /// `baml_compiler2_tir::normalize::is_subtype_of`. The relation is purely
    /// structural — only representation-preserving widenings are allowed.
    /// Representation-changing numeric coercions (`int → bigint`, `int → float`,
    /// and their literal forms) are not subtype relations: `int → bigint`
    /// happens only at the FFI boundary (`bex_engine::conversion`), and
    /// `int → float` requires an explicit `float` literal. Keep behaviour
    /// aligned with `crate::normalize::is_subtype_of` in TIR.
    pub fn is_subtype_of(&self, other: &Ty) -> bool {
        // Same types are subtypes
        if self == other {
            return true;
        }

        // Any type is a subtype of BuiltinUnknown (it accepts everything)
        if matches!(other, Ty::BuiltinUnknown { .. }) {
            return true;
        }

        match (self, other) {
            // Literal types are subtypes of their corresponding primitives.
            // (Same representation — these are free widenings, like
            // `Literal(Int 42) <: Int`.)
            (Ty::Literal(Literal::Int(_), _, _), Ty::Int { .. }) => true,
            (Ty::Literal(Literal::Float(_), _, _), Ty::Float { .. }) => true,
            (Ty::Literal(Literal::String(_), _, _), Ty::String { .. }) => true,
            (Ty::Literal(Literal::Bool(_), _, _), Ty::Bool { .. }) => true,
            (Ty::Literal(Literal::Bigint(_), _, _), Ty::Bigint { .. }) => true,

            // T is a subtype of T | U (union containing T). Subsumes the former
            // `Optional` rules: `?` is now `T | null`, so `null <: T | null` and
            // `T <: T | null` both fall out of union membership.
            (inner, Ty::Union(types, _)) => types.iter().any(|t| inner.is_subtype_of(t)),

            // Union<T1, T2> is a subtype of U if all Ti are subtypes of U
            (Ty::Union(types, _), other) => types.iter().all(|t| t.is_subtype_of(other)),

            // List: structural recursion. Since this impl is coercion-free,
            // recursion via `is_subtype_of` only admits free widenings —
            // `int[]` is **not** a subtype of `bigint[]`/`float[]`.
            (Ty::List(inner1, _), Ty::List(inner2, _)) => inner1.is_subtype_of(inner2),

            // Map: structural recursion in both key and value. Same coercion-
            // free semantics as `List` — values cannot widen across
            // representation boundaries.
            (
                Ty::Map {
                    key: k1, value: v1, ..
                },
                Ty::Map {
                    key: k2, value: v2, ..
                },
            ) => k1.is_subtype_of(k2) && v1.is_subtype_of(v2),

            // Note: `int <: bigint`, `int <: float`, and the literal-int
            // widenings to bigint/float are intentionally absent — numeric
            // types do not widen across representations in the type system (TIR
            // matches this). `int → bigint` is an FFI-boundary coercion only.
            _ => false,
        }
    }

    /// Returns true if this type is a compiler-only variant that should
    /// never appear at runtime.
    pub fn is_compiler_only(&self) -> bool {
        matches!(
            self,
            Ty::Function { .. }
                | Ty::Void { .. }
                | Ty::WatchAccessor(..)
                | Ty::BuiltinUnknown { .. }
        )
    }

    /// Recursively walk this type tree and return an error if any compiler-only
    /// variants are found.
    pub fn validate_runtime(&self) -> Result<(), String> {
        match self {
            // Recursive type aliases are intentionally preserved at runtime
            // for output format rendering (cycle detection needs the alias name).
            Ty::TypeAlias(_, _) => Ok(()),
            Ty::Void { .. } => Err("Void type should not reach runtime".to_string()),
            Ty::WatchAccessor(inner, _) => inner.validate_runtime(),
            Ty::BuiltinUnknown { .. } => Ok(()),
            // Recurse into containers
            Ty::List(inner, _) => inner.validate_runtime(),
            Ty::Map { key, value, .. } => {
                key.validate_runtime()?;
                value.validate_runtime()
            }
            Ty::Union(members, _) => {
                for m in members {
                    m.validate_runtime()?;
                }
                Ok(())
            }
            // All other variants are fine at runtime
            Ty::Function {
                params,
                ret,
                throws,
                ..
            } => {
                for p in params {
                    p.validate_runtime()?;
                }
                ret.validate_runtime()?;
                if matches!(throws.as_ref(), Ty::Void { .. }) {
                    Ok(())
                } else {
                    throws.validate_runtime()
                }
            }
            Ty::Future(value, error, _) => {
                value.validate_runtime()?;
                error.validate_runtime()
            }
            Ty::Class(_, args, _) => {
                for a in args {
                    a.validate_runtime()?;
                }
                Ok(())
            }
            Ty::Interface(_, args, associated_bindings, _) => {
                for a in args {
                    a.validate_runtime()?;
                }
                for (_, ty) in associated_bindings {
                    ty.validate_runtime()?;
                }
                Ok(())
            }
            // TIR-only variants must have been erased before runtime.
            Ty::TypeVar(..)
            | Ty::AssociatedTypeProjection { .. }
            | Ty::Never { .. }
            | Ty::Unknown { .. }
            | Ty::Error { .. }
            | Ty::EvolvingList(..)
            | Ty::EvolvingMap(..)
            | Ty::RustType { .. }
            | Ty::Type { .. } => Err("compiler-only type should not reach runtime".to_string()),
            Ty::Int { .. }
            | Ty::Bigint { .. }
            | Ty::Float { .. }
            | Ty::String { .. }
            | Ty::Bool { .. }
            | Ty::Null { .. }
            | Ty::Media(..)
            | Ty::Uint8Array { .. }
            | Ty::Literal(..)
            | Ty::Enum(..)
            | Ty::EnumVariant(..)
            | Ty::Opaque(..) => Ok(()),
        }
    }

    fn needs_postfix_parens(&self) -> bool {
        matches!(self, Ty::Union(..) | Ty::Function { .. })
    }

    fn needs_function_result_parens(&self) -> bool {
        matches!(self, Ty::Function { .. })
    }

    fn fmt_as_postfix_base(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.needs_postfix_parens() {
            write!(f, "({self})")
        } else {
            write!(f, "{self}")
        }
    }

    fn fmt_as_function_result(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.needs_function_result_parens() {
            write!(f, "({self})")
        } else {
            write!(f, "{self}")
        }
    }
}

impl fmt::Display for Ty {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Ty::Int { .. } => write!(f, "int"),
            Ty::Float { .. } => write!(f, "float"),
            Ty::String { .. } => write!(f, "string"),
            Ty::Bool { .. } => write!(f, "bool"),
            Ty::Null { .. } => write!(f, "null"),
            Ty::Uint8Array { .. } => write!(f, "uint8array"),
            Ty::Media(kind, _) => write!(f, "{kind}"),
            Ty::Literal(lit, _, _) => match lit {
                Literal::Int(i) => write!(f, "{i}"),
                Literal::Bigint(n) => write!(f, "{n}n"),
                Literal::Float(s) => write!(f, "{s}"),
                Literal::String(s) => write!(f, "{s:?}"),
                Literal::Bool(b) => write!(f, "{b}"),
            },
            Ty::Bigint { .. } => write!(f, "bigint"),
            Ty::Class(tn, args, _) => {
                write!(f, "{}", tn.display_name())?;
                if !args.is_empty() {
                    write!(f, "<")?;
                    for (i, arg) in args.iter().enumerate() {
                        if i > 0 {
                            write!(f, ", ")?;
                        }
                        write!(f, "{arg}")?;
                    }
                    write!(f, ">")?;
                }
                Ok(())
            }
            Ty::Interface(tn, args, associated_bindings, _) => {
                write!(f, "{}", tn.display_name())?;
                if !args.is_empty() || !associated_bindings.is_empty() {
                    write!(f, "<")?;
                    let mut first = true;
                    for arg in args {
                        if !first {
                            write!(f, ", ")?;
                        }
                        first = false;
                        write!(f, "{arg}")?;
                    }
                    for (name, ty) in associated_bindings {
                        if !first {
                            write!(f, ", ")?;
                        }
                        first = false;
                        write!(f, "{name} = {ty}")?;
                    }
                    write!(f, ">")?;
                }
                Ok(())
            }
            Ty::Enum(tn, _) => write!(f, "{}", tn.display_name()),
            Ty::EnumVariant(tn, variant, _) => write!(f, "{}.{variant}", tn.display_name()),
            Ty::Opaque(tn, _) => {
                // The reflection meta-type renders as the `type` keyword even
                // though its qualified name is `baml.reflect.Type`.
                if tn.render_dotted(false) == "baml.reflect.Type" {
                    write!(f, "type")
                } else {
                    write!(f, "{}", tn.display_name())
                }
            }
            Ty::TypeAlias(tn, _) => write!(f, "{}", tn.display_name()),
            Ty::List(inner, _) => {
                inner.fmt_as_postfix_base(f)?;
                write!(f, "[]")
            }
            Ty::Map { key, value, .. } => write!(f, "map<{key}, {value}>"),
            Ty::Union(types, _) => {
                // `?` is sugar that exists only in source/lowering; after that a
                // nullable type is a plain union and renders as `T | null`.
                // Function members are parenthesized so a nullable callback reads
                // as `((..) -> ..) | null`, not a function with `throws .. | null`.
                let parts: Vec<std::string::String> = types
                    .iter()
                    .map(|ty| {
                        let rendered = ty.to_string();
                        if matches!(ty, Ty::Function { .. }) {
                            format!("({rendered})")
                        } else {
                            rendered
                        }
                    })
                    .collect();
                write!(f, "{}", parts.join(" | "))
            }
            Ty::Function {
                params,
                ret,
                throws,
                ..
            } => {
                let param_strs: Vec<std::string::String> = params
                    .iter()
                    .map(std::string::ToString::to_string)
                    .collect();
                let throws_display = if matches!(throws.as_ref(), Ty::Void { .. }) {
                    "never".to_string()
                } else {
                    throws.to_string()
                };
                write!(f, "({}) -> ", param_strs.join(", "))?;
                ret.fmt_as_function_result(f)?;
                write!(f, " throws {}", throws_display)
            }
            Ty::Void { .. } => write!(f, "void"),
            Ty::WatchAccessor(inner, _) => write!(f, "{inner}.$watch"),
            Ty::BuiltinUnknown { .. } => write!(f, "unknown"),
            Ty::Future(value, error, _) => write!(f, "future<{value}, {error}>"),
            Ty::TypeVar(name, _) => write!(f, "{name}"),
            Ty::AssociatedTypeProjection {
                base,
                interface,
                member,
                ..
            } => match interface {
                Some(iface) => write!(f, "({base} as {iface}).{member}"),
                None => write!(f, "{base}.{member}"),
            },
            Ty::Never { .. } => write!(f, "never"),
            Ty::Unknown { .. } => write!(f, "unknown"),
            Ty::Error { .. } => write!(f, "<error>"),
            Ty::EvolvingList(inner, _) => write!(f, "{inner}[]"),
            Ty::EvolvingMap(key, value, _) => write!(f, "map<{key}, {value}>"),
            Ty::RustType { .. } => write!(f, "RustType"),
            Ty::Type { .. } => write!(f, "type"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Shorthand helpers for tests — all use default TyAttr.
    fn ty_int() -> Ty {
        Ty::Int {
            attr: TyAttr::default(),
        }
    }
    fn ty_float() -> Ty {
        Ty::Float {
            attr: TyAttr::default(),
        }
    }
    fn ty_string() -> Ty {
        Ty::String {
            attr: TyAttr::default(),
        }
    }
    fn ty_bool() -> Ty {
        Ty::Bool {
            attr: TyAttr::default(),
        }
    }
    fn ty_null() -> Ty {
        Ty::Null {
            attr: TyAttr::default(),
        }
    }

    #[test]
    fn test_literal_int_subtype_of_int() {
        let lit_42 = Ty::Literal(Literal::Int(42), Freshness::Regular, TyAttr::default());
        assert!(lit_42.is_subtype_of(&ty_int()));
    }

    #[test]
    fn test_literal_float_subtype_of_float() {
        let lit_3_14 = Ty::Literal(
            Literal::Float("3.14".to_string()),
            Freshness::Regular,
            TyAttr::default(),
        );
        assert!(lit_3_14.is_subtype_of(&ty_float()));
    }

    #[test]
    fn test_literal_int_does_not_widen_to_float() {
        // `baml_type::Ty::is_subtype_of` is coercion-free; the int-literal
        // → float widening is a representation change, not a structural
        // subtype. TIR keeps the scalar widening as a runtime coercion
        // (MIR-level), not as a subtype relation modeled here.
        let lit_42 = Ty::Literal(Literal::Int(42), Freshness::Regular, TyAttr::default());
        assert!(!lit_42.is_subtype_of(&ty_float()));
    }

    #[test]
    fn test_literal_string_subtype_of_string() {
        let lit_hello = Ty::Literal(
            Literal::String("hello".to_string()),
            Freshness::Regular,
            TyAttr::default(),
        );
        assert!(lit_hello.is_subtype_of(&ty_string()));
    }

    #[test]
    fn test_literal_bool_subtype_of_bool() {
        let lit_true = Ty::Literal(Literal::Bool(true), Freshness::Regular, TyAttr::default());
        assert!(lit_true.is_subtype_of(&ty_bool()));
    }

    #[test]
    fn test_literal_in_union() {
        let lit_42 = Ty::Literal(Literal::Int(42), Freshness::Regular, TyAttr::default());
        let union_type = Ty::Union(vec![ty_string(), ty_int()], TyAttr::default());
        assert!(lit_42.is_subtype_of(&union_type));
    }

    #[test]
    fn test_literal_float_in_union() {
        let lit_3_14 = Ty::Literal(
            Literal::Float("3.14".to_string()),
            Freshness::Regular,
            TyAttr::default(),
        );
        let union_type = Ty::Union(vec![ty_string(), ty_float()], TyAttr::default());
        assert!(lit_3_14.is_subtype_of(&union_type));
    }

    #[test]
    fn test_literal_in_optional() {
        let lit_42 = Ty::Literal(Literal::Int(42), Freshness::Regular, TyAttr::default());
        let opt_int = Ty::optional(ty_int());
        assert!(lit_42.is_subtype_of(&opt_int));
    }

    #[test]
    fn test_null_subtype_of_optional() {
        let opt_string = Ty::optional(ty_string());
        assert!(ty_null().is_subtype_of(&opt_string));
    }

    #[test]
    fn test_int_not_subtype_of_float() {
        // Coercion-free: `int` is i64, `float` is f64. Values past 2^53 lose
        // precision; TIR removed this scalar rule, and `baml_type` mirrors it.
        assert!(!ty_int().is_subtype_of(&ty_float()));
    }

    #[test]
    fn test_int_not_subtype_of_bigint() {
        // Scalar int→bigint widening is a representation change (i64 → heap
        // BigInt) and is not a subtype relation in either `baml_type` or TIR;
        // it happens only at the FFI boundary.
        assert!(!ty_int().is_subtype_of(&Ty::Bigint {
            attr: TyAttr::default()
        }));
    }

    #[test]
    fn test_int_array_not_subtype_of_bigint_array() {
        // Regression: container invariance. `int[]` must not be a subtype
        // of `bigint[]`.
        let int_arr = Ty::List(Box::new(ty_int()), TyAttr::default());
        let bigint_arr = Ty::List(
            Box::new(Ty::Bigint {
                attr: TyAttr::default(),
            }),
            TyAttr::default(),
        );
        assert!(!int_arr.is_subtype_of(&bigint_arr));
    }

    #[test]
    fn test_list_covariance() {
        let list_lit = Ty::List(
            Box::new(Ty::Literal(
                Literal::Int(42),
                Freshness::Regular,
                TyAttr::default(),
            )),
            TyAttr::default(),
        );
        let list_int = Ty::List(Box::new(ty_int()), TyAttr::default());
        assert!(list_lit.is_subtype_of(&list_int));
    }

    #[test]
    fn test_validate_runtime_accepts_core_types() {
        assert!(ty_int().validate_runtime().is_ok());
        assert!(ty_float().validate_runtime().is_ok());
        assert!(ty_string().validate_runtime().is_ok());
        assert!(
            Ty::Literal(
                Literal::Float("3.14".to_string()),
                Freshness::Regular,
                TyAttr::default()
            )
            .validate_runtime()
            .is_ok()
        );
    }

    #[test]
    fn test_validate_runtime_accepts_opaque_types() {
        assert!(Ty::resource().validate_runtime().is_ok());
        assert!(Ty::prompt_ast().validate_runtime().is_ok());
        assert!(Ty::type_type().validate_runtime().is_ok());
    }

    #[test]
    fn test_display_opaque_types() {
        assert_eq!(Ty::resource().to_string(), "baml.llm.Resource");
        assert_eq!(Ty::prompt_ast().to_string(), "baml.llm.PromptAst");
        assert_eq!(Ty::type_type().to_string(), "type");
    }

    #[test]
    fn test_opaque_helpers() {
        assert!(Ty::resource().is_opaque("baml.llm.Resource"));
        assert!(!Ty::resource().is_opaque("baml.reflect.Type"));
        assert_eq!(
            Ty::prompt_ast().as_opaque().map(|tn| tn.name().as_str()),
            Some("PromptAst"),
        );
        assert_eq!(ty_int().as_opaque(), None);
    }

    #[test]
    fn test_display_nullable_union_is_plain_union() {
        // `?` does not survive lowering; a nullable type displays as a union.
        let ty = Ty::optional(Ty::union([ty_int(), ty_string()]));
        assert_eq!(ty.to_string(), "int | string | null");
    }

    #[test]
    fn test_display_list_union_parenthesized() {
        let ty = Ty::list(Ty::union([ty_int(), ty_string()]));
        assert_eq!(ty.to_string(), "(int | string)[]");
    }

    #[test]
    fn test_validate_runtime_rejects_compiler_types() {
        assert!(
            (Ty::Void {
                attr: TyAttr::default()
            })
            .validate_runtime()
            .is_err()
        );
        // TypeAlias is now allowed at runtime for recursive type alias rendering
        assert!(
            Ty::TypeAlias(TypeName::local(Name::new("MyAlias")), TyAttr::default())
                .validate_runtime()
                .is_ok()
        );
    }

    #[test]
    fn test_function_display_uses_never_for_void_throws_sentinel() {
        let ty = Ty::Function {
            params: vec![ty_int()],
            ret: Box::new(ty_string()),
            throws: Box::new(Ty::Void {
                attr: TyAttr::default(),
            }),
            attr: TyAttr::default(),
        };

        assert_eq!(ty.to_string(), "(int) -> string throws never");
        assert!(ty.validate_runtime().is_ok());
    }

    #[test]
    fn test_function_display_parenthesizes_nested_function_returns() {
        let ty = Ty::Function {
            params: vec![],
            ret: Box::new(Ty::Function {
                params: vec![ty_int()],
                ret: Box::new(ty_string()),
                throws: Box::new(Ty::Void {
                    attr: TyAttr::default(),
                }),
                attr: TyAttr::default(),
            }),
            throws: Box::new(Ty::Void {
                attr: TyAttr::default(),
            }),
            attr: TyAttr::default(),
        };

        assert_eq!(
            ty.to_string(),
            "() -> ((int) -> string throws never) throws never"
        );
    }

    #[test]
    fn test_function_display_parenthesizes_function_postfix_types() {
        let callback = Ty::Function {
            params: vec![ty_int()],
            ret: Box::new(ty_string()),
            throws: Box::new(Ty::Void {
                attr: TyAttr::default(),
            }),
            attr: TyAttr::default(),
        };

        assert_eq!(
            Ty::optional(callback.clone()).to_string(),
            "((int) -> string throws never) | null"
        );
        assert_eq!(
            Ty::list(callback).to_string(),
            "((int) -> string throws never)[]"
        );
    }
}

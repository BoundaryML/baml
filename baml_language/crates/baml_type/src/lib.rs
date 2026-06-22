//! Unified type system for BAML.
//!
//! Includes five main type representations:
//! - [`Ty`]: the full type representation containing all compiler and runtime types.
//! - [`RuntimeTy`]: the runtime-facing deep subset of [`Ty`] that excludes type inference helper types.
//! - [`ConcreteTy`]: like [`RuntimeTy`] but only the concrete-layout variants (plus `never`).
//!   Not deep: parameter types are [`RuntimeTy`]s.
//! - [`RealizedTy`]: the realized deep subset of [`RuntimeTy`] that excludes type variables.
//! - [`ConcreteRealizedTy`]: like [`RealizedTy`] but only contains concrete types.
//!   Not deep: parameter types are [`RealizedTy`]s.
//!
//! The following represents the infallible conversion hierarchy:
//! ```text
//!     `Ty`
//!      ↑
//!  `RuntimeTy` <- `RealizedTy`
//!      ↑               ↑
//! `ConcreteTy` <- `ConcreteRealizedTy`
//! ```

use std::fmt;

// Re-export core baml_base types so downstream crates can depend on baml_type
// instead of baml_base directly.
pub use baml_base::{Literal, MediaKind, Name, Span};
use borsh::{BorshDeserialize, BorshSerialize};

mod attr;
mod defs;
mod family;
mod names;
pub mod normalize;
mod primitive;
mod realized_ty;
mod runtime_ty;
pub mod simplify_sap;
pub mod template;
pub mod typetag;
pub use attr::*;
pub use defs::*;
pub use family::*;
pub use names::*;
pub use primitive::*;
pub use runtime_ty::*;
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

/// Whether a [`FunctionParamTy`] is required or optional (has a default value).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, BorshSerialize, BorshDeserialize,
)]
pub enum FunctionParamMode {
    Required,
    Optional,
}

/// Flatten, deduplicate, and collapse a vec of widened types into a single `Ty`.
///
/// After `widen_fresh()` has run on each union member, multiple members may
/// have widened to the same primitive (e.g. `[Literal(1,Fresh), Literal(2,Fresh)]`
/// both become `Int`). This helper deduplicates and collapses:
/// - Flattens nested unions one level
/// - Deduplicates by `PartialEq`
/// - Unwraps singletons
fn dedup_and_collapse(types: Vec<Ty>, attr: TyAttr) -> Ty {
    let mut members: Vec<Ty> = Vec::new();
    for ty in types {
        match ty {
            Ty::Union(inner, _) => {
                for m in inner {
                    if !members.contains(&m) {
                        members.push(m);
                    }
                }
            }
            _ => {
                if !members.contains(&ty) {
                    members.push(ty);
                }
            }
        }
    }
    match members.len() {
        0 => Ty::Never { attr },
        1 => members.into_iter().next().unwrap(),
        _ => Ty::Union(members, attr),
    }
}

impl Ty {
    /// Whether this type may be the *implementor* (`for`-target) of an interface
    /// implementation — i.e. `implement I for <Self>` is allowed.
    ///
    /// Valid: concrete user-facing types with a single runtime representation that
    /// dispatch can key on — the primitives, `Media`, classes, enums, the
    /// containers `T[]` / `map<K, V>`, the builtin handles `Type` / `Resource` /
    /// `PromptAst`, and a blanket type parameter or its associated-type projection
    /// (both stand for a concrete type at use sites). `Never` is vacuously allowed
    /// (it has no values).
    ///
    /// Rejected:
    ///   - `Future` — the runtime impl registry's `TyTemplate` has no constructor for
    ///     it, so its `value`/`error` args can't carry a `TypeArgRef`: a generic
    ///     `Future<T>` for-type would bake a `Concrete` rule carrying an unbindable bare
    ///     `T` that never matches a real `Future<int>`. Rejected outright rather than
    ///     silently mis-dispatching; revisit if a `TyTemplate::Future` is ever added.
    ///   - `Literal` / `EnumVariant` — singleton subtypes whose values dispatch
    ///     through their base (`int`, `Color`), so they have no implementor of
    ///     their own;
    ///   - `Interface` (existential) and `Union` — no single concrete implementor;
    ///   - `Function` (an arrow type) and `RustType` (an opaque native leaf);
    ///   - `WatchAccessor` — the compiler-internal type of `x.$watch`, not a
    ///     user-facing data type;
    ///   - `Void`, the top type `BuiltinUnknown`, and the compiler-only sentinels
    ///     `Unknown` / `Error` / `EvolvingList` / `EvolvingMap`;
    ///   - `TypeAlias` — callers resolve aliases first, so a surviving alias here
    ///     is unresolved (recursive or missing), i.e. not a valid bare target.
    ///
    /// The match is intentionally exhaustive (no wildcard) so a new `Ty` variant
    /// must be classified here rather than silently defaulting to implementable.
    pub fn is_valid_impl_subject(&self) -> bool {
        match self {
            Ty::Int { .. }
            | Ty::Bigint { .. }
            | Ty::Float { .. }
            | Ty::String { .. }
            | Ty::Bool { .. }
            | Ty::Null { .. }
            | Ty::Uint8Array { .. }
            | Ty::Media(..)
            | Ty::Class(..)
            | Ty::Enum(..)
            | Ty::List(..)
            | Ty::Map { .. }
            | Ty::Type { .. }
            | Ty::Resource { .. }
            | Ty::PromptAst { .. }
            | Ty::Never { .. }
            | Ty::TypeVar(..)
            | Ty::AssociatedTypeProjection { .. } => true,
            Ty::Literal(..)
            | Ty::EnumVariant(..)
            | Ty::Interface(..)
            | Ty::Union(..)
            | Ty::Future(..)
            | Ty::Function { .. }
            | Ty::RustType { .. }
            | Ty::WatchAccessor(..)
            | Ty::TypeAlias(..)
            | Ty::Void { .. }
            | Ty::BuiltinUnknown { .. }
            | Ty::Unknown { .. }
            | Ty::Error { .. }
            | Ty::EvolvingList(..)
            | Ty::EvolvingMap(..) => false,
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

    /// Widen fresh literal types to their base primitive.
    ///
    /// Called at mutable binding sites (`let` without annotation).
    /// Regular (non-fresh) literals pass through unchanged.
    ///
    /// Recurses into `Union`, `List`, `Map`, and `Optional` so that compound
    /// types like `(1 | 2 | 3)[]` widen to `int[]` at unannotated bindings.
    #[must_use]
    pub fn widen_fresh(self) -> Ty {
        match self {
            Ty::Literal(lit, Freshness::Fresh, attr) => match PrimitiveType::from_literal(&lit) {
                PrimitiveType::Int => Ty::Int { attr },
                PrimitiveType::Bigint => Ty::Bigint { attr },
                PrimitiveType::Float => Ty::Float { attr },
                PrimitiveType::String => Ty::String { attr },
                PrimitiveType::Bool => Ty::Bool { attr },
                PrimitiveType::Null => Ty::Null { attr },
                PrimitiveType::Uint8Array => Ty::Uint8Array { attr },
                PrimitiveType::Image => Ty::Media(MediaKind::Image, attr),
                PrimitiveType::Audio => Ty::Media(MediaKind::Audio, attr),
                PrimitiveType::Video => Ty::Media(MediaKind::Video, attr),
                PrimitiveType::Pdf => Ty::Media(MediaKind::Pdf, attr),
            },
            Ty::Union(members, attr) => {
                let widened: Vec<Ty> = members.into_iter().map(Ty::widen_fresh).collect();
                dedup_and_collapse(widened, attr)
            }
            Ty::List(inner, attr) => Ty::List(Box::new((*inner).widen_fresh()), attr),
            Ty::Map {
                key: k,
                value: v,
                attr,
            } => Ty::Map {
                key: Box::new((*k).widen_fresh()),
                value: Box::new((*v).widen_fresh()),
                attr,
            },
            Ty::Class(name, type_args, attr) => {
                let widened: Vec<Ty> = type_args.into_iter().map(Ty::widen_fresh).collect();
                Ty::Class(name, widened, attr)
            }
            other => other,
        }
    }

    /// Promote empty containers to evolving containers.
    ///
    /// Called at mutable binding sites (`let` without annotation), right
    /// after `widen_fresh()`. This is the mirror of `widen_fresh()`:
    /// - `widen_fresh` *removes* literal specificity (1 → int)
    /// - `make_evolving` *adds* container mutability (List(Never) → EvolvingList(Never))
    ///
    /// Only converts `List(Never)` and `Map(Never, Never)` — non-empty
    /// container literals already have a known element type and don't need
    /// evolving semantics.
    #[must_use]
    pub fn make_evolving(self) -> Ty {
        match self {
            Ty::List(inner, attr) if matches!(*inner, Ty::Never { .. }) => {
                Ty::EvolvingList(inner, attr)
            }
            Ty::Map {
                key: k,
                value: v,
                attr,
            } if matches!(*k, Ty::Never { .. }) && matches!(*v, Ty::Never { .. }) => {
                Ty::EvolvingMap(k, v, attr)
            }
            other => other,
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

    pub fn type_var(name: &str) -> Self {
        Ty::TypeVar(Name::new(name), TyAttr::default())
    }

    /// View this type as an [`Interface`] constraint when it is an interface
    /// existential ([`Ty::Interface`]); `None` for any other type. Attributes
    /// are dropped — the constraint is identity data (name, generic arguments,
    /// associated-type bindings), not SAP metadata. The inverse of
    /// [`Interface::to_ty`].
    pub fn as_interface(&self) -> Option<Interface> {
        match self {
            Ty::Interface(name, generics, associated_types, _) => Some(Interface {
                name: name.clone(),
                generics: generics.clone(),
                associated_types: associated_types.clone(),
            }),
            _ => None,
        }
    }

    // --- Opaque leaf-type constructors (default TyAttr) ---

    /// Opaque resource handle type (file, socket, HTTP response body).
    /// Renders as `baml.llm.Resource`.
    pub fn resource() -> Self {
        Ty::Resource {
            attr: TyAttr::default(),
        }
    }

    /// Opaque structured prompt tree type for LLM calls.
    /// Renders as `baml.llm.PromptAst`.
    pub fn prompt_ast() -> Self {
        Ty::PromptAst {
            attr: TyAttr::default(),
        }
    }

    /// Meta-type — a runtime value that wraps a `Ty`. Renders as the `type`
    /// keyword though its qualified name is `baml.reflect.Type`.
    pub fn type_type() -> Self {
        Ty::Type {
            attr: TyAttr::default(),
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
                    p.ty.validate_runtime()?;
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
            | Ty::EvolvingMap(..) => Err("compiler-only type should not reach runtime".to_string()),
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
            // The opaque leaf concrete types are genuine runtime types (their
            // values live as concrete Rust types on the VM heap): `type`
            // (reflection), `$rust_type` (Rust-managed field state), resource
            // handles, and prompt trees.
            | Ty::RustType { .. }
            | Ty::Type { .. }
            | Ty::Resource { .. }
            | Ty::PromptAst { .. } => Ok(()),
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

// ── Strategy-based rendering ─────────────────────────────────────────────────

/// Strategy controlling how a [`Ty`] renders its leaf names plus a couple of
/// presentation choices. A single recursive renderer ([`Ty::render_with`])
/// walks the structure; everything package-, type-var-, or context-specific
/// lives behind this trait. This is the one place type *structure* is turned
/// into text — the canonical dump renderer, user-facing diagnostics, and the
/// LSP's context-aware hover all implement this trait instead of re-walking
/// `Ty` (the former "~10 renderers").
pub trait TyRenderStrategy {
    /// Render a qualified name's dotted path (package/namespace/name) *without*
    /// any `<...>` suffix; the renderer appends any type args separately.
    fn qtn(&self, qtn: &QualifiedTypeName) -> String;

    /// Render a type-variable name (`T`, or a synthetic effect param).
    fn type_var(&self, name: &Name) -> String;

    /// Whether evolving list/map types are annotated `(evolving)`.
    /// Canonical/user-facing: yes; the LSP's hover hides it.
    fn show_evolving(&self) -> bool {
        true
    }
}

impl Ty {
    /// User-facing rendering: identical to the canonical render
    /// ([`Ty::render_canonical`]) except the reserved implicit `user` package is
    /// elided ([`RESERVED_USER_PACKAGE`]) and synthetic effect params show as
    /// `callback`. This is the single structural source of the "no `user.` in
    /// messages" rule — diagnostics render through here instead of
    /// post-processing the canonical string.
    pub fn render_user_facing(&self) -> String {
        self.render_with(&CanonicalTyRender { user_facing: true })
    }

    /// Canonical structural rendering — fully-qualified leaf names (including the
    /// implicit `user` package). This is what TIR's dump output expects.
    pub fn render_canonical(&self) -> String {
        self.render_with(&CanonicalTyRender { user_facing: false })
    }

    /// Render with parentheses if needed for postfix (`[]`/`?`) context.
    fn render_as_postfix_base(&self, s: &dyn TyRenderStrategy) -> String {
        let inner = self.render_with(s);
        if self.needs_postfix_parens() {
            format!("({inner})")
        } else {
            inner
        }
    }

    /// Render with parentheses if needed in a function-return position.
    fn render_as_function_result(&self, s: &dyn TyRenderStrategy) -> String {
        let inner = self.render_with(s);
        if self.needs_function_result_parens() {
            format!("({inner})")
        } else {
            inner
        }
    }

    /// The single structural renderer. Walks the type, delegating every
    /// package-, type-var-, and presentation-policy decision to `s`. All type
    /// rendering — canonical dumps, user-facing diagnostics, LSP hover —
    /// funnels through here so the structure is described in exactly one place.
    pub fn render_with(&self, s: &dyn TyRenderStrategy) -> String {
        match self {
            Ty::Class(qn, type_args, _) => {
                let mut out = s.qtn(qn);
                if !type_args.is_empty() {
                    let args: Vec<_> = type_args.iter().map(|a| a.render_with(s)).collect();
                    out.push('<');
                    out.push_str(&args.join(", "));
                    out.push('>');
                }
                out
            }
            Ty::Interface(qn, type_args, associated_bindings, _) => {
                let mut out = s.qtn(qn);
                if !type_args.is_empty() || !associated_bindings.is_empty() {
                    let mut args: Vec<_> = type_args.iter().map(|a| a.render_with(s)).collect();
                    args.extend(
                        associated_bindings
                            .iter()
                            .map(|(name, ty)| format!("{name} = {}", ty.render_with(s))),
                    );
                    out.push('<');
                    out.push_str(&args.join(", "));
                    out.push('>');
                }
                out
            }
            Ty::Enum(qn, _) | Ty::TypeAlias(qn, _) => s.qtn(qn),
            Ty::EnumVariant(qn, v, _) => format!("{}.{v}", s.qtn(qn)),
            Ty::Int { .. } => PrimitiveType::Int.to_string(),
            Ty::Bigint { .. } => PrimitiveType::Bigint.to_string(),
            Ty::Float { .. } => PrimitiveType::Float.to_string(),
            Ty::String { .. } => PrimitiveType::String.to_string(),
            Ty::Bool { .. } => PrimitiveType::Bool.to_string(),
            Ty::Null { .. } => PrimitiveType::Null.to_string(),
            Ty::Uint8Array { .. } => PrimitiveType::Uint8Array.to_string(),
            Ty::Media(kind, _) => kind.to_string(),
            Ty::List(inner, _) => format!("{}[]", inner.render_as_postfix_base(s)),
            Ty::Map {
                key: k, value: v, ..
            } => format!("map<{}, {}>", k.render_with(s), v.render_with(s)),
            Ty::EvolvingList(inner, _) => {
                if matches!(**inner, Ty::Never { .. }) {
                    "_[]".to_string()
                } else if s.show_evolving() {
                    format!("{}[] (evolving)", inner.render_as_postfix_base(s))
                } else {
                    format!("{}[]", inner.render_as_postfix_base(s))
                }
            }
            Ty::EvolvingMap(k, v, _) => {
                if matches!(**k, Ty::Never { .. }) && matches!(**v, Ty::Never { .. }) {
                    "map<_, _>".to_string()
                } else if s.show_evolving() {
                    format!("map<{}, {}> (evolving)", k.render_with(s), v.render_with(s))
                } else {
                    format!("map<{}, {}>", k.render_with(s), v.render_with(s))
                }
            }
            Ty::Union(members, _) => {
                // `?` is sugar that exists only in source/lowering; after that a
                // nullable type is a plain union and renders as `T | null`.
                // Function members are parenthesized so a nullable callback reads
                // as `((..) -> ..) | null`, not a function with `throws .. | null`.
                members
                    .iter()
                    .map(|m| {
                        let rendered = m.render_with(s);
                        if matches!(m, Ty::Function { .. }) {
                            format!("({rendered})")
                        } else {
                            rendered
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(" | ")
            }
            Ty::Literal(lit, _freshness, _) => lit.to_string(),
            Ty::Function {
                params,
                ret,
                throws,
                ..
            } => {
                let ps: Vec<String> = params
                    .iter()
                    .map(|param| {
                        let ty = param.ty.render_with(s);
                        match (&param.name, param.mode) {
                            (Some(name), FunctionParamMode::Optional) => format!("{name}?: {ty}"),
                            (Some(name), FunctionParamMode::Required) => format!("{name}: {ty}"),
                            (None, _) => ty,
                        }
                    })
                    .collect();
                format!(
                    "({}) -> {} throws {}",
                    ps.join(", "),
                    ret.render_as_function_result(s),
                    throws.render_with(s),
                )
            }
            Ty::TypeVar(name, _) => s.type_var(name),
            Ty::AssociatedTypeProjection {
                base,
                interface,
                member,
                ..
            } => {
                if let Some(interface) = interface {
                    format!(
                        "({} as {}).{}",
                        base.render_with(s),
                        interface.to_ty().render_with(s),
                        member
                    )
                } else {
                    format!("{}.{}", base.render_with(s), member)
                }
            }
            Ty::Never { .. } => "never".to_string(),
            Ty::Void { .. } => "void".to_string(),
            Ty::BuiltinUnknown { .. } | Ty::Unknown { .. } => "unknown".to_string(),
            Ty::RustType { .. } => "$rust_type".to_string(),
            Ty::Type { .. } => "type".to_string(),
            // Opaque leaf types render as their fixed qualified names; these
            // strings feed canonical dumps and must stay byte-identical.
            Ty::Resource { .. } => "baml.llm.Resource".to_string(),
            Ty::PromptAst { .. } => "baml.llm.PromptAst".to_string(),
            Ty::Error { .. } => "!error".to_string(),
            Ty::Future(value, error, _) => {
                format!("Future<{}, {}>", value.render_with(s), error.render_with(s))
            }
            Ty::WatchAccessor(inner, _) => format!("{}.$watch", inner.render_with(s)),
        }
    }
}

/// The built-in strategy for canonical and user-facing rendering. When
/// `user_facing`, the reserved implicit `user` package is elided and synthetic
/// effect params show as `callback`; otherwise everything renders verbatim (for
/// dumps and identity). Both keep `(evolving)` annotations and `<_>`
/// placeholders. [`Ty::render_canonical`] uses `user_facing = false`;
/// [`Ty::render_user_facing`] uses `true`.
pub struct CanonicalTyRender {
    pub user_facing: bool,
}

impl TyRenderStrategy for CanonicalTyRender {
    fn qtn(&self, qtn: &QualifiedTypeName) -> String {
        qtn.render_dotted(self.user_facing)
    }

    fn type_var(&self, name: &Name) -> String {
        // A synthetic effect parameter (`__effect_param_N`) is an implementation
        // detail of effect-polymorphic callbacks; show it as `callback` in
        // user-facing output.
        if self.user_facing && is_synthetic_effect_param(name) {
            "callback".to_string()
        } else {
            name.to_string()
        }
    }
}

impl Interface {
    /// The interface *existential* type ([`Ty::Interface`]) denoted by this
    /// constraint, with default attributes. The inverse of
    /// [`Ty::as_interface`].
    ///
    /// A constraint may pin only some associated types whereas a fully-specified
    /// existential pins all of them; this lifts the constraint verbatim, so the
    /// result carries exactly the bindings the constraint holds.
    pub fn to_ty(&self) -> Ty {
        Ty::Interface(
            self.name.clone(),
            self.generics.clone(),
            self.associated_types.clone(),
            TyAttr::default(),
        )
    }

    /// Iterate every [`Ty`] the constraint carries: its generic arguments
    /// followed by the type of each associated-type binding (the names are not
    /// yielded). Lets generic `Ty`-walkers descend into a constraint without
    /// re-deriving its shape.
    pub fn tys(&self) -> impl Iterator<Item = &Ty> {
        self.generics
            .iter()
            .chain(self.associated_types.iter().map(|(_, ty)| ty))
    }

    /// Rebuild the constraint with every carried [`Ty`] (generics and
    /// associated-type bindings) mapped through `f`. The name and the binding
    /// names are preserved. The structure-preserving companion to [`tys`](Self::tys),
    /// for substitution/erasure/normalization passes.
    pub fn map_tys(&self, mut f: impl FnMut(&Ty) -> Ty) -> Interface {
        Interface {
            name: self.name.clone(),
            generics: self.generics.iter().map(&mut f).collect(),
            associated_types: self
                .associated_types
                .iter()
                .map(|(name, ty)| (name.clone(), f(ty)))
                .collect(),
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
                let param_strs: Vec<std::string::String> =
                    params.iter().map(|p| p.ty.to_string()).collect();
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
                Some(iface) => write!(f, "({base} as {}).{member}", iface.to_ty()),
                None => write!(f, "{base}.{member}"),
            },
            Ty::Never { .. } => write!(f, "never"),
            Ty::Unknown { .. } => write!(f, "unknown"),
            Ty::Error { .. } => write!(f, "<error>"),
            Ty::EvolvingList(inner, _) => write!(f, "{inner}[]"),
            Ty::EvolvingMap(key, value, _) => write!(f, "map<{key}, {value}>"),
            // Opaque leaf types: render identically to `render_with` so the two
            // renderers never diverge. (`type`/`$rust_type` are keywords; the
            // resource/prompt handles render as their fixed qualified names.)
            Ty::RustType { .. } => write!(f, "$rust_type"),
            Ty::Type { .. } => write!(f, "type"),
            Ty::Resource { .. } => write!(f, "baml.llm.Resource"),
            Ty::PromptAst { .. } => write!(f, "baml.llm.PromptAst"),
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
    fn is_valid_impl_subject_classifies_variants() {
        let qtn = |n: &str| QualifiedTypeName::local(Name::new(n));
        let boxed = |t: Ty| Box::new(t);

        // Concrete user-facing types — valid implementors.
        let valid = [
            ty_int(),
            Ty::class("Foo"),
            Ty::Enum(qtn("Color"), TyAttr::default()),
            Ty::list(ty_int()),
            Ty::Map {
                key: boxed(ty_string()),
                value: boxed(ty_int()),
                attr: TyAttr::default(),
            },
            Ty::Type {
                attr: TyAttr::default(),
            },
            Ty::resource(),
            Ty::prompt_ast(),
            Ty::Never {
                attr: TyAttr::default(),
            },
            Ty::TypeVar(Name::new("T"), TyAttr::default()),
            Ty::AssociatedTypeProjection {
                base: boxed(Ty::TypeVar(Name::new("T"), TyAttr::default())),
                interface: None,
                member: Name::new("Item"),
                attr: TyAttr::default(),
            },
        ];
        for ty in &valid {
            assert!(ty.is_valid_impl_subject(), "{ty:?} should be implementable");
        }

        // Singletons, existentials, opaque/native/compiler-only — not implementors.
        let invalid = [
            Ty::Literal(Literal::Int(1), Freshness::Regular, TyAttr::default()),
            Ty::EnumVariant(qtn("Color"), Name::new("Red"), TyAttr::default()),
            Ty::Interface(qtn("I"), vec![], vec![], TyAttr::default()),
            Ty::union([ty_int(), ty_string()]),
            // `Future` has type args `TyTemplate` can't carry — rejected so a generic
            // `Future<T>` for-type errors rather than baking an undispatchable rule.
            Ty::Future(boxed(ty_int()), boxed(Ty::null()), TyAttr::default()),
            Ty::Function {
                params: vec![],
                ret: boxed(Ty::null()),
                throws: boxed(Ty::null()),
                attr: TyAttr::default(),
            },
            Ty::RustType {
                attr: TyAttr::default(),
            },
            Ty::WatchAccessor(boxed(ty_int()), TyAttr::default()),
            Ty::TypeAlias(qtn("A"), TyAttr::default()),
            Ty::Void {
                attr: TyAttr::default(),
            },
            Ty::BuiltinUnknown {
                attr: TyAttr::default(),
            },
            Ty::unknown(),
            Ty::Error {
                attr: TyAttr::default(),
            },
            Ty::EvolvingList(boxed(ty_int()), TyAttr::default()),
        ];
        for ty in &invalid {
            assert!(
                !ty.is_valid_impl_subject(),
                "{ty:?} should not be implementable"
            );
        }
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
    fn test_opaque_constructors_build_concrete_variants() {
        assert!(matches!(Ty::resource(), Ty::Resource { .. }));
        assert!(matches!(Ty::prompt_ast(), Ty::PromptAst { .. }));
        assert!(matches!(Ty::type_type(), Ty::Type { .. }));
        assert!(!matches!(Ty::resource(), Ty::Type { .. }));
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
            params: vec![FunctionParamTy::required(None, ty_int())],
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
                params: vec![FunctionParamTy::required(None, ty_int())],
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
            params: vec![FunctionParamTy::required(None, ty_int())],
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

// Copied from crates/baml_type/src/family.rs: the `RealizedTy` and `TyTemplate` members of
// the family, written out by hand where the original generates them with `ty_family!`, with
// layouts the compiler can write. A change to either file must be made to both.

//! The `Ty` type family: of its members, the two the runtime holds.
//!
//! In the original, [`ty_family!`](baml_type_macros::ty_family) expands the
//! master `Ty` enum into the whole family. Membership is by axis: each
//! variant is tagged with exactly one `#[axis(..)]`, and each member includes
//! a set of axes. Here the two members are written out: [`RealizedTy`]
//! (`concrete, abstract, literal, never, special`) and [`TyTemplate`] (those
//! and `projection, frame`), each with the master's discriminants, so a
//! variant has the same tag in both.
//!
//! Where the original's members are parameterized by `N`, the *nominal type
//! head*, these hold the address of the object a head names: a class or an
//! enum's class object, an interface object, an enum variant's object, the
//! type object a recursive alias stands for. A member name (an associated
//! type, an enum variant) is a position or an object, never a string.

use std::ptr::NonNull;

use super::{
    AssocBinding, AssocIndex, ClassObject, CowBox, CowSlice, EnumVariantObject, Freshness,
    FunctionParamMode, InterfaceBound, InterfaceObject, Literal, MediaKind, Text, TypeObject,
};

/// The satellite `FunctionParamTy` of every member, over the member `T`.
#[repr(C)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionParamTy<T> {
    pub name: Option<Text>,
    pub ty: T,
    pub mode: FunctionParamMode,
}

/// A type with nothing left to fill in: the `RealizedTy` member, whose axes
/// are `concrete, abstract, literal, never, special`. Where the original has
/// a name, the address of the object it names.
#[repr(C, u8)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RealizedTy {
    Int = 0,
    Bigint = 1,
    Float = 2,
    String = 3,
    Bool = 4,
    Null = 5,
    Uint8Array = 6,
    Media(MediaKind) = 7,
    /// A literal type — a single value (`1`, `"hi"`, `true`) as a type. The
    /// [`Freshness`] flag is compiler-only (fresh literals widen at mutable
    /// binding sites); it is normalized to `Regular` at the runtime boundary.
    Literal(Literal, Freshness) = 8,
    Class(NonNull<ClassObject>, CowSlice<RealizedTy>) = 9,
    /// An interface existential type, equivalent to Rust `dyn Trait`.
    /// Must specify all generic type args and all associated types.
    Interface(
        NonNull<InterfaceObject>,
        CowSlice<RealizedTy>,
        CowSlice<AssocBinding<RealizedTy>>,
    ) = 10,
    Enum(NonNull<ClassObject>) = 11,
    /// A specific enum variant — `Status.HttpError`.
    EnumVariant(NonNull<EnumVariantObject>) = 12,
    List(CowBox<RealizedTy>) = 13,
    Map {
        key: CowBox<RealizedTy>,
        value: CowBox<RealizedTy>,
    } = 14,
    Union(CowSlice<RealizedTy>) = 15,

    /// Function/arrow type: `(T1, T2, ...) -> R throws E`.
    Function {
        params: CowSlice<FunctionParamTy<RealizedTy>>,
        ret: CowBox<RealizedTy>,
        throws: CowBox<RealizedTy>,
    } = 16,
    /// A future handle — the result of `schedule_future` or `spawn`
    /// before `await`.
    ///
    /// Carries both the value type the future resolves to and the error
    /// type the future may throw. A future whose body statically cannot
    /// throw has error type `never`.
    Future(CowBox<RealizedTy>, CowBox<RealizedTy>) = 17,
    /// Opaque Rust-managed state (`$rust_type` fields in builtin class stubs,
    /// e.g. `Media._data`). A leaf concrete type with no inner structure.
    ///
    /// Renders as `$rust_type` (qualified name `baml.rust.RustType`).
    RustType = 18,
    /// The `reflect.Type` metatype — a runtime value that wraps a `Ty`
    /// (reflection). A leaf concrete type.
    Type = 19,
    /// Opaque resource handle — file, socket, or HTTP response body. A leaf
    /// concrete type whose *values* are concrete Rust types on the VM heap; the
    /// type system treats it nominally (no structural decomposition).
    ///
    /// Renders as its qualified name `ai.Resource`.
    Resource = 20,
    /// Opaque structured prompt tree for LLM calls. A leaf concrete type whose
    /// *values* are concrete Rust types on the VM heap; the type system treats
    /// it nominally (no structural decomposition).
    ///
    /// Renders as its qualified name `ai.Prompt`.
    PromptAst = 21,

    // reserved: 22 was `Void`, a second unit type beside `Null`. BAML has
    // one unit type (`null`); `void` is a source spelling of it.
    // reserved = 23
    /// Only recursive aliases survive lower_ty; non-recursive are expanded.
    TypeAlias(NonNull<TypeObject>) = 24,
    /// The top type - may have any concrete value.
    ///
    /// Similar to TypeScript's `unknown` - any value can be passed where
    /// `Unknown` is expected, but `Unknown` cannot be used
    /// where a specific type is required.
    ///
    /// Used in llm.baml for functions like:
    /// ```baml
    /// function render_prompt(function_name: string, args: map<string, unknown>) -> PromptAst
    /// ```
    Unknown = 27,
    /// The bottom type — an expression that never produces a value (`return`,
    /// `break`, `continue`, diverging blocks). A subtype of every type.
    Never = 28,
    Tuple(CowSlice<RealizedTy>) = 38,
}

/// A type with holes for the type arguments of the code it appears in: the
/// `TyTemplate` member, whose axes are `concrete, abstract, literal, never,
/// projection, special, frame`. Every `RealizedTy` variant, over
/// `TyTemplate`, keeps its discriminant here.
#[repr(C, u8)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TyTemplate {
    Int = 0,
    Bigint = 1,
    Float = 2,
    String = 3,
    Bool = 4,
    Null = 5,
    Uint8Array = 6,
    Media(MediaKind) = 7,
    /// A literal type — a single value (`1`, `"hi"`, `true`) as a type. The
    /// [`Freshness`] flag is compiler-only (fresh literals widen at mutable
    /// binding sites); it is normalized to `Regular` at the runtime boundary.
    Literal(Literal, Freshness) = 8,
    Class(NonNull<ClassObject>, CowSlice<TyTemplate>) = 9,
    /// An interface existential type, equivalent to Rust `dyn Trait`.
    /// Must specify all generic type args and all associated types.
    Interface(
        NonNull<InterfaceObject>,
        CowSlice<TyTemplate>,
        CowSlice<AssocBinding<TyTemplate>>,
    ) = 10,
    Enum(NonNull<ClassObject>) = 11,
    /// A specific enum variant — `Status.HttpError`.
    EnumVariant(NonNull<EnumVariantObject>) = 12,
    List(CowBox<TyTemplate>) = 13,
    Map {
        key: CowBox<TyTemplate>,
        value: CowBox<TyTemplate>,
    } = 14,
    Union(CowSlice<TyTemplate>) = 15,

    /// Function/arrow type: `(T1, T2, ...) -> R throws E`.
    Function {
        params: CowSlice<FunctionParamTy<TyTemplate>>,
        ret: CowBox<TyTemplate>,
        throws: CowBox<TyTemplate>,
    } = 16,
    /// A future handle — the result of `schedule_future` or `spawn`
    /// before `await`.
    ///
    /// Carries both the value type the future resolves to and the error
    /// type the future may throw. A future whose body statically cannot
    /// throw has error type `never`.
    Future(CowBox<TyTemplate>, CowBox<TyTemplate>) = 17,
    /// Opaque Rust-managed state (`$rust_type` fields in builtin class stubs,
    /// e.g. `Media._data`). A leaf concrete type with no inner structure.
    ///
    /// Renders as `$rust_type` (qualified name `baml.rust.RustType`).
    RustType = 18,
    /// The `reflect.Type` metatype — a runtime value that wraps a `Ty`
    /// (reflection). A leaf concrete type.
    Type = 19,
    /// Opaque resource handle — file, socket, or HTTP response body. A leaf
    /// concrete type whose *values* are concrete Rust types on the VM heap; the
    /// type system treats it nominally (no structural decomposition).
    ///
    /// Renders as its qualified name `ai.Resource`.
    Resource = 20,
    /// Opaque structured prompt tree for LLM calls. A leaf concrete type whose
    /// *values* are concrete Rust types on the VM heap; the type system treats
    /// it nominally (no structural decomposition).
    ///
    /// Renders as its qualified name `ai.Prompt`.
    PromptAst = 21,

    // reserved: 22 was `Void`, a second unit type beside `Null`. BAML has
    // one unit type (`null`); `void` is a source spelling of it.
    // reserved = 23
    /// Only recursive aliases survive lower_ty; non-recursive are expanded.
    TypeAlias(NonNull<TypeObject>) = 24,
    /// Associated type projection, e.g. `P.Output` or `(T as Iterator).Item`. Bound
    /// during inference; can survive at runtime only inside reflective generic
    /// metadata. Split into its own `projection` axis (distinct from `typevar`)
    /// so a template can carry an unresolved projection without also admitting
    /// a name-based `TypeVar`.
    AssociatedTypeProjection {
        base: CowBox<TyTemplate>,
        /// The declaring interface of this projection — always known: the TIR
        /// resolves `(base as I).member` to its interface `I` (or lowers to
        /// `Ty::Error` when it cannot be determined), so a resolved projection
        /// never lacks its qualifier. This is what lets a realized-base
        /// projection reduce to the impl's binding at substitution time.
        interface: CowBox<InterfaceBound>,
        member: AssocIndex,
    } = 26,
    /// The top type - may have any concrete value.
    ///
    /// Similar to TypeScript's `unknown` - any value can be passed where
    /// `Unknown` is expected, but `Unknown` cannot be used
    /// where a specific type is required.
    ///
    /// Used in llm.baml for functions like:
    /// ```baml
    /// function render_prompt(function_name: string, args: map<string, unknown>) -> PromptAst
    /// ```
    Unknown = 27,
    /// The bottom type — an expression that never produces a value (`return`,
    /// `break`, `continue`, diverging blocks). A subtype of every type.
    Never = 28,

    // --- Template-only: a positional reference into an enclosing frame's
    // type arguments, present only in `TyTemplate` (the `frame` axis). It
    // always materializes to exactly one type.
    /// A De Bruijn reference to the n-th type argument of the enclosing
    /// call frame. Materialized to a concrete type by `TyTemplate::substitute`
    /// against the frame's `type_args`; the template-space replacement for a
    /// name-based `TypeVar`.
    TypeArgRef(u32) = 34,
    Tuple(CowSlice<TyTemplate>) = 38,
}

#[cfg(test)]
mod tests {
    use super::{RealizedTy, TyTemplate};

    /// The leading byte of a `#[repr(C, u8)]` value is its discriminant. Reading
    /// it directly lets us assert a logical variant carries the same *in-memory*
    /// tag in every member — the premise the zero-cost `transmute` upcasts rest
    /// on. Borsh uses these same explicit discriminants for its wire tags.
    fn in_memory_tag<T>(v: &T) -> u8 {
        // SAFETY: every family member is `#[repr(C, u8)]`, so its first byte is
        // the `u8` discriminant.
        unsafe { *std::ptr::from_ref(v).cast::<u8>() }
    }

    #[test]
    fn in_memory_discriminants_are_consistent_across_members() {
        // `Unknown` is master variant #27; `RealizedTy` drops the
        // `typevar` and `projection` variants before it, yet its tag stays 27.
        assert_eq!(in_memory_tag::<RealizedTy>(&RealizedTy::Unknown), 27);
        assert_eq!(in_memory_tag::<TyTemplate>(&TyTemplate::Unknown), 27);
        // `Never` (#28) is shared and tag-stable across the deep members.
        assert_eq!(in_memory_tag::<RealizedTy>(&RealizedTy::Never), 28);
        assert_eq!(in_memory_tag::<TyTemplate>(&TyTemplate::Never), 28);
        // A leaf concrete variant present in every member.
        assert_eq!(in_memory_tag::<RealizedTy>(&RealizedTy::Int), 0);
        assert_eq!(in_memory_tag::<TyTemplate>(&TyTemplate::Int), 0);
        // The template-only frame leaf keeps its master tag.
        assert_eq!(in_memory_tag::<TyTemplate>(&TyTemplate::TypeArgRef(0)), 34);
    }
}

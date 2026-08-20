//! Compile-time constant values ([`ConstValue`]).

use borsh::{BorshDeserialize, BorshSerialize};

use crate::{heap_ptr::HeapPtr, types::Value};

/// Compile-time constant values.
///
/// Similar to `Value` but uses `ObjectIndex` for object references instead of `HeapPtr`.
/// Used in bytecode constants which are converted to `Value` when loading into the engine.
///
/// Note: `ConstValue::Type` is intentionally excluded from the `to_value` conversion — the
/// `LoadType` instruction reads the `TyTemplate` directly from the constant pool at execution
/// time and substitutes type arguments from `frame.type_args` before allocating an
/// `Object::Type` on the heap.
#[derive(Clone, Debug, PartialEq, BorshSerialize, BorshDeserialize)]
pub enum ConstValue {
    OmittedArg,
    Null,
    Int(i64),
    Float(f64),
    Bool(bool),
    /// Index into the object pool (converted to `HeapPtr` at load time).
    Object(crate::ObjectIndex),
    /// A type template for use by the `LoadType` instruction.
    ///
    /// Unlike the other variants, this constant is **not** pre-resolved at load
    /// time — `LoadType` reads the template here and performs substitution at
    /// runtime (using the current frame's `type_args`).
    Type(crate::TyTemplate),
    /// A parametric-class `IsType` check constant.
    ///
    /// Used by `Instruction::IsType` when the expected type is a generic class
    /// instantiation (e.g. `Foo<int>` or `Foo<T>`).  Like `ConstValue::Type`,
    /// this constant is **not** pre-resolved: the `IsType` VM dispatch reads it
    /// directly from the raw constant pool and resolves the `class_obj` index to
    /// a `HeapPtr` at execution time.
    ClassWithTypeArgs {
        /// Compile-time index of the class object in the object pool.
        class_obj: crate::ObjectIndex,
        /// Complete templates for the class-level type args, in De Bruijn
        /// order. `TypeArgRef(n)` refers to `frame.type_args[n]`; each
        /// position denotes exactly one type per frame (`TyTemplate` carries
        /// no match-any holes).
        type_args_templates: Vec<crate::TyTemplate>,
    },
    /// A singleton-type `IsType` check constant: membership in the literal
    /// type `1`, `"go"`, `true`, `1n`.
    ///
    /// Membership in a singleton is decided by the value's *own* identity, so
    /// unlike every other `IsType` constant this one is compared against the
    /// value rather than against a type the value reconstructs. It is a
    /// specialization of `ConstValue::Type(TyTemplate::Literal(..))`, which
    /// decides the same question through the canonical algebra: the two must
    /// agree, and `literal_membership_agrees_with_algebra` in `type_match`
    /// pins that.
    ///
    /// Like `Type` and `ClassWithTypeArgs`, this constant is **not**
    /// pre-resolved at load time — the `IsType` dispatch reads it straight from
    /// the raw constant pool.
    Literal(baml_base::Literal),
}

impl ConstValue {
    /// Convert to a runtime `Value` using a function to resolve object indices to heap pointers.
    ///
    /// # Panics
    ///
    /// Panics if called on `ConstValue::Type` — type-template constants are
    /// handled at runtime by the `LoadType` instruction, not pre-resolved at
    /// load time.
    pub fn to_value<F>(&self, resolve: F) -> Value
    where
        F: Fn(crate::ObjectIndex) -> HeapPtr,
    {
        match self {
            ConstValue::OmittedArg => Value::OMITTED_ARG,
            ConstValue::Null => Value::NULL,
            ConstValue::Int(v) => Value::int(*v),
            ConstValue::Float(_) => panic!(
                "ConstValue::Float must be heap-boxed at engine load time — \
                 use the float-allocating conversion path, not to_value"
            ),
            ConstValue::Bool(v) => Value::bool(*v),
            ConstValue::Object(idx) => Value::object(resolve(*idx)),
            ConstValue::Type(_) => {
                panic!(
                    "ConstValue::Type must not be pre-resolved via to_value — \
                     use the LoadType instruction instead"
                )
            }
            ConstValue::ClassWithTypeArgs { .. } => {
                panic!(
                    "ConstValue::ClassWithTypeArgs must not be pre-resolved via to_value — \
                     use the IsType instruction instead"
                )
            }
            ConstValue::Literal(_) => {
                panic!(
                    "ConstValue::Literal must not be pre-resolved via to_value — \
                     use the IsType instruction instead"
                )
            }
        }
    }
}

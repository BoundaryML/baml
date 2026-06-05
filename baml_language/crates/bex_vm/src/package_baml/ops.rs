//! Native implementations of the `baml.ops` comparison interfaces
//! (`Equals` / `Compare`) for primitives and containers, declared in
//! `baml_std/baml/ns_ops/comparison.baml`.
//!
//! These mirror BAML's `==` / `<` / `>` / `<=` / `>=` operators (which the
//! compiler usually special-cases to direct comparison bytecode). They exist
//! so primitives and containers satisfy interface bounds (`T extends Compare`)
//! and so a comparison reached via dynamic dispatch produces the *same* result
//! the specialized bytecode would.
//!
//! Floats compare by IEEE rules (so `NaN != NaN`), matching the `==` operator
//! and deliberately *unlike* `baml.deep_equals`, whose NaN-equal convention is
//! a test-helper nicety rather than the language's equality.

use std::{collections::HashMap, sync::Arc};

use bex_str::BexStr;
use bex_vm_types::{
    HeapPtr, ValueKind,
    types::{Object, Value},
};
use indexmap::IndexMap;
use num_bigint::BigInt;

use super::{
    BamlClassOpsCompare_for_bigint, BamlClassOpsCompare_for_float, BamlClassOpsCompare_for_int,
    BamlClassOpsCompare_for_string, BamlClassOpsEquals_for_T__, BamlClassOpsEquals_for_bigint,
    BamlClassOpsEquals_for_bool, BamlClassOpsEquals_for_float, BamlClassOpsEquals_for_int,
    BamlClassOpsEquals_for_map_K__V_, BamlClassOpsEquals_for_string,
    BamlClassOpsEquals_for_uint8array, BamlNamespaceOps, PackageBamlImpl,
};
use crate::BexVm;

// ── int ───────────────────────────────────────────────────────────────────

impl BamlClassOpsEquals_for_int for PackageBamlImpl {
    fn eq(int: i64, other: i64) -> bool {
        int == other
    }
}

impl BamlClassOpsCompare_for_int for PackageBamlImpl {
    fn lt(int: i64, other: i64) -> bool {
        int < other
    }
}

// ── bigint ─────────────────────────────────────────────────────────────────

impl BamlClassOpsEquals_for_bigint for PackageBamlImpl {
    fn eq(bigint: Arc<BigInt>, other: Arc<BigInt>) -> bool {
        // `Arc<T>: PartialEq` compares the pointed-to values, so two distinct
        // allocations holding the same integer compare equal.
        bigint == other
    }
}

impl BamlClassOpsCompare_for_bigint for PackageBamlImpl {
    fn lt(bigint: Arc<BigInt>, other: Arc<BigInt>) -> bool {
        bigint < other
    }
}

// ── float ────────────────────────────────────────────────────────────────

impl BamlClassOpsEquals_for_float for PackageBamlImpl {
    #[allow(clippy::float_cmp)] // IEEE equality on purpose; see module docs.
    fn eq(float: f64, other: f64) -> bool {
        float == other
    }
}

impl BamlClassOpsCompare_for_float for PackageBamlImpl {
    // All four are direct IEEE comparisons rather than the interface's
    // boolean-derived defaults (`gt = !le`, etc.): with NaN those defaults
    // would wrongly report `gt`/`ge` as `true`, whereas IEEE `>`/`>=` are
    // `false` for any NaN operand, matching the `==`/`<` operators.
    fn lt(float: f64, other: f64) -> bool {
        float < other
    }

    fn gt(float: f64, other: f64) -> bool {
        float > other
    }

    fn ge(float: f64, other: f64) -> bool {
        float >= other
    }

    fn le(float: f64, other: f64) -> bool {
        float <= other
    }
}

// ── bool ─────────────────────────────────────────────────────────────────

impl BamlClassOpsEquals_for_bool for PackageBamlImpl {
    // The receiver arrives as a raw `&Value` (the codegen has no dedicated
    // `bool` receiver shape); `self: bool` guarantees it is a Bool, so a
    // non-bool falls through to `false`.
    fn eq(bool: &Value, other: bool) -> bool {
        bool.as_bool() == Some(other)
    }
}

// ── string ─────────────────────────────────────────────────────────────────

impl BamlClassOpsEquals_for_string for PackageBamlImpl {
    fn eq(string: &BexStr, other: &BexStr) -> bool {
        string == other
    }
}

impl BamlClassOpsCompare_for_string for PackageBamlImpl {
    // Lexicographic order (Unicode code unit order), as documented in
    // `comparison.baml`.
    fn lt(string: &BexStr, other: &BexStr) -> bool {
        string < other
    }

    fn gt(string: &BexStr, other: &BexStr) -> bool {
        string > other
    }

    fn ge(string: &BexStr, other: &BexStr) -> bool {
        string >= other
    }

    fn le(string: &BexStr, other: &BexStr) -> bool {
        string <= other
    }
}

// ── uint8array ─────────────────────────────────────────────────────────────

impl BamlClassOpsEquals_for_uint8array for PackageBamlImpl {
    fn eq(uint8array: &[u8], other: &[u8]) -> bool {
        uint8array == other
    }
}

// ── containers ─────────────────────────────────────────────────────────────

impl BamlClassOpsEquals_for_T__ for PackageBamlImpl {
    fn eq(vm: &BexVm, array: &[Value], other: &[Value]) -> bool {
        if array.len() != other.len() {
            return false;
        }
        let mut visited = HashMap::new();
        array
            .iter()
            .zip(other.iter())
            .all(|(a, b)| ops_value_eq(vm, *a, *b, &mut visited))
    }
}

impl BamlClassOpsEquals_for_map_K__V_ for PackageBamlImpl {
    fn eq(vm: &BexVm, map: &IndexMap<BexStr, Value>, other: &IndexMap<BexStr, Value>) -> bool {
        if map.len() != other.len() {
            return false;
        }
        // Order-insensitive: maps are equal when they have the same keys and
        // the value at each key is equal.
        let mut visited = HashMap::new();
        map.iter().all(|(key, value)| {
            other
                .get(key)
                .is_some_and(|other_value| ops_value_eq(vm, *value, *other_value, &mut visited))
        })
    }
}

impl BamlNamespaceOps for PackageBamlImpl {}

/// Recursive value equality used by the container `eq` impls.
///
/// This is a faithful, recursing extension of the `==` operator
/// (`BexVm::exec_cmpop`): scalars and leaf heap types (string, bigint,
/// uint8array, enum variant, type) compare by value exactly as `==` does, and
/// nested arrays/maps recurse element-wise (which the `T extends Equals` bound
/// guarantees is well-defined). It deliberately differs from
/// [`BamlPackageBaml::deep_equals`](super::BamlPackageBaml::deep_equals) in two
/// ways, both for consistency with `==`/`float.eq`:
///
/// - floats use IEEE equality (`NaN != NaN`), not the NaN-equal convention;
/// - class instances (and any other heap object) compare by reference rather
///   than structurally. The native container `eq` cannot dispatch a
///   user-defined `Equals` impl on an element class, so it preserves `==`'s
///   identity semantics instead of guessing a structural comparison.
///
/// The `visited` map breaks reference cycles co-inductively (a pair assumed
/// equal while it is still being compared), matching `deep_equals`.
#[allow(clippy::float_cmp)] // IEEE float equality on purpose; see module docs.
fn ops_value_eq(
    vm: &BexVm,
    a: Value,
    b: Value,
    visited: &mut HashMap<(HeapPtr, HeapPtr), bool>,
) -> bool {
    match (a.kind(), b.kind()) {
        (ValueKind::Null, ValueKind::Null) => true,
        (ValueKind::OmittedArg, ValueKind::OmittedArg) => true,
        (ValueKind::Int(a), ValueKind::Int(b)) => a == b,
        (ValueKind::Bool(a), ValueKind::Bool(b)) => a == b,

        (ValueKind::Object(a_ptr), ValueKind::Object(b_ptr)) => {
            if a_ptr == b_ptr {
                return true;
            }

            let key = if a_ptr < b_ptr {
                (a_ptr, b_ptr)
            } else {
                (b_ptr, a_ptr)
            };
            if let Some(&result) = visited.get(&key) {
                return result;
            }
            // Assume equal while recursing so a cycle terminates instead of
            // looping forever; overwritten with the real verdict below.
            visited.insert(key, true);

            let result = match (vm.get_object(a_ptr), vm.get_object(b_ptr)) {
                (Object::Float(a), Object::Float(b)) => a == b,
                (Object::String(a), Object::String(b)) => a == b,
                // Different `Arc`s with the same numeric value compare equal.
                (Object::Bigint(a), Object::Bigint(b)) => a == b,
                (Object::Uint8Array(a), Object::Uint8Array(b)) => a.to_vec() == b.to_vec(),

                (Object::Array(a_values), Object::Array(b_values)) => {
                    // Snapshot under each lock before recursing: the container
                    // lock is a non-reentrant exclusive spin-lock, so it must
                    // be released before a recursive lookup that may lock
                    // another (or the same) container.
                    let a_snap = a_values.to_vec();
                    let b_snap = b_values.to_vec();
                    a_snap.len() == b_snap.len()
                        && a_snap
                            .iter()
                            .zip(b_snap.iter())
                            .all(|(a, b)| ops_value_eq(vm, *a, *b, visited))
                }

                (Object::Map(a_map), Object::Map(b_map)) => {
                    let a_snap = a_map.to_index_map();
                    let b_snap = b_map.to_index_map();
                    a_snap.len() == b_snap.len()
                        && a_snap.iter().all(|(key, a_val)| {
                            b_snap
                                .get(key)
                                .is_some_and(|b_val| ops_value_eq(vm, *a_val, *b_val, visited))
                        })
                }

                (Object::Variant(a_var), Object::Variant(b_var)) => {
                    a_var.enm == b_var.enm && a_var.index == b_var.index
                }

                (Object::Type(a_ty), Object::Type(b_ty)) => a_ty == b_ty,

                // Class instances and every other heap object: compare by
                // reference. The equal-pointer case already returned `true`
                // above, so distinct pointers here are unequal.
                _ => false,
            };

            visited.insert(key, result);
            result
        }

        _ => false,
    }
}

//! Operator dispatch through interfaces (README decision 3B): `a + b` IS
//! `Implements(lhs, baml.ops.Add<rhs>)` with the impl's `Output` as the
//! result - the interface registry is the sole source of operator validity,
//! exactly as TIR's arithmetic arm already works. Rewriting primitive cases
//! to single instructions is MIR's job at lowering, invisible to inference.
//!
//! This registry is the operator-shaped seed of the full impl registry (I1):
//! it reads only monomorphic free impls (`implement Add<int> for int`),
//! which covers every `baml.ops` operator impl the stdlib defines. Blanket
//! impls (`implement<T> Equals for T[]`), in-class implements blocks, and
//! non-operator interfaces join when I1 generalizes it; at S3 it becomes a
//! salsa query instead of a per-inference-run build.

use baml_type::{
    Name,
    interned::{Ty, TyKind},
};

use crate::lower::lower_ctx_for_file;

/// The `baml.ops` interfaces operators dispatch through. Bitwise operators
/// are ABSENT: the stdlib has no interfaces for them yet, so they type
/// through the hack table in `infer.rs` until `ns_ops` grows them.
const OPERATOR_INTERFACES: [&str; 6] = [
    "Add",
    "Subtract",
    "Multiply",
    "Divide",
    "Remainder",
    "Negate",
];

/// One monomorphic operator impl: `implement <interface><rhs> for <for_ty>`
/// with its bound `Output`.
struct OperatorImplEntry {
    interface: Name,
    for_ty: Ty,
    /// The interface's type argument; `None` for unary interfaces
    /// (`Negate`).
    rhs: Option<Ty>,
    output: Ty,
}

/// Every operator impl visible in the project (stdlib included), keyed for
/// exact-type lookup: callers widen literals to their bases first, so
/// `1 + 2.5` looks up `(int, float)` and finds `Output = float`.
pub struct OperatorRegistry {
    entries: Vec<OperatorImplEntry>,
}

impl OperatorRegistry {
    pub fn build(db: &dyn baml_compiler2_ppir::Db) -> OperatorRegistry {
        let mut entries = Vec::new();
        for file in baml_compiler2_hir::compiler2_all_files(db) {
            let impls = baml_compiler2_ppir::item_data::file_impls(db, file);
            if impls.is_empty() {
                continue;
            }
            let lower = lower_ctx_for_file(db, file);
            for impl_loc in impls {
                let data = baml_compiler2_ppir::item_data::impl_block_data(db, *impl_loc);
                let baml_compiler2_ppir::item_data::ImplSubjectData::Free {
                    for_target,
                    generics,
                } = &data.subject
                else {
                    continue;
                };
                if !generics.is_empty() {
                    continue;
                }
                let interface_ty = lower.lower_type_ref(&data.type_refs, data.interface_target);
                let TyKind::Interface(qtn, args, _, _) = interface_ty.kind() else {
                    continue;
                };
                if qtn.package().as_str() != "baml"
                    || qtn.namespace().len() != 1
                    || qtn.namespace()[0].as_str() != "ops"
                    || !OPERATOR_INTERFACES.contains(&qtn.name().as_str())
                {
                    continue;
                }
                let Some(output_ref) = data
                    .associated_type_bindings
                    .iter()
                    .find(|binding| binding.name.as_str() == "Output")
                    .and_then(|binding| binding.type_ref)
                else {
                    continue;
                };
                let for_ty = lower.lower_type_ref(&data.type_refs, *for_target);
                let output = lower.lower_type_ref(&data.type_refs, output_ref);
                if for_ty.has_error() || output.has_error() {
                    continue;
                }
                entries.push(OperatorImplEntry {
                    interface: qtn.name().clone(),
                    for_ty,
                    rhs: args.first().cloned(),
                    output,
                });
            }
        }
        OperatorRegistry { entries }
    }

    /// The `Output` of the unique impl matching `<interface><rhs> for
    /// <lhs>`, or `None` when no impl exists (the operands do not support
    /// the operator). Exact interned equality: operands must be widened to
    /// their bases by the caller.
    pub fn output(&self, interface: &str, lhs: &Ty, rhs: Option<&Ty>) -> Option<Ty> {
        self.entries
            .iter()
            .find(|entry| {
                entry.interface.as_str() == interface
                    && entry.for_ty == *lhs
                    && entry.rhs.as_ref() == rhs
            })
            .map(|entry| entry.output.clone())
    }
}

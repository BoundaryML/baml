//! Class literals allocated in place by compiled code.
//!
//! Replaces, for a literal of a class without type arguments, the
//! `vm.new_instance(class, vec![..])` call, which built a `Vec` of tagged
//! values (boxing every float) for the VM to copy into the instance: the
//! literal takes a cell from the thread's allocator (`BexVm::alloc_shaped`)
//! and stores each field at its layout offset in its own representation.
//! A class whose instances need more (a finalizer) takes the VM's path.

use std::fmt::Write as _;

use baml_compiler2_mir::{BlockId, Operand};
use bex_vm_types::layout::{RawKind, SlotKind};

use super::{Expr, Gen, Generator};
use crate::{analysis::Repr, render::coerce};

impl<'db> Generator<'_, 'db> {
    /// A literal of the class at `object` with `fields` in declaration order.
    pub(super) fn new_instance(
        &mut self,
        block: BlockId,
        object: usize,
        fields: &[Operand<'db>],
    ) -> Gen<Expr> {
        let values = fields
            .iter()
            .map(|field| self.operand(block, field))
            .collect::<Gen<Vec<Expr>>>()?;
        let class = format!("vm.record_object(fr, ObjectIndex::from_raw({object}))");
        let mut boxed = String::new();
        let mut args = Vec::with_capacity(values.len());
        for (i, value) in values.iter().enumerate() {
            let _ = write!(
                boxed,
                "let v{i} = {}; ",
                coerce(&value.code, value.repr, Repr::Value)
            );
            args.push(format!("v{i}"));
        }
        let generic = format!(
            "{{ {boxed}vm.new_instance({class}, vec![{}]) }}",
            args.join(", ")
        );
        let Some(stores) = self.field_stores(object, &values) else {
            return Ok(Expr::new(generic, Repr::Value));
        };
        Ok(Expr::new(
            format!(
                "(match vm.instance_shape({class}) {{ Some(s) => {{ let (v, f) = vm.alloc_shaped(s); {stores}v }} None => {generic} }})"
            ),
            Repr::Value,
        ))
    }

    /// The stores of `values` into a fresh instance `f` of the class at
    /// `object`, or `None` when a value's representation does not fit its
    /// slot as compiled code stores it.
    fn field_stores(&self, object: usize, values: &[Expr]) -> Option<String> {
        let mut out = String::new();
        for (i, value) in values.iter().enumerate() {
            let slot = self.index.field_slot(object, i)?;
            let offset = slot.offset;
            let store = match (slot.kind, value.repr) {
                (SlotKind::Raw(raw), _) => {
                    let (setter, repr) = match raw {
                        RawKind::Int => ("set_int", Repr::Int),
                        RawKind::Float => ("set_float", Repr::Float),
                        RawKind::Bool => ("set_bool", Repr::Bool),
                    };
                    let value = coerce(&value.code, value.repr, repr);
                    format!("unsafe {{ f.{setter}({offset}, {value}) }}; ")
                }
                (SlotKind::Ptr, Repr::Value) => {
                    format!("unsafe {{ f.set_ptr({offset}, {}) }}; ", value.code)
                }
                (SlotKind::Ptr, _) => return None,
                (SlotKind::Tagged16, _) => {
                    let value = coerce(&value.code, value.repr, Repr::Value);
                    format!(
                        "{{ let t = {value}; if let Err(e) = unsafe {{ vm.store_tagged_at(f, {offset}, t) }} {{ return Err(e); }} }} "
                    )
                }
            };
            out.push_str(&store);
        }
        Some(out)
    }
}

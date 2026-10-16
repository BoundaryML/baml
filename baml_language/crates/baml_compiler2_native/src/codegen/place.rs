//! Operands and places: reading and writing locals, fields, and elements.

use baml_compiler2_mir::{BlockId, Operand, Place};

use super::{Expr, Gen, Generator, unsupported};
use crate::{analysis::Repr, render::coerce};

impl<'db> Generator<'_, 'db> {
    pub(super) fn operand(&mut self, block: BlockId, operand: &Operand<'db>) -> Gen<Expr> {
        let expr = match operand {
            Operand::Copy(place) | Operand::Move(place) => self.load(block, place)?,
            Operand::Constant(constant) => self.constant(constant)?,
        };
        Ok(self.hoist(expr))
    }

    pub(super) fn load(&mut self, block: BlockId, place: &Place) -> Gen<Expr> {
        match place {
            Place::Local(local) => {
                if self.analysis.static_types.contains_key(&local.0) {
                    return unsupported("type value");
                }
                if self.analysis.array_iters.contains_key(&local.0) {
                    return unsupported("array iterator used as a value");
                }
                Ok(Expr::new(format!("l{}", local.0), self.repr(*local)))
            }
            Place::Field { base, field } => {
                let base = self.load(block, base)?;
                let base = self.value_arg(&base.code, base.repr);
                let code = self.fallible(block, &format!("vm.load_field({base}, {field})"))?;
                Ok(Expr::new(code, Repr::Value))
            }
            Place::Index { base, index, kind } => {
                if !matches!(kind, baml_compiler2_mir::IndexKind::Array) {
                    return unsupported("map index");
                }
                let base = self.load(block, base)?;
                let base = self.value_arg(&base.code, base.repr);
                let index = self.value_arg(&format!("l{}", index.0), self.repr(*index));
                let code =
                    self.fallible(block, &format!("vm.load_array_element({base}, {index})"))?;
                Ok(Expr::new(code, Repr::Value))
            }
            Place::Capture(_) | Place::Deref(_) => unsupported("captured local"),
        }
    }

    /// The statement storing `value` into `place`.
    pub(super) fn store(&mut self, block: BlockId, place: &Place, value: &Expr) -> Gen<String> {
        match place {
            Place::Local(local) => {
                let code = coerce(&value.code, value.repr, self.repr(*local));
                if self.suspends && self.write_through.contains(&local.0) {
                    let slot = coerce(&format!("l{}", local.0), self.repr(*local), Repr::Value);
                    return Ok(format!(
                        "l{0} = {code}; vm.set_slot(depth, {0}, {slot});",
                        local.0
                    ));
                }
                Ok(format!("l{} = {code};", local.0))
            }
            Place::Field { base, field } => {
                let value = self.value_arg(&value.code, value.repr);
                let base = self.load(block, base)?;
                let base = self.value_arg(&base.code, base.repr);
                let code =
                    self.fallible(block, &format!("vm.store_field({base}, {field}, {value})"))?;
                Ok(format!("{code};"))
            }
            Place::Index { base, index, kind } => {
                if !matches!(kind, baml_compiler2_mir::IndexKind::Array) {
                    return unsupported("map index");
                }
                let value = self.value_arg(&value.code, value.repr);
                let base = self.load(block, base)?;
                let base = self.value_arg(&base.code, base.repr);
                let index = self.value_arg(&format!("l{}", index.0), self.repr(*index));
                let code = self.fallible(
                    block,
                    &format!("vm.store_array_element({base}, {index}, {value})"),
                )?;
                Ok(format!("{code};"))
            }
            Place::Capture(_) | Place::Deref(_) => unsupported("captured local"),
        }
    }
}

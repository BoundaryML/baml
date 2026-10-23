//! Reads and writes locals, fields, elements, and captured cells.
//! Captured locals hold cells; dereferences access their values.

use baml_compiler2_mir::{BlockId, CellId, Operand, Place};

use super::{Expr, Gen, Generator, unsupported};
use crate::{admit::Rejection, analysis::Repr, render::coerce};

impl<'db> Generator<'_, 'db> {
    pub(super) fn operand(&mut self, block: BlockId, operand: &Operand<'db>) -> Gen<Expr> {
        let expr = match operand {
            Operand::Copy(place) | Operand::Move(place) => self.load(block, place)?,
            Operand::Constant(constant) => self.constant(block, constant)?,
        };
        Ok(self.hoist(expr))
    }

    pub(super) fn load(&mut self, block: BlockId, place: &Place) -> Gen<Expr> {
        match place {
            Place::Local(local) => {
                // Frame-derived types are rebuilt per read; static types are cached per site.
                if let Some(template) = self.analysis.deferred_types.get(&local.0) {
                    return self.type_value(template);
                }
                if let Some(ty) = self.analysis.static_types.get(&local.0) {
                    let code = ty.code.clone();
                    let site = match self.static_value_sites.iter().position(|c| *c == code) {
                        Some(site) => site,
                        None => {
                            self.static_value_sites.push(code.clone());
                            self.static_value_sites.len() - 1
                        }
                    };
                    return Ok(Expr::new(
                        format!("vm.static_type_value(fr, {site}, {code})"),
                        Repr::Value,
                    ));
                }
                // The analysis keeps only iterators the loop alone consumes.
                if self.analysis.array_iters.contains(&local.0) {
                    return Err(Rejection::Invalid("array iterator used as a value".into()));
                }
                Ok(Expr::atom(format!("l{}", local.0), self.repr(*local)))
            }
            Place::Field { base, field } => {
                let load = if self.analysis.escape.object_is_thread_local(base) {
                    "load_field_local"
                } else {
                    "load_field"
                };
                let base = self.load(block, base)?;
                let base = self.value_arg(&base);
                let code = self.fallible(block, &format!("vm.{load}({base}, {field})"))?;
                Ok(Expr::new(code, Repr::Value))
            }
            Place::Index { base, index, kind } => {
                if !matches!(kind, baml_compiler2_mir::IndexKind::Array) {
                    return unsupported("map index");
                }
                let load = if self.analysis.escape.object_is_thread_local(base) {
                    "load_array_element_local"
                } else {
                    "load_array_element"
                };
                let base = self.load(block, base)?;
                let base = self.value_arg(&base);
                let index = self.value_arg(&Expr::atom(format!("l{}", index.0), self.repr(*index)));
                let code = self.fallible(block, &format!("vm.{load}({base}, {index})"))?;
                Ok(Expr::new(code, Repr::Value))
            }
            Place::Capture(index) => Ok(Expr::new(
                format!("vm.capture_cell(fr, {index})?"),
                Repr::Value,
            )),
            Place::Deref(cell) => {
                let (cell, repr) = self.cell(cell);
                let code = coerce(&format!("vm.cell_load({cell})?"), Repr::Value, repr);
                Ok(Expr::new(code, repr))
            }
        }
    }

    fn cell(&mut self, cell: &CellId) -> (String, Repr) {
        match cell {
            CellId::Local(local) => (format!("l{}", local.0), self.analysis.values[local.0]),
            CellId::Capture(index) => {
                let cell = self.hoist(Expr::new(
                    format!("vm.capture_cell(fr, {index})?"),
                    Repr::Value,
                ));
                let repr = self
                    .analysis
                    .captures
                    .get(*index)
                    .copied()
                    .unwrap_or(Repr::Value);
                (cell.code, repr)
            }
        }
    }

    pub(super) fn store(&mut self, block: BlockId, place: &Place, value: &Expr) -> Gen<String> {
        match place {
            Place::Local(local) => {
                let code = coerce(&value.code, value.repr, self.repr(*local));
                Ok(format!("l{} = {code};", local.0))
            }
            Place::Field { base, field } => {
                let store = if self.analysis.escape.object_is_thread_local(base) {
                    "store_field_local"
                } else {
                    "store_field"
                };
                let value = self.value_arg(value);
                let base = self.load(block, base)?;
                let base = self.value_arg(&base);
                let code =
                    self.fallible(block, &format!("vm.{store}({base}, {field}, {value})"))?;
                Ok(format!("{code};"))
            }
            Place::Index { base, index, kind } => {
                if !matches!(kind, baml_compiler2_mir::IndexKind::Array) {
                    return unsupported("map index");
                }
                let store = if self.analysis.escape.object_is_thread_local(base) {
                    "store_array_element_local"
                } else {
                    "store_array_element"
                };
                let value = self.value_arg(value);
                let base = self.load(block, base)?;
                let base = self.value_arg(&base);
                let index = self.value_arg(&Expr::atom(format!("l{}", index.0), self.repr(*index)));
                let code =
                    self.fallible(block, &format!("vm.{store}({base}, {index}, {value})"))?;
                Ok(format!("{code};"))
            }
            Place::Deref(cell) => {
                let value = self.value_arg(value);
                let (cell, _) = self.cell(cell);
                Ok(format!("vm.cell_store({cell}, {value})?;"))
            }
            Place::Capture(_) => Err(Rejection::Invalid(
                "store into a closure's capture cell".into(),
            )),
        }
    }
}

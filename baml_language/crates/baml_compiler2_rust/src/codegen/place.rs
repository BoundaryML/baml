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
                let needs = self.analysis.escape.object_needs(base);
                let base = self.load(block, base)?;
                let base = self.value_arg(&base);
                let load = Self::by_locality(
                    needs,
                    &format!("vm.load_field_local({base}, {field})"),
                    &format!("vm.load_field({base}, {field})"),
                );
                let code = self.fallible(block, &load)?;
                Ok(Expr::new(code, Repr::Value))
            }
            Place::Index { base, index, kind } => {
                if !matches!(kind, baml_compiler2_mir::IndexKind::Array) {
                    return unsupported("map index");
                }
                let needs = self.analysis.escape.object_needs(base);
                let base = self.load(block, base)?;
                let base = self.value_arg(&base);
                let index = self.value_arg(&Expr::atom(format!("l{}", index.0), self.repr(*index)));
                let load = Self::by_locality(
                    needs,
                    &format!("vm.load_array_element_local({base}, {index})"),
                    &format!("vm.load_array_element({base}, {index})"),
                );
                let code = self.fallible(block, &load)?;
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
                let needs = self.analysis.escape.object_needs(base);
                let value = self.value_arg(value);
                let base = self.load(block, base)?;
                let base = self.value_arg(&base);
                let store = Self::by_locality(
                    needs,
                    &format!("vm.store_field_local({base}, {field}, {value})"),
                    &format!("vm.store_field({base}, {field}, {value})"),
                );
                let code = self.fallible(block, &store)?;
                Ok(format!("{code};"))
            }
            Place::Index { base, index, kind } => {
                if !matches!(kind, baml_compiler2_mir::IndexKind::Array) {
                    return unsupported("map index");
                }
                let needs = self.analysis.escape.object_needs(base);
                let value = self.value_arg(value);
                let base = self.load(block, base)?;
                let base = self.value_arg(&base);
                let index = self.value_arg(&Expr::atom(format!("l{}", index.0), self.repr(*index)));
                let store = Self::by_locality(
                    needs,
                    &format!("vm.store_array_element_local({base}, {index}, {value})"),
                    &format!("vm.store_array_element({base}, {index}, {value})"),
                );
                let code = self.fallible(block, &store)?;
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

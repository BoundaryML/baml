//! Operand spaces: how a pooled object's unit- or tail-convention operands
//! map into the image, and the one relocation walk over them.

use baml_base::Name;

use super::{
    LinkError, PerPackage,
    layout::{Placed, SlotLayout, TailSlots, UnitObjects},
};
use crate::{
    GlobalIndex, Object, ObjectIndex,
    relink::{IndexOperand, visit_object_operands},
    unit::{CompilationUnit, InitTail, import_ordinal},
};

/// Resolve a global operand in unit or tail convention: an import ordinal
/// through `imports`, a local slot through `local`.
pub(super) fn global_operand(
    name: &Name,
    raw: usize,
    imports: &[usize],
    local: impl Fn(usize) -> Option<usize>,
) -> Result<usize, LinkError> {
    match import_ordinal(raw) {
        Some(ordinal) => imports.get(ordinal).copied().ok_or_else(|| {
            LinkError::invalid(format!(
                "package `{name}` references global import {ordinal} of {}",
                imports.len()
            ))
        }),
        None => local(raw).ok_or_else(|| {
            LinkError::invalid(format!(
                "package `{name}` references local global {raw} outside its slots"
            ))
        }),
    }
}

/// Resolve an object operand in unit or tail convention.
fn object_operand(
    name: &Name,
    raw: usize,
    imports: &[usize],
    local: impl Fn(usize) -> Option<usize>,
) -> Result<usize, LinkError> {
    match import_ordinal(raw) {
        Some(ordinal) => imports.get(ordinal).copied().ok_or_else(|| {
            LinkError::invalid(format!(
                "package `{name}` references object import {ordinal} of {}",
                imports.len()
            ))
        }),
        None => local(raw).ok_or_else(|| {
            LinkError::invalid(format!(
                "package `{name}` references local object {raw} outside its buckets"
            ))
        }),
    }
}

/// Resolved import tables of one unit or tail: import ordinal to image index.
#[derive(Default)]
pub(super) struct Imports {
    pub(super) objects: Vec<usize>,
    pub(super) globals: Vec<usize>,
}

/// One index space's resolved import tables, per unit and per tail.
pub(super) struct Resolved {
    pub(super) units: PerPackage<Vec<usize>>,
    pub(super) tails: PerPackage<Vec<usize>>,
}

impl Resolved {
    /// Pair an object-space resolution with a global-space one.
    pub(super) fn zip(objects: Self, globals: Self) -> (PerPackage<Imports>, PerPackage<Imports>) {
        let pair = |objects: PerPackage<Vec<usize>>, globals: PerPackage<Vec<usize>>| {
            PerPackage(
                objects
                    .0
                    .into_iter()
                    .zip(globals.0)
                    .map(|(objects, globals)| Imports { objects, globals })
                    .collect(),
            )
        };
        (
            pair(objects.units, globals.units),
            pair(objects.tails, globals.tails),
        )
    }
}

/// How a pooled object's operands map into the image.
pub(super) trait OperandSpace {
    fn object(&self, raw: usize) -> Result<usize, LinkError>;
    fn global(&self, raw: usize) -> Result<usize, LinkError>;
}

pub(super) struct UnitSpace<'l> {
    pub(super) name: &'l Name,
    pub(super) unit: &'l CompilationUnit,
    pub(super) objects: &'l UnitObjects,
    pub(super) slots: &'l SlotLayout,
    pub(super) imports: &'l Imports,
}

impl OperandSpace for UnitSpace<'_> {
    fn object(&self, raw: usize) -> Result<usize, LinkError> {
        object_operand(self.name, raw, &self.imports.objects, |local| {
            self.objects.local(self.unit, local)
        })
    }

    fn global(&self, raw: usize) -> Result<usize, LinkError> {
        global_operand(self.name, raw, &self.imports.globals, |local| {
            self.slots.local(local)
        })
    }
}

pub(super) struct TailSpace<'l> {
    pub(super) name: &'l Name,
    pub(super) tail: &'l InitTail,
    pub(super) objects: &'l [Placed],
    pub(super) slots: &'l TailSlots,
    pub(super) imports: &'l Imports,
}

impl OperandSpace for TailSpace<'_> {
    fn object(&self, raw: usize) -> Result<usize, LinkError> {
        object_operand(self.name, raw, &self.imports.objects, |local| {
            self.objects.get(local).map(|placed| placed.abs())
        })
    }

    fn global(&self, raw: usize) -> Result<usize, LinkError> {
        global_operand(self.name, raw, &self.imports.globals, |local| {
            self.slots.local(self.tail, local)
        })
    }
}

/// Rewrite every index operand of a pooled `object` into image space.
pub(super) fn relocate(object: &mut Object, space: &impl OperandSpace) -> Result<(), LinkError> {
    let mut failure = None;
    visit_object_operands(object, |operand| {
        if failure.is_some() {
            return;
        }
        let resolved = match &operand {
            IndexOperand::Object(index) => space.object(index.raw()),
            IndexOperand::Global(slot) => space.global(slot.raw()),
        };
        match (resolved, operand) {
            (Ok(abs), IndexOperand::Object(index)) => *index = ObjectIndex::from_raw(abs),
            (Ok(abs), IndexOperand::Global(slot)) => *slot = GlobalIndex::from_raw(abs),
            (Err(error), _) => failure = Some(error),
        }
    });
    failure.map_or(Ok(()), Err)
}

//! Operand spaces: how a pooled object's unit- or tail-convention operands
//! map into the image, and the one relocation walk over them — index
//! operands, type heads (a head is the declaration's operand until the walk
//! writes its assigned tag), and the type switches solved over those tags.

use baml_base::Name;
use baml_linker_types::{CompilationUnit, ImportEntry, InitTail, import_ordinal};
use baml_type::typetag::TypeTag;
use bex_vm_types::{
    GlobalIndex, Object, ObjectIndex, TypeHead,
    bytecode::{SwitchDispatch, SwitchKey},
    head_walk::visit_object_heads_mut,
    relink::{IndexOperand, visit_object_operands},
};

use super::{
    LinkError, PerPackage,
    layout::{Bucket, Placed, SlotLayout, TailSlots, UnitObjects},
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

/// Resolve an object operand that must name a type declaration: an import
/// of a type through `imports`, a local declaration through `local`. What a
/// head or a type switch's declaration key may name — the image index it
/// resolves to is the tag the link assigns that declaration.
fn declaration_operand(
    name: &Name,
    raw: usize,
    entries: &[ImportEntry],
    imports: &[usize],
    local: impl Fn(usize) -> Option<usize>,
) -> Result<usize, LinkError> {
    match import_ordinal(raw) {
        Some(ordinal) => {
            let Some(entry) = entries.get(ordinal) else {
                return Err(LinkError::invalid(format!(
                    "package `{name}` references object import {ordinal} of {}",
                    entries.len()
                )));
            };
            if !entry.key.path.is_type() {
                return Err(LinkError::invalid(format!(
                    "package `{name}` names {} where a type declaration is required",
                    entry.key.path
                )));
            }
            imports.get(ordinal).copied().ok_or_else(|| {
                LinkError::invalid(format!(
                    "package `{name}` references object import {ordinal} of {}",
                    imports.len()
                ))
            })
        }
        None => local(raw).ok_or_else(|| {
            LinkError::invalid(format!(
                "package `{name}` names local object {raw}, which is not a type declaration"
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
    fn name(&self) -> &Name;
    /// The package's position in the executable.
    fn ordinal(&self) -> usize;
    fn object(&self, raw: usize) -> Result<usize, LinkError>;
    fn global(&self, raw: usize) -> Result<usize, LinkError>;
    /// An object operand that must name a type declaration.
    fn declaration(&self, raw: usize) -> Result<usize, LinkError>;

    /// The tag the link assigns the declaration a unit-convention head
    /// names: that declaration's image index.
    fn head(&self, head: &TypeHead) -> Result<TypeHead, LinkError> {
        let Some(operand) = head.try_operand() else {
            return Err(LinkError::invalid(format!(
                "package `{}` carries a type head outside the unit convention",
                self.name()
            )));
        };
        let abs = self.declaration(operand.raw())?;
        Ok(TypeHead::unresolved(TypeTag::of_static_index(abs)))
    }
}

pub(super) struct UnitSpace<'l> {
    pub(super) name: &'l Name,
    pub(super) ordinal: usize,
    pub(super) unit: &'l CompilationUnit,
    pub(super) objects: &'l UnitObjects,
    pub(super) slots: &'l SlotLayout,
    pub(super) imports: &'l Imports,
}

impl OperandSpace for UnitSpace<'_> {
    fn name(&self) -> &Name {
        self.name
    }

    fn ordinal(&self) -> usize {
        self.ordinal
    }

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

    fn declaration(&self, raw: usize) -> Result<usize, LinkError> {
        // The type buckets come first in the unit convention, so a local
        // declaration operand is one below the code bucket.
        declaration_operand(
            self.name,
            raw,
            &self.unit.object_imports,
            &self.imports.objects,
            |local| {
                (local < Bucket::type_count(self.unit))
                    .then(|| self.objects.local(self.unit, local))
                    .flatten()
            },
        )
    }
}

pub(super) struct TailSpace<'l> {
    pub(super) name: &'l Name,
    pub(super) ordinal: usize,
    pub(super) tail: &'l InitTail,
    pub(super) objects: &'l [Placed],
    pub(super) slots: &'l TailSlots,
    pub(super) imports: &'l Imports,
}

impl OperandSpace for TailSpace<'_> {
    fn name(&self) -> &Name {
        self.name
    }

    fn ordinal(&self) -> usize {
        self.ordinal
    }

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

    fn declaration(&self, raw: usize) -> Result<usize, LinkError> {
        // A tail declares no types: every declaration it names is an import.
        declaration_operand(
            self.name,
            raw,
            &self.tail.object_imports,
            &self.imports.objects,
            |_| None,
        )
    }
}

/// Rewrite every index operand of a pooled `object` into image space, every
/// head into the tag its declaration is assigned, and solve every type
/// switch the object carries over those tags.
pub(super) fn relocate(object: &mut Object, space: &impl OperandSpace) -> Result<(), LinkError> {
    // A type switch states keys, and each declaration key names a type
    // declaration — checked while the keys are still unit operands, before
    // the walk below rewrites them.
    if let Object::Function(function) = &*object {
        for table in &function.bytecode.switch_tables {
            let SwitchDispatch::Keys(keys) = &table.dispatch else {
                return Err(LinkError::invalid(format!(
                    "package `{}`: function `{}` states a switch by values instead of keys",
                    space.name(),
                    function.name
                )));
            };
            for key in keys {
                if let SwitchKey::Declaration(declaration) = key {
                    space.declaration(declaration.raw())?;
                }
            }
        }
    }
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
    visit_object_heads_mut(object, &mut |head| {
        if failure.is_some() {
            return;
        }
        match space.head(head) {
            Ok(relocated) => *head = relocated,
            Err(error) => failure = Some(error),
        }
    });
    if let Some(error) = failure {
        return Err(error);
    }
    if let Object::Function(function) = object {
        for instruction in &mut function.bytecode.instructions {
            if let bex_vm_types::bytecode::Instruction::LoadCurrentPackage(ordinal) = instruction {
                *ordinal = space.ordinal();
            }
        }
        for table in &mut function.bytecode.switch_tables {
            let SwitchDispatch::Keys(keys) = &table.dispatch else {
                unreachable!("every table was checked to state keys before relocation")
            };
            // The keys are image indices now; the tag of each is its index.
            table.dispatch = SwitchDispatch::solved(keys, |declaration| {
                TypeTag::of_static_index(declaration.raw())
            });
        }
    }
    Ok(())
}

/// Relocate every head of a rule's templates through `space`.
pub(super) fn relocate_template_heads(
    template: &mut bex_vm_types::TyTemplate,
    space: &impl OperandSpace,
) -> Result<(), LinkError> {
    let mut failure = None;
    template.visit_heads_mut(&mut |head| {
        if failure.is_some() {
            return;
        }
        match space.head(head) {
            Ok(relocated) => *head = relocated,
            Err(error) => failure = Some(error),
        }
    });
    failure.map_or(Ok(()), Err)
}

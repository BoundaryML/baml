//! Where things land: the bucket vocabulary, each object's placement, and
//! the slot and object layouts of one unit or tail.

use std::{collections::HashMap, ops::Range};

use baml_base::Name;
use baml_linker_types::{CompilationUnit, InitTail, LocalRef, import_ordinal};
use baml_type::typetag::TypeTag;
use bex_vm_types::{DeclPath, Object, RealizedTy, TypeHead};

use super::{LinkError, LinkPackageId, resolve::PathKey};

/// The per-kind object buckets of a unit, in pool order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Bucket {
    Class,
    Enum,
    Interface,
    Alias,
    Code,
}

impl Bucket {
    pub(super) const ALL: [Self; 5] = [
        Self::Class,
        Self::Enum,
        Self::Interface,
        Self::Alias,
        Self::Code,
    ];

    pub(super) fn of(local: LocalRef) -> (Self, usize) {
        match local {
            LocalRef::Class(k) => (Self::Class, k as usize),
            LocalRef::Enum(k) => (Self::Enum, k as usize),
            LocalRef::Interface(k) => (Self::Interface, k as usize),
            LocalRef::TypeAlias(k) => (Self::Alias, k as usize),
            LocalRef::Code(k) => (Self::Code, k as usize),
        }
    }

    pub(super) fn objects(self, unit: &CompilationUnit) -> &[Object] {
        match self {
            Self::Class => &unit.classes,
            Self::Enum => &unit.enums,
            Self::Interface => &unit.interfaces,
            Self::Alias => &unit.type_alias_objects,
            Self::Code => &unit.code,
        }
    }

    /// The number of objects in the type buckets — the unit-convention
    /// operands below it name declarations, the ones from it code.
    pub(super) fn type_count(unit: &CompilationUnit) -> usize {
        unit.classes.len()
            + unit.enums.len()
            + unit.interfaces.len()
            + unit.type_alias_objects.len()
    }
}

/// Refuse a unit whose buckets hold objects of another kind, or a tail
/// holding what only a unit's type buckets may.
pub(super) fn validate_objects(
    name: &Name,
    unit: &CompilationUnit,
    tail: Option<&InitTail>,
) -> Result<(), LinkError> {
    let misplaced = unit
        .misplaced_object()
        .or_else(|| tail.and_then(InitTail::misplaced_object));
    match misplaced {
        Some(misplaced) => Err(LinkError::invalid(format!(
            "package `{name}` pools a {:?} object at offset {} of `{}`",
            bex_vm_types::ObjectType::of(misplaced.object),
            misplaced.offset,
            misplaced.bucket
        ))),
        None => Ok(()),
    }
}

/// Where one pooled object of a unit or tail landed.
#[derive(Clone, Copy, Debug)]
pub(super) enum Placed {
    Dropped,
    /// At this image index.
    Own(usize),
    /// A duplicate generic value: not placed; references go to the canonical
    /// copy at this image index.
    Shadow(usize),
}

impl Placed {
    pub(super) fn index(self) -> Option<usize> {
        match self {
            Self::Own(i) | Self::Shadow(i) => Some(i),
            Self::Dropped => None,
        }
    }

    pub(super) fn abs(self) -> usize {
        match self {
            Self::Own(abs) | Self::Shadow(abs) => abs,
            Self::Dropped => panic!("an unplaced object has no output index"),
        }
    }

    pub(super) fn is_shadow(self) -> bool {
        matches!(self, Self::Shadow(_) | Self::Dropped)
    }
}

/// What each head operand of one unit or tail names: the declaration's
/// `(package, path)` identity, for the local type buckets and the import
/// table. Heads are unit-convention operands until relocation, so anything
/// keyed on a type before then keys on this instead.
pub(super) struct HeadKeys {
    /// By unit-convention object index over the type buckets; `None` where
    /// no export names the object (a malformed unit).
    objects: Vec<Option<PathKey>>,
    /// By import ordinal; `None` where the import is not of a type, or
    /// names a dependency slot the table lacks (a malformed unit).
    imports: Vec<Option<PathKey>>,
}

impl HeadKeys {
    pub(super) fn of_unit(
        id: LinkPackageId,
        unit: &CompilationUnit,
        table: &[LinkPackageId],
    ) -> Self {
        let bases = [
            0,
            unit.classes.len(),
            unit.classes.len() + unit.enums.len(),
            unit.classes.len() + unit.enums.len() + unit.interfaces.len(),
        ];
        let total = bases[3] + unit.type_alias_objects.len();
        let mut objects = vec![None; total];
        for (path, local) in &unit.exports.objects {
            let index = match local {
                LocalRef::Class(k) => bases[0] + *k as usize,
                LocalRef::Enum(k) => bases[1] + *k as usize,
                LocalRef::Interface(k) => bases[2] + *k as usize,
                LocalRef::TypeAlias(k) => bases[3] + *k as usize,
                LocalRef::Code(_) => continue,
            };
            if let Some(slot) = objects.get_mut(index) {
                *slot = Some((id, path.clone()));
            }
        }
        Self {
            objects,
            imports: Self::import_keys(&unit.object_imports, table),
        }
    }

    pub(super) fn of_tail(tail: &InitTail, table: &[LinkPackageId]) -> Self {
        Self {
            objects: Vec::new(),
            imports: Self::import_keys(&tail.object_imports, table),
        }
    }

    fn import_keys(
        entries: &[baml_linker_types::ImportEntry],
        table: &[LinkPackageId],
    ) -> Vec<Option<PathKey>> {
        entries
            .iter()
            .map(|entry| {
                let id = *table.get(entry.key.dep.0 as usize)?;
                entry
                    .key
                    .path
                    .is_type()
                    .then(|| (id, entry.key.path.clone()))
            })
            .collect()
    }

    /// The declaration `head` names.
    fn key(&self, name: &Name, head: &TypeHead) -> Result<PathKey, LinkError> {
        let Some(operand) = head.try_operand() else {
            return Err(LinkError::invalid(format!(
                "package `{name}` carries a type head outside the unit convention"
            )));
        };
        let raw = operand.raw();
        let key = match import_ordinal(raw) {
            Some(ordinal) => self.imports.get(ordinal).cloned().flatten(),
            None => self.objects.get(raw).cloned().flatten(),
        };
        key.ok_or_else(|| {
            LinkError::invalid(format!(
                "package `{name}` carries a type head at operand {raw}, which names no declaration"
            ))
        })
    }
}

/// Interns generic-function values across the whole image by `(base
/// function's absolute slot, type args)`, with each type argument's heads
/// compared by declaration identity.
#[derive(Default)]
pub(super) struct Interner {
    values: HashMap<(usize, Vec<RealizedTy>), usize>,
    /// A link-local number per declaration, standing in for the tag the
    /// link has not assigned yet.
    declarations: HashMap<PathKey, usize>,
}

impl Interner {
    /// Lay out one run of code objects from `cursor`; returns each object's
    /// placement and the number actually placed (shadows are not).
    pub(super) fn place(
        &mut self,
        name: &Name,
        objects: &[Object],
        base_slot: impl Fn(usize) -> Result<usize, LinkError>,
        keys: &HeadKeys,
        cursor: usize,
    ) -> Result<(Vec<Placed>, usize), LinkError> {
        let mut placed = 0usize;
        let mut out = Vec::with_capacity(objects.len());
        for object in objects {
            if let Object::GenericFunction(generic) = object {
                let type_args = generic
                    .type_args
                    .iter()
                    .map(|ty| self.canonical(name, keys, ty))
                    .collect::<Result<Vec<_>, _>>()?;
                let key = (base_slot(generic.function.raw())?, type_args);
                if let Some(&canonical) = self.values.get(&key) {
                    out.push(Placed::Shadow(canonical));
                    continue;
                }
                self.values.insert(key, cursor + placed);
            }
            out.push(Placed::Own(cursor + placed));
            placed += 1;
        }
        Ok((out, placed))
    }

    /// `ty` with every head replaced by a link-local stand-in for its
    /// declaration, so two units' spellings of one type compare equal.
    fn canonical(
        &mut self,
        name: &Name,
        keys: &HeadKeys,
        ty: &RealizedTy,
    ) -> Result<RealizedTy, LinkError> {
        let mut failure = None;
        let declarations = &mut self.declarations;
        let canonical = ty.map_heads(&mut |head: &TypeHead| match keys.key(name, head) {
            Ok(key) => {
                let next = declarations.len();
                let ordinal = *declarations.entry(key).or_insert(next);
                TypeHead::unresolved(TypeTag::of_static_index(ordinal))
            }
            Err(error) => {
                failure.get_or_insert(error);
                *head
            }
        });
        failure.map_or(Ok(canonical), Err)
    }
}

/// A unit's global slots: functions and interface bodies first, then `let`s.
#[derive(Clone, Default)]
pub(super) struct SlotLayout {
    pub(super) selected: Option<Vec<Option<usize>>>,
    pub(super) func_base: usize,
    pub(super) func_count: usize,
    pub(super) let_base: usize,
    pub(super) let_count: usize,
}

impl SlotLayout {
    /// The partition a unit's export table declares, validated: every
    /// function or body at `[0, F)`, every `let` at `[F, F + L)`, no slot
    /// twice.
    pub(super) fn partition(name: &Name, unit: &CompilationUnit) -> Result<Self, LinkError> {
        let mut layout = Self::default();
        for (path, _) in &unit.exports.globals {
            match path {
                DeclPath::Function(_) | DeclPath::InterfaceBody(_) => layout.func_count += 1,
                DeclPath::Let(_) => layout.let_count += 1,
                other => {
                    return Err(LinkError::invalid(format!(
                        "package `{name}` exports a global slot for {other}, which owns none"
                    )));
                }
            }
        }
        let mut seen = vec![false; layout.func_count + layout.let_count];
        for (path, flat) in &unit.exports.globals {
            let flat = *flat as usize;
            let in_partition = match path {
                DeclPath::Let(_) => (layout.func_count..seen.len()).contains(&flat),
                _ => flat < layout.func_count,
            };
            if !in_partition {
                return Err(LinkError::invalid(format!(
                    "package `{name}` places {path} at local slot {flat}, outside its partition"
                )));
            }
            if std::mem::replace(&mut seen[flat], true) {
                return Err(LinkError::invalid(format!(
                    "package `{name}` exports two globals at local slot {flat}"
                )));
            }
        }
        Ok(layout)
    }

    pub(super) fn local(&self, raw: usize) -> Option<usize> {
        if let Some(map) = &self.selected {
            return map.get(raw).copied().flatten();
        }
        if raw < self.func_count {
            Some(self.func_base + raw)
        } else if raw < self.func_count + self.let_count {
            Some(self.let_base + (raw - self.func_count))
        } else {
            None
        }
    }
}

/// A tail's global slots: its init part's, then its test part's.
#[derive(Clone, Copy, Default)]
pub(super) struct TailSlots {
    pub(super) init_base: usize,
    pub(super) test_base: usize,
}

impl TailSlots {
    pub(super) fn base(&self, part: TailPart) -> usize {
        match part {
            TailPart::Init => self.init_base,
            TailPart::Test => self.test_base,
        }
    }

    pub(super) fn local(&self, tail: &InitTail, raw: usize) -> Option<usize> {
        let split = tail.test_slots_start as usize;
        if raw < split {
            Some(self.init_base + raw)
        } else if raw < tail.slot_objects.len() {
            Some(self.test_base + (raw - split))
        } else {
            None
        }
    }
}

/// The base of each of a unit's type buckets, and each code object's place.
#[derive(Default)]
pub(super) struct UnitObjects {
    pub(super) selected: Option<Vec<Option<usize>>>,
    pub(super) class_base: usize,
    pub(super) enum_base: usize,
    pub(super) interface_base: usize,
    pub(super) alias_base: usize,
    pub(super) code: Vec<Placed>,
}

impl UnitObjects {
    /// The image index of the `k`-th object of `bucket`, if the unit has one.
    fn at(&self, unit: &CompilationUnit, bucket: Bucket, k: usize) -> Option<usize> {
        if k >= bucket.objects(unit).len() {
            return None;
        }
        if let Some(map) = &self.selected {
            let mut flat = k;
            for prior in Bucket::ALL {
                if prior == bucket {
                    break;
                }
                flat += prior.objects(unit).len();
            }
            return map.get(flat).copied().flatten();
        }
        Some(match bucket {
            Bucket::Class => self.class_base + k,
            Bucket::Enum => self.enum_base + k,
            Bucket::Interface => self.interface_base + k,
            Bucket::Alias => self.alias_base + k,
            Bucket::Code => self.code[k].abs(),
        })
    }

    /// A unit-local object operand: the buckets laid end to end.
    pub(super) fn local(&self, unit: &CompilationUnit, raw: usize) -> Option<usize> {
        let mut offset = raw;
        for bucket in Bucket::ALL {
            let len = bucket.objects(unit).len();
            if offset < len {
                return self.at(unit, bucket, offset);
            }
            offset -= len;
        }
        None
    }

    /// An exported object. Generic values are never exported, so an exported
    /// code object is never a shadow.
    pub(super) fn export(&self, unit: &CompilationUnit, local: LocalRef) -> Option<usize> {
        let (bucket, k) = Bucket::of(local);
        if bucket == Bucket::Code && self.code.get(k).is_some_and(|placed| placed.is_shadow()) {
            return None;
        }
        self.at(unit, bucket, k)
    }
}

/// The two halves of a tail, each laid out and placed as its own run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum TailPart {
    Init,
    Test,
}

impl TailPart {
    pub(super) fn objects(self, tail: &InitTail) -> Range<usize> {
        let split = tail.test_objects_start as usize;
        match self {
            Self::Init => 0..split,
            Self::Test => split..tail.objects.len(),
        }
    }

    pub(super) fn slots(self, tail: &InitTail) -> &[u32] {
        let split = tail.test_slots_start as usize;
        match self {
            Self::Init => &tail.slot_objects[..split],
            Self::Test => &tail.slot_objects[split..],
        }
    }

    pub(super) fn named(self, tail: &InitTail) -> Option<u32> {
        match self {
            Self::Init => tail.init,
            Self::Test => tail.init_test,
        }
    }

    pub(super) fn synthesized_name(self) -> &'static str {
        match self {
            Self::Init => "$init",
            Self::Test => "$init_test",
        }
    }
}

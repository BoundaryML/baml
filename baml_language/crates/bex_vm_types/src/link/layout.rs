//! Where things land: the bucket vocabulary, each object's placement, and
//! the slot and object layouts of one unit or tail.

use std::{collections::HashMap, ops::Range};

use baml_base::Name;

use super::LinkError;
use crate::{
    Object, RealizedTy,
    unit::{CompilationUnit, DeclPath, InitTail, LocalRef},
};

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
}

/// Where one pooled object of a unit or tail landed.
#[derive(Clone, Copy, Debug)]
pub(super) enum Placed {
    /// At this image index.
    Own(usize),
    /// A duplicate generic value: not placed; references go to the canonical
    /// copy at this image index.
    Shadow(usize),
}

impl Placed {
    pub(super) fn abs(self) -> usize {
        match self {
            Self::Own(abs) | Self::Shadow(abs) => abs,
        }
    }

    pub(super) fn is_shadow(self) -> bool {
        matches!(self, Self::Shadow(_))
    }
}

/// Interns generic-function values across the whole image by `(base
/// function's absolute slot, type args)`.
#[derive(Default)]
pub(super) struct Interner(HashMap<(usize, Vec<RealizedTy>), usize>);

impl Interner {
    /// Lay out one run of code objects from `cursor`; returns each object's
    /// placement and the number actually placed (shadows are not).
    pub(super) fn place(
        &mut self,
        objects: &[Object],
        base_slot: impl Fn(usize) -> Result<usize, LinkError>,
        cursor: usize,
    ) -> Result<(Vec<Placed>, usize), LinkError> {
        let mut placed = 0usize;
        let mut out = Vec::with_capacity(objects.len());
        for object in objects {
            if let Object::GenericFunction(generic) = object {
                let key = (
                    base_slot(generic.function.raw())?,
                    generic.type_args.to_vec(),
                );
                if let Some(&canonical) = self.0.get(&key) {
                    out.push(Placed::Shadow(canonical));
                    continue;
                }
                self.0.insert(key, cursor + placed);
            }
            out.push(Placed::Own(cursor + placed));
            placed += 1;
        }
        Ok((out, placed))
    }
}

/// A unit's global slots: functions and interface bodies first, then `let`s.
#[derive(Clone, Copy, Default)]
pub(super) struct SlotLayout {
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

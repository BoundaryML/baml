//! The layout: package-major in set order — each package's buckets, then
//! its init part, then its test part — and, separately, the order the init
//! parts execute in.

use std::collections::{BTreeMap, BTreeSet};

use baml_base::Name;
use baml_linker_types::InitTail;

use super::{
    LinkError, LinkPackageId, LinkSet, PerPackage,
    bind::Tables,
    layout::{Bucket, HeadKeys, Interner, Placed, SlotLayout, TailPart, TailSlots, UnitObjects},
    space::{Resolved, global_operand},
};

/// The order everything is laid out in, and the order init parts run in.
pub(super) struct LayoutOrder {
    /// The packages, in set order: the placement order.
    members: Vec<LinkPackageId>,
    /// The packages with an init part, in initialization order.
    init: Vec<LinkPackageId>,
    /// The packages with a test part, by name, then by position.
    tests: Vec<LinkPackageId>,
}

impl LayoutOrder {
    pub(super) fn new(set: &LinkSet<'_>) -> Result<Self, LinkError> {
        for package in &set.packages {
            if let Some(tail) = package.tail {
                Self::validate_tail(&package.name, tail)?;
            }
        }
        let members: Vec<LinkPackageId> = set.ids().collect();
        let with_part = |part: TailPart| -> Vec<LinkPackageId> {
            members
                .iter()
                .copied()
                .filter(|&id| {
                    set.package(id)
                        .tail
                        .is_some_and(|tail| part.named(tail).is_some())
                })
                .collect()
        };
        let init = init_order(set, &with_part(TailPart::Init))?;
        let mut tests = with_part(TailPart::Test);
        tests.sort_by_key(|&id| (set.name(id).as_str(), id));
        Ok(Self {
            members,
            init,
            tests,
        })
    }

    fn validate_tail(name: &Name, tail: &InitTail) -> Result<(), LinkError> {
        let init_ok = tail
            .init
            .is_none_or(|k| TailPart::Init.objects(tail).contains(&(k as usize)));
        let test_ok = tail
            .init_test
            .is_none_or(|k| TailPart::Test.objects(tail).contains(&(k as usize)));
        let split_ok = tail.test_objects_start as usize <= tail.objects.len()
            && tail.test_slots_start as usize <= tail.slot_objects.len();
        let slots_ok = tail
            .slot_objects
            .iter()
            .all(|&object| (object as usize) < tail.objects.len());
        if init_ok && test_ok && split_ok && slots_ok {
            Ok(())
        } else {
            Err(LinkError::invalid(format!(
                "package `{name}` has a malformed init tail"
            )))
        }
    }

    /// The placement order.
    pub(super) fn members(&self) -> &[LinkPackageId] {
        &self.members
    }

    /// The order init parts execute in.
    pub(super) fn init_order(&self) -> &[LinkPackageId] {
        &self.init
    }

    /// The packages with a test part, by name, then by position.
    pub(super) fn test_order(&self) -> &[LinkPackageId] {
        &self.tests
    }

    /// The tail parts of `id` in placement order: its init part, then its
    /// test part, each only if the package has it.
    pub(super) fn parts(set: &LinkSet<'_>, id: LinkPackageId) -> impl Iterator<Item = TailPart> {
        let tail = set.package(id).tail;
        [TailPart::Init, TailPart::Test]
            .into_iter()
            .filter(move |part| tail.is_some_and(|tail| part.named(tail).is_some()))
    }
}

/// Package initialization order: a package with `let`s initializes after
/// every package with `let`s it reaches over the set's edges — directly, or
/// through packages that have none — and among the packages free to
/// initialize, the least by `(name, position)` goes first, so one set has
/// one order.
///
/// Reach is taken over every edge rather than between the members alone:
/// the packages without `let`s between two members still carry the
/// dependency. The language packages reach one another, so the graph over
/// all packages is not acyclic; only two MEMBERS that reach each other are an
/// error ([`LinkError::InitCycle`]). Reach is transitive, so with no such
/// pair the order it induces on the members is a strict partial order, and
/// every member is placed.
fn init_order(
    set: &LinkSet<'_>,
    members: &[LinkPackageId],
) -> Result<Vec<LinkPackageId>, LinkError> {
    let reach = |from: LinkPackageId| -> BTreeSet<LinkPackageId> {
        let mut reached = BTreeSet::new();
        let mut stack = vec![from];
        while let Some(id) = stack.pop() {
            for edge in &set.package(id).edges {
                if reached.insert(edge.target) {
                    stack.push(edge.target);
                }
            }
        }
        reached
    };
    // Each member's prerequisites: the other members it reaches.
    let prerequisites: BTreeMap<LinkPackageId, Vec<LinkPackageId>> = members
        .iter()
        .map(|&id| {
            let reached = reach(id);
            let before = members
                .iter()
                .copied()
                .filter(|&other| other != id && reached.contains(&other))
                .collect();
            (id, before)
        })
        .collect();
    for (&id, before) in &prerequisites {
        if let Some(&other) = before
            .iter()
            .find(|&&other| other > id && prerequisites[&other].contains(&id))
        {
            return Err(LinkError::InitCycle {
                package: set.name(id).clone(),
                other: set.name(other).clone(),
            });
        }
    }

    let mut dependents: BTreeMap<LinkPackageId, Vec<LinkPackageId>> = BTreeMap::new();
    let mut waiting: BTreeMap<LinkPackageId, usize> = BTreeMap::new();
    for (&id, before) in &prerequisites {
        waiting.insert(id, before.len());
        for &prerequisite in before {
            dependents.entry(prerequisite).or_default().push(id);
        }
    }
    let key = |id: LinkPackageId| (set.name(id).as_str(), id);
    let mut ready: BTreeSet<(&str, LinkPackageId)> = waiting
        .iter()
        .filter(|(_, count)| **count == 0)
        .map(|(&id, _)| key(id))
        .collect();
    let mut order = Vec::with_capacity(members.len());
    while let Some((_, id)) = ready.pop_first() {
        order.push(id);
        for &dependent in dependents.get(&id).into_iter().flatten() {
            let count = waiting
                .get_mut(&dependent)
                .unwrap_or_else(|| unreachable!("every dependent is a member"));
            *count -= 1;
            if *count == 0 {
                ready.insert(key(dependent));
            }
        }
    }
    debug_assert_eq!(
        order.len(),
        members.len(),
        "with no two members reaching each other, every member is placed"
    );
    Ok(order)
}

/// Every unit's and tail's slot layout.
pub(super) struct Slots {
    pub(super) units: PerPackage<SlotLayout>,
    pub(super) tails: PerPackage<TailSlots>,
    pub(super) total: usize,
}

impl Slots {
    pub(super) fn new(set: &LinkSet<'_>, order: &LayoutOrder) -> Result<Self, LinkError> {
        let mut units = PerPackage::try_from_set(set, |_, package| {
            SlotLayout::partition(&package.name, package.unit)
        })?;
        let mut tails = PerPackage::try_from_set(set, |_, _| Ok(TailSlots::default()))?;
        let mut cursor = 0usize;
        for &id in order.members() {
            units[id].func_base = cursor;
            cursor += units[id].func_count;
            units[id].let_base = cursor;
            cursor += units[id].let_count;
            for part in LayoutOrder::parts(set, id) {
                let tail = tail_of(set, id);
                match part {
                    TailPart::Init => tails[id].init_base = cursor,
                    TailPart::Test => tails[id].test_base = cursor,
                }
                cursor += part.slots(tail).len();
            }
        }
        Ok(Self {
            units,
            tails,
            total: cursor,
        })
    }
}

/// Every unit's and tail's object layout.
pub(super) struct Objects {
    pub(super) units: PerPackage<UnitObjects>,
    /// Each tail object's place, in tail order (both parts).
    pub(super) tails: PerPackage<Vec<Placed>>,
    pub(super) total: usize,
}

impl Objects {
    pub(super) fn new(
        set: &LinkSet<'_>,
        order: &LayoutOrder,
        slots: &Slots,
        globals: &Resolved,
        tables: &PerPackage<Tables>,
    ) -> Result<Self, LinkError> {
        let mut units = PerPackage::try_from_set(set, |_, _| Ok(UnitObjects::default()))?;
        let mut tails = PerPackage::try_from_set(set, |_, package| {
            Ok(vec![
                Placed::Own(0);
                package.tail.map_or(0, |tail| tail.objects.len())
            ])
        })?;
        let mut interner = Interner::default();
        let mut cursor = 0usize;
        for &id in order.members() {
            let package = set.package(id);
            let target = &mut units[id];
            for bucket in Bucket::ALL {
                match bucket {
                    Bucket::Class => target.class_base = cursor,
                    Bucket::Enum => target.enum_base = cursor,
                    Bucket::Interface => target.interface_base = cursor,
                    Bucket::Alias => target.alias_base = cursor,
                    Bucket::Code => {
                        let base_slot = |raw: usize| {
                            global_operand(&package.name, raw, &globals.units[id], |local| {
                                slots.units[id].local(local)
                            })
                        };
                        let keys = HeadKeys::of_unit(id, package.unit, &tables[id].unit);
                        let (code, placed) = interner.place(
                            &package.name,
                            &package.unit.code,
                            base_slot,
                            &keys,
                            cursor,
                        )?;
                        target.code = code;
                        cursor += placed;
                        continue;
                    }
                }
                cursor += bucket.objects(package.unit).len();
            }
            for part in LayoutOrder::parts(set, id) {
                let tail = tail_of(set, id);
                let range = part.objects(tail);
                let base_slot = |raw: usize| {
                    global_operand(&package.name, raw, &globals.tails[id], |local| {
                        slots.tails[id].local(tail, local)
                    })
                };
                let keys = HeadKeys::of_tail(id, tail, &tables[id].tail);
                let (placed, count) = interner.place(
                    &package.name,
                    &tail.objects[range.clone()],
                    base_slot,
                    &keys,
                    cursor,
                )?;
                tails[id][range].copy_from_slice(&placed);
                cursor += count;
            }
        }
        Ok(Self {
            units,
            tails,
            total: cursor,
        })
    }
}

/// The tail of a package the layout order placed — it has one by
/// construction.
pub(super) fn tail_of<'a>(set: &LinkSet<'a>, id: LinkPackageId) -> &'a InitTail {
    set.package(id)
        .tail
        .unwrap_or_else(|| unreachable!("ordered tails exist"))
}

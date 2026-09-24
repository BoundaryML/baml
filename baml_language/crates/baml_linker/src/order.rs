//! The layout: which package goes where, computed group-major, pass-major,
//! with the package tails in initialization then name order.

use std::collections::{HashMap, HashSet, VecDeque};

use baml_base::Name;
use baml_linker_types::InitTail;

use super::{
    LinkError, LinkGroup, LinkPackageId, LinkSet, PerPackage,
    layout::{Bucket, Interner, Placed, SlotLayout, TailPart, TailSlots, UnitObjects},
    space::{Resolved, global_operand},
};

/// One layout group: its packages in set order, and the order its package
/// tails are placed in.
pub(super) struct Group {
    pub(super) members: Vec<LinkPackageId>,
    init: Vec<LinkPackageId>,
    tests: Vec<LinkPackageId>,
}

impl Group {
    pub(super) fn tails(&self) -> impl Iterator<Item = (TailPart, LinkPackageId)> + '_ {
        self.init
            .iter()
            .map(|&id| (TailPart::Init, id))
            .chain(self.tests.iter().map(|&id| (TailPart::Test, id)))
    }
}

/// The order everything is laid out in.
pub(super) struct LayoutOrder([Group; 2]);

impl LayoutOrder {
    pub(super) fn new(set: &LinkSet<'_>) -> Result<Self, LinkError> {
        for package in &set.packages {
            if let Some(tail) = package.tail {
                Self::validate_tail(&package.name, tail)?;
            }
        }
        let group = |group: LinkGroup| -> Result<Group, LinkError> {
            let members: Vec<LinkPackageId> = set
                .ids()
                .filter(|&id| set.package(id).group == group)
                .collect();
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
            tests.sort_by(|a, b| set.name(*a).as_str().cmp(set.name(*b).as_str()));
            Ok(Group {
                members,
                init,
                tests,
            })
        };
        Ok(Self([group(LinkGroup::Stdlib)?, group(LinkGroup::User)?]))
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

    pub(super) fn groups(&self) -> impl Iterator<Item = &Group> {
        self.0.iter()
    }
}

/// Package initialization order within one group: Kahn over the group's edges
/// restricted to `members`, alphabetical ties, exactly as the whole-program
/// emit orders `$init`.
fn init_order(
    set: &LinkSet<'_>,
    members: &[LinkPackageId],
) -> Result<Vec<LinkPackageId>, LinkError> {
    let member_set: HashSet<LinkPackageId> = members.iter().copied().collect();
    let mut in_degree: HashMap<LinkPackageId, usize> = members.iter().map(|&id| (id, 0)).collect();
    let mut dependents: HashMap<LinkPackageId, Vec<LinkPackageId>> = HashMap::new();
    for &id in members {
        for &(_, dependency) in &set.package(id).edges {
            if dependency != id && member_set.contains(&dependency) {
                *in_degree.entry(id).or_insert(0) += 1;
                dependents.entry(dependency).or_default().push(id);
            }
        }
    }
    let by_name =
        |a: &LinkPackageId, b: &LinkPackageId| set.name(*a).as_str().cmp(set.name(*b).as_str());
    let mut ready: Vec<LinkPackageId> = in_degree
        .iter()
        .filter(|(_, degree)| **degree == 0)
        .map(|(&id, _)| id)
        .collect();
    ready.sort_by(by_name);
    let mut queue: VecDeque<LinkPackageId> = ready.into_iter().collect();
    let mut order = Vec::with_capacity(members.len());
    while let Some(id) = queue.pop_front() {
        order.push(id);
        let mut released: Vec<LinkPackageId> = dependents
            .get(&id)
            .into_iter()
            .flatten()
            .filter(|dependent| {
                let degree = in_degree
                    .get_mut(dependent)
                    .unwrap_or_else(|| unreachable!("every dependent is a member"));
                *degree -= 1;
                *degree == 0
            })
            .copied()
            .collect();
        released.sort_by(by_name);
        queue.extend(released);
    }
    if order.len() == members.len() {
        Ok(order)
    } else {
        Err(LinkError::invalid(
            "package dependency cycle among packages with `let`s",
        ))
    }
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
        for group in order.groups() {
            for &id in &group.members {
                units[id].func_base = cursor;
                cursor += units[id].func_count;
            }
            for &id in &group.members {
                units[id].let_base = cursor;
                cursor += units[id].let_count;
            }
            for (part, id) in group.tails() {
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
        for group in order.groups() {
            for bucket in Bucket::ALL {
                for &id in &group.members {
                    let package = set.package(id);
                    let target = &mut units[id];
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
                            let (code, placed) =
                                interner.place(&package.unit.code, base_slot, cursor)?;
                            target.code = code;
                            cursor += placed;
                            continue;
                        }
                    }
                    cursor += bucket.objects(package.unit).len();
                }
            }
            for (part, id) in group.tails() {
                let package = set.package(id);
                let tail = tail_of(set, id);
                let range = part.objects(tail);
                let base_slot = |raw: usize| {
                    global_operand(&package.name, raw, &globals.tails[id], |local| {
                        slots.tails[id].local(tail, local)
                    })
                };
                let (placed, count) =
                    interner.place(&tail.objects[range.clone()], base_slot, cursor)?;
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

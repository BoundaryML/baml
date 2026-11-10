//! Receiver checks and conversions shared by generated adapters and bodies.

use bex_vm_types::layout::{
    Descriptors, Elem, ElemKind, Fault, Kind, ListOf, Lists, NeedsVm, ObjPtr, ObjectModel, RawWord,
    Shape, SlotKind, Tagged,
};

fn wrong_receiver(expected: Kind, found: Shape) -> Fault {
    Fault::WrongReceiver {
        expected,
        found: found.kind(),
    }
}

/// The element slot of a list receiver.
pub(crate) fn list_elem_slot<R: Descriptors + ?Sized>(
    rt: &R,
    list: ObjPtr,
) -> Result<SlotKind, Fault> {
    match rt.shape(list) {
        Shape::List { elem } => Ok(elem),
        other => Err(wrong_receiver(Kind::List, other)),
    }
}

/// The value slot of a map receiver.
pub(crate) fn map_value_slot<R: Descriptors + ?Sized>(
    rt: &R,
    map: ObjPtr,
) -> Result<SlotKind, Fault> {
    match rt.shape(map) {
        Shape::Map { value, .. } => Ok(value),
        other => Err(wrong_receiver(Kind::Map, other)),
    }
}

/// Check a string receiver.
pub(crate) fn str_receiver<R: Descriptors + ?Sized>(rt: &R, string: ObjPtr) -> Result<(), Fault> {
    match rt.shape(string) {
        Shape::Str => Ok(()),
        other => Err(wrong_receiver(Kind::Str, other)),
    }
}

/// A generic map key as a string object; any other key needs the VM's
/// hashing, which may run user code.
pub(crate) fn string_key<R: Descriptors + ?Sized>(rt: &R, key: Tagged) -> Result<ObjPtr, Fault> {
    match key {
        Tagged::Ptr(ptr) if rt.shape(ptr) == Shape::Str => Ok(ptr),
        _ => Err(Fault::NeedsVm(NeedsVm::KeyHash)),
    }
}

/// Every element of `list` in its universal form.
pub(crate) fn tagged_elems<R: ObjectModel>(rt: &R, list: ObjPtr) -> Result<Vec<Tagged>, Fault> {
    let slot = list_elem_slot(rt, list)?;
    Ok(match slot.elem_kind() {
        ElemKind::Raw8 => to_tagged(Lists::<RawWord>::list_to_vec(rt, list), slot),
        ElemKind::Ptr => to_tagged(Lists::<ObjPtr>::list_to_vec(rt, list), slot),
        ElemKind::Tagged16 => Lists::<Tagged>::list_to_vec(rt, list),
    })
}

fn to_tagged<E: Elem>(elems: Vec<E>, slot: SlotKind) -> Vec<Tagged> {
    elems.into_iter().map(|elem| elem.to_tagged(slot)).collect()
}

/// Every element of `list` as `E` for a slot of kind `slot`, converting when
/// `list` stores its elements differently.
pub(crate) fn elems_as<R: ObjectModel + Lists<E>, E: Elem>(
    rt: &R,
    list: ObjPtr,
    slot: SlotKind,
) -> Result<Vec<E>, Fault> {
    if list_elem_slot(rt, list)? == slot {
        return Ok(Lists::<E>::list_to_vec(rt, list));
    }
    tagged_elems(rt, list)?
        .into_iter()
        .map(|value| E::from_tagged(value, slot))
        .collect()
}

/// Allocate a list described by `of` from universal elements.
pub(crate) fn alloc_tagged_list<R: ObjectModel>(
    rt: &mut R,
    of: ListOf,
    items: Vec<Tagged>,
) -> Result<ObjPtr, Fault> {
    let slot = rt.list_of_slot(of);
    match slot.elem_kind() {
        ElemKind::Raw8 => alloc_as::<R, RawWord>(rt, of, items, slot),
        ElemKind::Ptr => alloc_as::<R, ObjPtr>(rt, of, items, slot),
        ElemKind::Tagged16 => Ok(Lists::<Tagged>::list_alloc(rt, of, &items)),
    }
}

fn alloc_as<R: Lists<E>, E: Elem>(
    rt: &mut R,
    of: ListOf,
    items: Vec<Tagged>,
    slot: SlotKind,
) -> Result<ObjPtr, Fault> {
    let elems = items
        .into_iter()
        .map(|value| E::from_tagged(value, slot))
        .collect::<Result<Vec<E>, Fault>>()?;
    Ok(rt.list_alloc(of, &elems))
}

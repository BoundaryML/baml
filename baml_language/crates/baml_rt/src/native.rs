//! Loop-owned native payloads. Managed handles carry no hidden heap edges.
use crate::{R, strings::Cx};
use baml_rt_gc::{Descriptor, Kind};
use std::rc::Rc;
pub const DESC: Descriptor = Descriptor {
    kind: Kind::Opaque,
    size: 8,
    ptr_slots: (0, 0),
    tagged_slots: (0, 0),
    elem: None,
    type_id: 18,
    fields: &[],
    finalizer: None,
};
/// # Safety
/// The payload must not contain managed pointers. Its handle's descriptor has
/// no pointer slots; the collector cannot trace Rust-owned payload fields.
pub unsafe fn new<T: std::any::Any>(cx: &mut Cx, payload: T) -> R<usize> {
    let object = cx.alloc(15, 8)? as usize;
    unsafe {
        (&mut *crate::HEAP.get()).attach_native(object, Rc::new(payload));
    }
    Ok(object)
}
/// # Safety
/// `object` is a live handle of T in the active instance.
pub unsafe fn get<T: std::any::Any>(object: usize) -> Rc<T> {
    unsafe { (&*crate::HEAP.get()).native(object) }
}

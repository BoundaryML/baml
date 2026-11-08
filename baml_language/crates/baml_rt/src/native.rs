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
    type_id: 15,
    fields: &[],
    finalizer: None,
};
pub fn new<T: std::any::Any>(cx: &mut Cx, payload: T) -> R<usize> {
    let object = unsafe { cx.alloc(12, 8)? } as usize;
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

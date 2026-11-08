//! JSON construction boundary for the shared immutable bigint layout.
use crate::{R, strings::Cx};
use std::{marker::PhantomData, rc::Rc};
#[derive(Clone, Copy)]
pub struct Bigint(usize, PhantomData<Rc<()>>);
impl Bigint {
    pub fn as_raw(self) -> usize {
        self.0
    }
}
use baml_rt_gc::Descriptor;
pub const fn descriptor(tag: u32) -> Descriptor {
    crate::tasks::bigint_descriptor(tag)
}
/// # Safety
/// `desc` describes the active instance's immutable bigint layout.
/// The returned handle is opaque; only its raw address is consumed by JSON.
pub unsafe fn from_decimal(_cx: &mut Cx, desc: u32, text: &str) -> R<Bigint> {
    unsafe { crate::tasks::bigint_text(desc, text).map(|p| Bigint(p, PhantomData)) }
}

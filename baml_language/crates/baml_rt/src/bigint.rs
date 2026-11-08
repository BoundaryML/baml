//! Immutable normalized decimal storage for JSON bigint leaves.
use crate::{
    R,
    strings::{Cx, Str},
};
use baml_rt_gc::Descriptor;
pub const fn descriptor(tag: u32) -> Descriptor {
    Descriptor {
        type_id: tag,
        ..crate::STRING_DESC
    }
}
/// The descriptor uses the flat string layout and a distinct dynamic tag.
/// # Safety
/// `desc` is the bigint descriptor in the active instance.
pub unsafe fn from_decimal(_cx: &mut Cx, desc: u32, text: &str) -> R<Str> {
    let integer =
        bex_vm_types::json_text::parse_decimal_bigint(text).expect("validated decimal bigint");
    let text = integer.to_string();
    let bytes = (text.len() + 7 & !7) + 16;
    let pointer = unsafe { crate::try_alloc(desc, bytes) }?;
    unsafe {
        pointer.add(8).cast::<usize>().write(text.len());
        std::ptr::copy_nonoverlapping(text.as_ptr(), pointer.add(16), text.len());
        Ok(Str::from_raw(pointer as usize))
    }
}

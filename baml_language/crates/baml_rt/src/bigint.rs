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
pub unsafe fn from_decimal(cx: &mut Cx, desc: u32, text: &str) -> R<Str> {
    let integer =
        bex_vm_types::json_text::parse_decimal_bigint(text).expect("validated decimal bigint");
    let text = integer.to_string();
    let bytes = crate::strings::allocation_size(text.len())?;
    let pointer = cx.alloc(desc, bytes)?;
    unsafe {
        crate::strings::init_flat(pointer, &text);
        Ok(Str::from_raw(pointer as usize))
    }
}

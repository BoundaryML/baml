//! Managed UTF-8 byte buffers used at the string boundary.
use crate::{
    R,
    strings::{Cx, Str},
};
use baml_rt_gc::{Descriptor, ElementLayout, Elements, Kind};
pub const DESC: Descriptor = Descriptor {
    kind: Kind::Opaque,
    size: 8,
    ptr_slots: (0, 0),
    tagged_slots: (0, 0),
    elem: Some(Elements {
        offset: 16,
        layout: ElementLayout::Scalar,
    }),
    type_id: 19,
    fields: &[],
    finalizer: None,
};
/// # Safety
/// `value` is a rooted byte array owned by the active instance.
pub unsafe fn data<'a>(value: usize) -> &'a [u8] {
    unsafe { std::slice::from_raw_parts((value + 16) as *const u8, *((value + 8) as *const usize)) }
}
pub fn new(cx: &mut Cx, data: &[u8]) -> R<usize> {
    let size = data
        .len()
        .checked_add(7)
        .map(|n| n & !7)
        .and_then(|n| n.checked_add(16))
        .ok_or_else(|| crate::allocation_failure(usize::MAX))?;
    let p = cx.alloc(16, size)?;
    unsafe {
        p.add(8).cast::<usize>().write(data.len());
        std::ptr::copy_nonoverlapping(data.as_ptr(), p.add(16), data.len());
    }
    Ok(p as usize)
}
pub fn encode(cx: &mut Cx, text: Str) -> R<usize> {
    let read = unsafe { Cx::current() };
    new(cx, text.try_as_str(&read)?.as_bytes())
}
/// # Safety
/// `value` obeys `data`'s lifetime and rooting contract.
pub unsafe fn decode(cx: &mut Cx, value: usize, error_desc: u32) -> R<Str> {
    match std::str::from_utf8(unsafe { data(value) }) {
        Ok(text) => cx.string(text),
        Err(error) => Err(crate::language::panic_value(
            error_desc,
            "baml.errors.InvalidArgument",
            format!(
                "string.from_utf8: invalid UTF-8 at byte {}: {error}",
                error.valid_up_to()
            ),
        )),
    }
}

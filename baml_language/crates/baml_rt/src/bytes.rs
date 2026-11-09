//! Managed UTF-8 byte buffers used at the string boundary.
use crate::{
    R,
    strings::{Cx, Str},
};
use baml_rt_gc::Descriptor;
pub const DESC: Descriptor = crate::native::BYTES_LAYOUT;
/// # Safety
/// `value` is a rooted byte array owned by the active instance.
pub unsafe fn data<'a>(value: usize) -> &'a [u8] {
    unsafe {
        let p = value as *const u8;
        let len = p.add(16).cast::<usize>().read();
        if len == 0 {
            return &[];
        }
        std::slice::from_raw_parts(p.add(8).cast::<*const u8>().read().add(8), len)
    }
}
pub fn new(_cx: &mut Cx, data: &[u8]) -> R<usize> {
    crate::native::bytes(data.to_vec())
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

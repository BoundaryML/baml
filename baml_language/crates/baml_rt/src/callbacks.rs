//! Snapshot native callback inputs before invoking user code, as the VM does.
use crate::R;
/// # Safety
/// The descriptor is the input's fixed buffer layout (with an eight-byte slot).
pub unsafe fn snapshot(source: usize, growing: bool, desc: u32) -> R<usize> {
    unsafe {
        let source = crate::nonnull(source)? as *const usize;
        let (data, len) = if growing {
            let buffer = *source.add(1) as *const usize;
            (buffer.wrapping_add(2), *source.add(2))
        } else {
            (source.add(2), *source.add(1))
        };
        let out = array(desc, len)?;
        // Collection occurs only in array(), while source still roots the input.
        std::ptr::copy_nonoverlapping(data, out.add(16).cast(), len);
        Ok(out as usize)
    }
}
/// # Safety
/// The descriptor describes a fixed eight-byte array. Initialize pointer slots
/// before invoking user code; allocation has already zeroed every slot.
pub unsafe fn array(desc: u32, len: usize) -> R<*mut u8> {
    let bytes = len
        .checked_mul(8)
        .and_then(|n| n.checked_add(16))
        .ok_or_else(|| crate::Error::internal("callback array too large".into()))?;
    let out = unsafe { crate::try_alloc(desc, bytes)? };
    unsafe {
        out.add(8).cast::<usize>().write(len);
    }
    Ok(out)
}

//! Heap-neutral sequence indexing shared by runtime backends.

/// Resolves a possibly-negative `slice` bound against a sequence of length
/// `len`, counting negatives back from the end and then clamping the result
/// into `[0, len]`.
///
/// Unlike the element-index resolver, an out-of-range bound saturates to the nearest
/// endpoint rather than failing, matching JavaScript's `Array.prototype.slice`.
pub fn resolve_slice_bound(bound: i64, len: usize) -> usize {
    // `count_from_end` is not reused here: an out-of-range bound clamps rather
    // than fails, so the impossible `len > i64::MAX` case clamps too.
    let len_i64 = i64::try_from(len).unwrap_or(i64::MAX);
    // `bound < 0` ⟹ the sum moves toward zero, so it cannot overflow.
    let resolved = if bound < 0 { bound + len_i64 } else { bound };
    // The clamp lands in `[0, len]`, which always fits back into `usize`.
    usize::try_from(resolved.clamp(0, len_i64)).unwrap_or(len)
}

//! Negative-index resolution for sequence operations (`at`, subscript `[]`,
//! `slice`, `remove_at`, `insert`, and `splice`) on arrays and byte arrays.
//!
//! All of them follow the JavaScript/Python convention: a negative index counts
//! back from the end, so `-1` is the last element. They differ only in how an
//! index that still falls outside the sequence is handled:
//!
//! - [`resolve_index`] (used by `at`, subscript `[]`, and `remove_at`) requires
//!   an exact in-bounds element offset in `[0, len)`. `at`/`remove_at` map the
//!   out-of-range `None` to `null`; the subscript operator maps it to an
//!   `IndexOutOfBounds` panic.
//! - [`resolve_insert_index`] (used by `insert` and `splice`'s `start`) accepts
//!   the *insertion* range `[0, len]` — the end slot is valid because it denotes
//!   "after the last element". Callers surface the out-of-range `None` as an
//!   `InvalidArgument` error.
//! - [`resolve_slice_bound`] (used by `slice`) clamps each bound into
//!   `[0, len]` instead of failing, so an out-of-range bound saturates to the
//!   nearest end.

/// Counts a possibly-negative `index` back from the end of a `len`-element
/// sequence, returning the resolved position — which may itself still be out of
/// range (negative, or `>= len`). A non-negative `index` passes through
/// unchanged. Callers apply their own bounds check to the result.
///
/// Returns `None` only when `len` does not fit in `i64`, which never happens for
/// a real slice (its length cannot exceed `isize::MAX`).
fn count_from_end(index: i64, len: usize) -> Option<i64> {
    let len_i64 = i64::try_from(len).ok()?;
    // `index < 0` ⟹ `index + len_i64` moves toward zero, so it cannot overflow.
    Some(if index < 0 { index + len_i64 } else { index })
}

/// Resolves a possibly-negative element index against a sequence of length
/// `len`, counting negatives back from the end.
///
/// Returns the non-negative offset when the index lands in `[0, len)`, or
/// `None` when it falls outside the sequence even after counting from the end
/// (e.g. `-len - 1`, or any index `>= len`).
pub(crate) fn resolve_index(index: i64, len: usize) -> Option<usize> {
    // `try_from` rejects the below-start case (`< 0`); the filter rejects the
    // past-end case (`>= len`).
    usize::try_from(count_from_end(index, len)?)
        .ok()
        .filter(|&i| i < len)
}

/// Resolves a possibly-negative *insertion* index against a sequence of length
/// `len`, counting negatives back from the end.
///
/// Like [`resolve_index`], but the end position `len` is in range: insertion
/// accepts `[0, len]` because inserting *at* `len` appends after the last
/// element. Returns the position when it lands in `[0, len]`, or `None` when it
/// falls outside even after counting from the end (callers surface this as an
/// `InvalidArgument` error). Used by `insert` and `splice`'s `start`.
pub(crate) fn resolve_insert_index(index: i64, len: usize) -> Option<usize> {
    // As `resolve_index`, but `<= len`: the end slot is a valid insertion point.
    usize::try_from(count_from_end(index, len)?)
        .ok()
        .filter(|&i| i <= len)
}

/// Resolves a possibly-negative `slice` bound against a sequence of length
/// `len`, counting negatives back from the end and then clamping the result
/// into `[0, len]`.
///
/// Unlike [`resolve_index`], an out-of-range bound saturates to the nearest
/// endpoint rather than failing, matching JavaScript's `Array.prototype.slice`.
pub(crate) fn resolve_slice_bound(bound: i64, len: usize) -> usize {
    // `count_from_end` is not reused here: an out-of-range bound clamps rather
    // than fails, so the impossible `len > i64::MAX` case clamps too.
    let len_i64 = i64::try_from(len).unwrap_or(i64::MAX);
    // `bound < 0` ⟹ the sum moves toward zero, so it cannot overflow.
    let resolved = if bound < 0 { bound + len_i64 } else { bound };
    // The clamp lands in `[0, len]`, which always fits back into `usize`.
    usize::try_from(resolved.clamp(0, len_i64)).unwrap_or(len)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_index_positive_in_bounds() {
        assert_eq!(resolve_index(0, 3), Some(0));
        assert_eq!(resolve_index(2, 3), Some(2));
    }

    #[test]
    fn resolve_index_negative_counts_from_end() {
        assert_eq!(resolve_index(-1, 3), Some(2));
        assert_eq!(resolve_index(-3, 3), Some(0));
    }

    #[test]
    fn resolve_index_out_of_bounds_is_none() {
        assert_eq!(resolve_index(3, 3), None); // past the end
        assert_eq!(resolve_index(-4, 3), None); // past the start
        assert_eq!(resolve_index(0, 0), None); // empty sequence
        assert_eq!(resolve_index(-1, 0), None); // empty sequence, negative
        assert_eq!(resolve_index(i64::MAX, 3), None);
        assert_eq!(resolve_index(i64::MIN, 3), None);
    }

    #[test]
    fn resolve_insert_index_allows_the_end_slot() {
        // Positive: `[0, len]` inclusive — `len` appends.
        assert_eq!(resolve_insert_index(0, 3), Some(0));
        assert_eq!(resolve_insert_index(3, 3), Some(3));
        assert_eq!(resolve_insert_index(0, 0), Some(0)); // empty: only the end slot
        // Negative counts from the end and reaches `[0, len - 1]`.
        assert_eq!(resolve_insert_index(-1, 3), Some(2));
        assert_eq!(resolve_insert_index(-3, 3), Some(0));
    }

    #[test]
    fn resolve_insert_index_out_of_range_is_none() {
        assert_eq!(resolve_insert_index(4, 3), None); // past the end slot
        assert_eq!(resolve_insert_index(-4, 3), None); // past the start
        assert_eq!(resolve_insert_index(-1, 0), None); // empty, negative
        assert_eq!(resolve_insert_index(i64::MAX, 3), None);
        assert_eq!(resolve_insert_index(i64::MIN, 3), None);
    }

    #[test]
    fn resolve_slice_bound_counts_from_end_and_clamps() {
        assert_eq!(resolve_slice_bound(1, 4), 1);
        assert_eq!(resolve_slice_bound(-2, 4), 2);
        assert_eq!(resolve_slice_bound(-100, 4), 0); // clamped to the start
        assert_eq!(resolve_slice_bound(100, 4), 4); // clamped to the end
        assert_eq!(resolve_slice_bound(0, 0), 0);
        assert_eq!(resolve_slice_bound(i64::MIN, 4), 0);
        assert_eq!(resolve_slice_bound(i64::MAX, 4), 4);
    }
}

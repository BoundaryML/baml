/// Replaces a Salsa query value when whole-value equality changes.
///
/// # Safety
///
/// The caller must uphold the safety contract of
/// [`salsa::Update::maybe_update`], including that comparing values across
/// revisions is safe and equality makes replacement unnecessary.
#[allow(unsafe_code)]
pub unsafe fn update_by_eq<T: PartialEq>(old_pointer: *mut T, new_value: T) -> bool {
    // SAFETY: The caller upholds Salsa's `Update::maybe_update` contract.
    let old = unsafe { &*old_pointer };
    if old == &new_value {
        false
    } else {
        // SAFETY: The caller guarantees that `old_pointer` is valid for replacement.
        unsafe {
            std::ptr::drop_in_place(old_pointer);
            std::ptr::write(old_pointer, new_value);
        }
        true
    }
}

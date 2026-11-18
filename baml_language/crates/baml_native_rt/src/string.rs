//! Strings: immutable UTF-8 with a cached count of characters, so that
//! `length()` is a load.

use crate::{
    heap::{Mutator, current_frame, load, store},
    layout::{STRING_CHAR_COUNT_SHIFT, STRING_LENGTHS, STRING_TEXT, TYPE_STRING, WORD, words},
};

/// The text of the string object at `string`.
///
/// # Safety
///
/// `string` is a live string object, and the result is not used after the
/// next allocation.
pub unsafe fn text<'a>(string: usize) -> &'a str {
    // SAFETY: a string object holds its byte length and then that many
    // bytes of UTF-8.
    unsafe {
        let length = load(string + STRING_LENGTHS) & 0xFFFF_FFFF;
        std::str::from_utf8_unchecked(std::slice::from_raw_parts(
            (string + STRING_TEXT) as *const u8,
            length,
        ))
    }
}

/// How many characters the string object at `string` has.
///
/// # Safety
///
/// `string` is a live string object.
unsafe fn char_count(string: usize) -> usize {
    // SAFETY: the count is the high half of the second word.
    unsafe { load(string + STRING_LENGTHS) >> STRING_CHAR_COUNT_SHIFT }
}

/// Allocate a string of `bytes` bytes and `chars` characters with its text
/// left to be written. May collect.
fn allocate(mutator: &mut Mutator, bytes: usize, chars: usize) -> usize {
    assert!(
        u32::try_from(bytes).is_ok(),
        "a string cannot be longer than 4 GiB"
    );
    let string = mutator.allocate_object(TYPE_STRING, STRING_TEXT + words(bytes));
    // SAFETY: the object is at least `STRING_TEXT` bytes.
    unsafe {
        store(
            string + STRING_LENGTHS,
            bytes | (chars << STRING_CHAR_COUNT_SHIFT),
        );
    };
    string
}

/// A new string object holding `value`. May collect.
pub fn from_str(mutator: &mut Mutator, value: &str) -> usize {
    let string = allocate(mutator, value.len(), value.chars().count());
    // SAFETY: the object has room for `value.len()` bytes of text.
    unsafe {
        std::ptr::copy_nonoverlapping(
            value.as_ptr(),
            (string + STRING_TEXT) as *mut u8,
            value.len(),
        );
    }
    string
}

/// Join the `count` strings whose references are the words at `parts`.
///
/// # Safety
///
/// Called from compiled code with its mutator; `parts` is the address of
/// `count` string references in the caller's frame.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baml_rt_string_concat(
    mutator: &mut Mutator,
    parts: usize,
    count: usize,
) -> usize {
    mutator.enter(current_frame());
    let (mut bytes, mut chars) = (0, 0);
    for index in 0..count {
        // SAFETY: the caller's contract.
        unsafe {
            let part = load(parts + index * WORD);
            bytes += text(part).len();
            chars += char_count(part);
        }
        // The allocation below can move the parts.
        mutator.root_at(parts + index * WORD);
    }
    let joined = allocate(mutator, bytes, chars);
    mutator.unroot(count);
    let mut cursor = joined + STRING_TEXT;
    for index in 0..count {
        // SAFETY: the parts were kept current, and `joined` has room for
        // all of their text.
        unsafe {
            let part = text(load(parts + index * WORD));
            std::ptr::copy_nonoverlapping(part.as_ptr(), cursor as *mut u8, part.len());
            cursor += part.len();
        }
    }
    joined
}

/// Whether two strings have the same text.
///
/// # Safety
///
/// Both are live string objects.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baml_rt_string_eq(left: usize, right: usize) -> u8 {
    // SAFETY: the caller's contract.
    u8::from(left == right || unsafe { text(left) == text(right) })
}

/// Order two strings by their bytes: negative, zero or positive.
///
/// # Safety
///
/// Both are live string objects.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baml_rt_string_cmp(left: usize, right: usize) -> i64 {
    // SAFETY: the caller's contract.
    unsafe { text(left).cmp(text(right)) as i64 }
}

/// The decimal text of an `int`.
///
/// # Safety
///
/// Called from compiled code with its mutator.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baml_rt_string_from_int(mutator: &mut Mutator, value: i64) -> usize {
    mutator.enter(current_frame());
    from_str(mutator, &value.to_string())
}

/// The text of a `float` inside a string: as a result prints, except that
/// the infinities are spelled out.
pub fn float_text(value: f64) -> String {
    if value.is_infinite() {
        return if value > 0.0 { "Infinity" } else { "-Infinity" }.to_owned();
    }
    crate::float_text(value)
}

/// The text of a `float`.
///
/// # Safety
///
/// Called from compiled code with its mutator.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baml_rt_string_from_float(mutator: &mut Mutator, value: f64) -> usize {
    mutator.enter(current_frame());
    from_str(mutator, &float_text(value))
}

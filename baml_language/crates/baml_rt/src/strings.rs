//! Flat-string building blocks for typed rt builtin glue.
//! These operate within an installed root task; no interpreter heap is linked.
use std::{marker::PhantomData, rc::Rc};

use crate::{R, language::allocation_failure};

/// Owner-thread builtin context. Generated adapters construct one only while a
/// runtime root binding is active. It never exposes RootSource or TaskCx.
pub struct Cx {
    _owner: PhantomData<Rc<()>>,
}
impl Cx {
    /// # Safety
    /// The context and every handle used with it must stay within the current
    /// root invocation. Handles must stay rooted through collecting calls.
    #[inline(always)]
    pub unsafe fn current() -> Self {
        debug_assert!(!crate::HEAP.get().is_null(), "builtin outside a root task");
        Self {
            _owner: PhantomData,
        }
    }

    pub fn poll(&mut self) -> R<()> {
        crate::language::poll()
    }

    pub fn string(&mut self, text: &str) -> R<Str> {
        if text.len() > baml_type::Int63::MAX.get() as usize {
            return Err(allocation_failure(text.len()));
        }
        let bytes = text
            .len()
            .checked_add(7)
            .and_then(|n| (n & !7).checked_add(16))
            .ok_or_else(|| allocation_failure(usize::MAX))?;
        let pointer = unsafe { crate::try_alloc(0, bytes) }?;
        unsafe {
            (*(pointer as *mut baml_rt_gc::Header)).aux = if text.is_ascii() { 1 } else { 2 };
            pointer.add(8).cast::<usize>().write(text.len());
            std::ptr::copy_nonoverlapping(text.as_ptr(), pointer.add(16), text.len());
        }
        Ok(Str(pointer as usize, PhantomData))
    }
}

/// A pointer to the flat string layout used by STRING_DESC.
#[derive(Clone, Copy)]
#[repr(transparent)]
pub struct Str(usize, PhantomData<Rc<()>>);
impl Str {
    /// # Safety
    /// `pointer` must be a live flat string in the current instance, with
    /// byte length at most Int63::MAX. Its
    /// lifetime/rooting must follow the Cx contract, including any copies.
    pub unsafe fn from_raw(pointer: usize) -> Self {
        Self(pointer, PhantomData)
    }
    pub fn as_raw(self) -> usize {
        self.0
    }

    fn is_ascii(self, cx: &Cx) -> bool {
        // Flat strings are immutable and owner-thread local. The header's aux
        // word is unused by GC; cache this property even for runtime-produced
        // strings that did not go through Cx::string.
        unsafe {
            let header = self.0 as *mut baml_rt_gc::Header;
            if (*header).aux == 0 {
                (*header).aux = if self.as_str(cx).is_ascii() { 1 } else { 2 };
            }
            (*header).aux == 1
        }
    }

    pub fn as_str<'a>(&self, _cx: &'a Cx) -> &'a str {
        unsafe {
            let pointer = self.0 as *const u8;
            let len = pointer.add(8).cast::<usize>().read();
            std::str::from_utf8_unchecked(std::slice::from_raw_parts(pointer.add(16), len))
        }
    }
}

/// Unicode scalar count, the same meaning as String.length/char_count.
pub fn length(cx: &Cx, value: Str) -> i64 {
    // The constructor bounds byte length by i63; code-point count cannot exceed it.
    if value.is_ascii(cx) {
        value.as_str(cx).len() as i64
    } else {
        value.as_str(cx).chars().count() as i64
    }
}

pub fn concat(cx: &mut Cx, values: &[Str]) -> R<Str> {
    let mut text = String::new();
    for value in values {
        text.push_str(value.as_str(cx));
    }
    cx.string(&text)
}

pub fn slice(cx: &mut Cx, value: Str, start: i64, end: i64) -> R<Str> {
    // Borrow the managed input. Constructing a BexStr here copies and counts
    // the entire source for every short substring used as a map key.
    let read = unsafe { Cx::current() };
    let source = value.as_str(&read);
    let ascii = value.is_ascii(&read);
    let len = if ascii {
        source.len()
    } else {
        source.chars().count()
    };
    let start = baml_type::sequence::resolve_slice_bound(start, len);
    let end = baml_type::sequence::resolve_slice_bound(end, len).max(start);
    let (start, end) = if ascii {
        (start, end)
    } else {
        let mut boundaries = source.char_indices().map(|(i, _)| i).chain([source.len()]);
        let first = boundaries.nth(start).unwrap();
        let last = if end == start {
            first
        } else {
            boundaries.nth(end - start - 1).unwrap()
        };
        (first, last)
    };
    // The non-moving heap keeps the borrowed input stable across allocation.
    cx.string(&source[start..end])
}

pub fn integer_to_string(cx: &mut Cx, value: i64) -> R<Str> {
    cx.string(&value.to_string())
}

/// Heap-neutral code-point extraction. Container glue builds the typed int list.
pub fn code_points(cx: &Cx, value: Str) -> Vec<i64> {
    value
        .as_str(cx)
        .chars()
        .map(|c| i64::from(u32::from(c)))
        .collect()
}

/// # Safety
/// `desc` and `desc+1` describe List<i64> and its buffer in the active instance.
pub unsafe fn to_code_points(cx: &mut Cx, value: Str, desc: u32) -> R<usize> {
    let values = code_points(cx, value);
    let list = unsafe { crate::lists::List::<i64>::with_capacity(cx, desc, values.len()) }?;
    for value in values {
        list.push(cx, value)?;
    }
    Ok(list.as_raw())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Instance, language::*};
    static DESCRIPTORS: &[baml_rt_gc::Descriptor] = &[
        STRING_DESC,
        OVERFLOW_DESC,
        SHIFT_DESC,
        CANCEL_DESC,
        STACK_DESC,
        ALLOC_DESC,
    ];
    #[test]
    fn unicode_negative_slices_and_concat_survive_collecting_allocations() {
        fn entry() -> R<i64> {
            let mut cx = unsafe { Cx::current() };
            let value = cx.string("aé🦀z")?;
            assert_eq!(length(&cx, value), 4);
            assert_eq!(code_points(&cx, value), [97, 233, 129408, 122]);
            for text in ["aé🦀z", "abcd", ""] {
                let source = cx.string(text)?;
                // Also exercise strings created by other runtime constructors,
                // which leave the optional ASCII cache uninitialized.
                unsafe {
                    (*(source.as_raw() as *mut baml_rt_gc::Header)).aux = 0;
                }
                let len = text.chars().count();
                for start in [i64::MIN, -6, -4, -1, 0, 1, 4, 6, i64::MAX] {
                    for end in [i64::MIN, -6, -4, -1, 0, 1, 4, 6, i64::MAX] {
                        let result = slice(&mut cx, source, start, end)?;
                        let first = baml_type::sequence::resolve_slice_bound(start, len);
                        let last = baml_type::sequence::resolve_slice_bound(end, len).max(first);
                        let expected: String =
                            text.chars().skip(first).take(last - first).collect();
                        assert_eq!(result.as_str(&cx), expected);
                        unsafe { crate::safepoint() }.unwrap();
                    }
                }
            }
            let number = integer_to_string(&mut cx, -42)?;
            let result = concat(&mut cx, &[value, number])?;
            assert_eq!(result.as_str(&cx), "aé🦀z-42");
            Ok(4)
        }
        let mut instance = Instance::new(
            DESCRIPTORS,
            &[false, false, false, true, true, true, true, true, true],
        );
        instance.set_stress(true);
        assert_eq!(unsafe { instance.run_baml(entry) }.unwrap(), 4);
        assert!(instance.stats().collections >= 81);
    }
}

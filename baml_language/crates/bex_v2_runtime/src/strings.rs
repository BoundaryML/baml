//! Flat-string building blocks for typed v2 builtin glue.
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
    pub unsafe fn current() -> Self {
        assert!(!crate::HEAP.get().is_null(), "builtin outside a root task");
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
    value.as_str(cx).chars().count() as i64
}

pub fn concat(cx: &mut Cx, values: &[Str]) -> R<Str> {
    let mut text = String::new();
    for value in values {
        text.push_str(value.as_str(cx));
    }
    cx.string(&text)
}

pub fn slice(cx: &mut Cx, value: Str, start: i64, end: i64) -> R<Str> {
    // Share the interpreter's BexStr Unicode slicing kernel and the exact
    // signed/clamped bounds helper. Only the final flat object is managed.
    let source = bex_str::BexStr::from(value.as_str(cx));
    let len = source.char_len();
    let start = baml_type::sequence::resolve_slice_bound(start, len);
    let end = baml_type::sequence::resolve_slice_bound(end, len).max(start);
    let text = source.char_slice(start..end);
    cx.string(text.as_str())
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Instance, language::*};
    static DESCRIPTORS: &[bex_v2_heap::Descriptor] = &[
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
            for start in [i64::MIN, -6, -4, -1, 0, 1, 4, 6, i64::MAX] {
                for end in [i64::MIN, -6, -4, -1, 0, 1, 4, 6, i64::MAX] {
                    let result = slice(&mut cx, value, start, end)?;
                    let first = baml_type::sequence::resolve_slice_bound(start, 4);
                    let last = baml_type::sequence::resolve_slice_bound(end, 4).max(first);
                    let expected: String = "aé🦀z".chars().skip(first).take(last - first).collect();
                    assert_eq!(result.as_str(&cx), expected);
                    unsafe { crate::safepoint() }.unwrap();
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

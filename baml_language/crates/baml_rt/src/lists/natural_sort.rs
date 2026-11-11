//! A native stable sort for primitive comparators which cannot enter BAML.
use super::*;
impl<T: Slot> List<T> {
    /// # Safety
    /// T has the same Rust and managed slot layout. `compare` must not allocate
    /// in the managed heap, reenter BAML, yield, or mutate this list. A Rust
    /// sort may temporarily hold pointer elements in unscanned scratch storage;
    /// no collection is allowed until they are all back in the managed buffer.
    pub unsafe fn sort_native(
        self,
        cx: &mut Cx,
        mut compare: impl FnMut(T, T) -> std::cmp::Ordering,
    ) -> R<Self> {
        cx.poll()?;
        let len = self.len(cx) as usize;
        if len > 1 {
            assert_eq!(std::mem::size_of::<T>(), T::SIZE);
            let object = self.pointer as *const Object;
            let buffer = unsafe { (*object).buffer };
            let values = unsafe { std::slice::from_raw_parts_mut((buffer + 16) as *mut T, len) };
            values.sort_by(|a, b| compare(*a, *b));
        }
        cx.poll()?;
        Ok(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::*;
    #[test]
    fn native_sort_keeps_aliases_stability_and_float_total_order_under_stress() {
        static DESCRIPTORS: &[baml_rt_gc::Descriptor] = &[
            STRING_DESC,
            OVERFLOW_DESC,
            SHIFT_DESC,
            CANCEL_DESC,
            STACK_DESC,
            ALLOC_DESC,
            INDEX_DESC,
            json::PARSE_DESC,
            json::DECODE_DESC,
            DIV_DESC,
            list_errors::SERIALIZATION_DESC,
            list_errors::INVALID_ARGUMENT_DESC,
            MAP_KEY_DESC,
            descriptor::<i64>(16),
            buffer_descriptor::<i64>(17),
            descriptor::<f64>(18),
            buffer_descriptor::<f64>(19),
            descriptor::<Ptr>(20),
            buffer_descriptor::<Ptr>(21),
        ];
        fn entry() -> R<i64> {
            let mut cx = unsafe { Cx::current() };
            let ints = unsafe { List::<i64>::new(&mut cx, 13) }?;
            for v in [3, -1, 2, 2] {
                ints.push(&mut cx, v)?;
            }
            let alias = ints;
            unsafe { ints.sort_native(&mut cx, |a, b| a.cmp(&b)) }?;
            assert_eq!(
                (0..4)
                    .map(|i| alias.get(&cx, i).unwrap())
                    .collect::<Vec<_>>(),
                [-1, 2, 2, 3]
            );
            let strings = unsafe { List::<Ptr>::new(&mut cx, 17) }?;
            let first = cx.string("a")?;
            let middle = cx.string("b")?;
            let second = cx.string("a")?;
            for v in [first, middle, second] {
                strings.push(&mut cx, unsafe { Ptr::from_raw(v.as_raw()) })?;
            }
            let read_cx = unsafe { Cx::current() };
            unsafe {
                strings.sort_native(&mut cx, |a, b| {
                    Str::from_raw(a.as_raw())
                        .as_str(&read_cx)
                        .cmp(Str::from_raw(b.as_raw()).as_str(&read_cx))
                })
            }?;
            assert_eq!(strings.get(&cx, 0)?.as_raw(), first.as_raw());
            assert_eq!(strings.get(&cx, 1)?.as_raw(), second.as_raw());
            let floats = unsafe { List::<f64>::new(&mut cx, 15) }?;
            for v in [f64::NAN, 0.0, -0.0, f64::NEG_INFINITY, 1.0] {
                floats.push(&mut cx, v)?;
            }
            unsafe { floats.sort_native(&mut cx, float_cmp) }?;
            assert_eq!(floats.get(&cx, 0)?, f64::NEG_INFINITY);
            assert_eq!(floats.get(&cx, 1)?.to_bits(), 0.0f64.to_bits());
            assert_eq!(floats.get(&cx, 2)?.to_bits(), (-0.0f64).to_bits());
            assert!(floats.get(&cx, 4)?.is_nan());
            Ok(1)
        }
        for stress in [false, true] {
            let mut instance = Instance::new(
                DESCRIPTORS,
                &[
                    false, false, false, true, true, true, true, true, true, true, true, true,
                    true, true, true, true, true, true, true, true, true, true,
                ],
            );
            instance.set_stress(stress);
            assert_eq!(unsafe { instance.run_baml(entry) }.unwrap(), 1);
        }
    }
}

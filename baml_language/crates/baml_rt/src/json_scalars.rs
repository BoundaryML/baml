//! Scalar conversion with the same signed 63-bit bounds as the VM.
use crate::{R, strings::Cx};
pub fn itrunc(cx: &mut Cx, value: f64) -> R<i64> {
    let value = value.trunc();
    if value.is_nan() {
        return Err(crate::list_errors::invalid_argument(
            cx,
            "float.itrunc: cannot convert NaN to int".into(),
        ));
    }
    if !(-4_611_686_018_427_387_904.0..4_611_686_018_427_387_904.0).contains(&value) {
        return Err(crate::list_errors::invalid_argument(
            cx,
            format!("float.itrunc: {value} is out of int range"),
        ));
    }
    Ok(value as i64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::*;
    #[test]
    fn truncation_keeps_vm_bounds_and_error_messages() {
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
        ];
        fn entry() -> R<i64> {
            let mut cx = unsafe { Cx::current() };
            assert_eq!(itrunc(&mut cx, -1.9)?, -1);
            assert_eq!(
                itrunc(&mut cx, 4_611_686_018_427_387_392.0)?,
                4_611_686_018_427_387_392
            );
            assert_eq!(
                itrunc(&mut cx, -4_611_686_018_427_387_904.0)?,
                -4_611_686_018_427_387_904
            );
            for value in [
                f64::NAN,
                f64::INFINITY,
                f64::NEG_INFINITY,
                4_611_686_018_427_387_904.0,
            ] {
                let error = itrunc(&mut cx, value).unwrap_err();
                assert_eq!(error.class, "baml.errors.InvalidArgument");
                assert_eq!(
                    error.message,
                    if value.is_nan() {
                        "float.itrunc: cannot convert NaN to int".into()
                    } else {
                        format!("float.itrunc: {value} is out of int range")
                    }
                );
            }
            Ok(1)
        }
        for stress in [false, true] {
            let mut instance = crate::Instance::new(
                DESCRIPTORS,
                &[
                    false, false, false, true, true, true, true, true, true, true, true, true,
                    true, true, true,
                ],
            );
            instance.set_stress(stress);
            assert_eq!(unsafe { instance.run_baml(entry) }.unwrap(), 1);
        }
    }
}

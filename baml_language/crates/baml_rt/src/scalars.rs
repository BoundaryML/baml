//! Scalar algorithms are shared with the interpreter, including error messages.
use crate::{Error, R};
use bex_vm_types::errors::{VmBamlError, VmRustFnError};
pub use bex_vm_types::scalar::{float, int};
pub use bex_vm_types::{bytecode::CmpOp, float_order::apply as compare};

/// Outside the descriptor tag range; a float's bits are never a precise root.
pub const FLOAT_TAG: u64 = u32::MAX as u64 - 1;

pub fn error(error: VmRustFnError) -> Box<Error> {
    match error {
        VmRustFnError::Panic(panic) => crate::language::from_panic(panic),
        VmRustFnError::BamlError(VmBamlError::InvalidArgument { message }) => {
            crate::language::panic_value(10, "baml.errors.InvalidArgument", message)
        }
        VmRustFnError::BamlError(VmBamlError::ParseError { message }) => {
            crate::language::panic_value(11, "baml.errors.ParseError", message)
        }
        error => Error::internal(error.to_string()),
    }
}

pub fn primitive_string(value: crate::Dyn) -> R<crate::strings::Str> {
    let text = match value.tag {
        0 => value.payload.cast_signed().to_string(),
        1 => (value.payload != 0).to_string(),
        2 => "null".into(),
        crate::native::BIGINT_TAG => {
            unsafe { crate::native::bigint_owned(value.payload as usize) }.to_string()
        }
        FLOAT_TAG => bex_vm_types::format_float(f64::from_bits(value.payload)),
        _ => return Err(Error::internal("expected a primitive value".into())),
    };
    unsafe { crate::strings::Cx::current() }.string(&text)
}

#[inline(always)]
pub fn neg(value: i64) -> R<i64> {
    if value == baml_type::Int63::MIN.get() {
        Err(crate::language::from_panic(
            bex_vm_types::errors::VmPanic::IntegerOverflow {
                message: format!("-({value}) overflows int"),
            },
        ))
    } else {
        Ok(-value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn negation_preserves_i63_overflow_class_and_message() {
        static D: &[baml_rt_gc::Descriptor] = &[crate::STRING_DESC, crate::OVERFLOW_DESC];
        let mut instance = crate::Instance::new(D, &[false, false, false, true, true]);
        let value = unsafe {
            instance.run_owned(|| {
                assert_eq!(neg(42)?, -42);
                let error = neg(baml_type::Int63::MIN.get()).unwrap_err();
                Ok((error.class, error.message.clone()))
            })
        }
        .unwrap();
        assert_eq!(value.0, "baml.panics.IntegerOverflow");
        assert_eq!(value.1, "-(-4611686018427387904) overflows int");
    }
}

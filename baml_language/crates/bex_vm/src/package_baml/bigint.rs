use std::sync::Arc;

use bex_vm_types::Value;
use num_bigint::BigInt;

use super::{BamlClassBigint, PackageBamlImpl};
use crate::errors::{VmBamlError, VmRustFnError};

impl BamlClassBigint for PackageBamlImpl {
    fn to_json(_bigint: &Value) -> Value {
        unimplemented!("bigint.to_json: not yet implemented (Phase 4)")
    }

    fn abs(_bigint: &Value) -> Arc<BigInt> {
        unimplemented!("bigint.abs: not yet implemented (Phase 4)")
    }

    fn min(_bigint: &Value, _other: Arc<BigInt>) -> Arc<BigInt> {
        unimplemented!("bigint.min: not yet implemented (Phase 4)")
    }

    fn max(_bigint: &Value, _other: Arc<BigInt>) -> Arc<BigInt> {
        unimplemented!("bigint.max: not yet implemented (Phase 4)")
    }

    fn clamp(_bigint: &Value, _min: Arc<BigInt>, _max: Arc<BigInt>) -> Arc<BigInt> {
        unimplemented!("bigint.clamp: not yet implemented (Phase 4)")
    }

    fn isqrt(_bigint: &Value) -> Result<Arc<BigInt>, VmRustFnError> {
        unimplemented!("bigint.isqrt: not yet implemented (Phase 4)")
    }

    fn pow(_bigint: &Value, _exp: Arc<BigInt>) -> Arc<BigInt> {
        unimplemented!("bigint.pow: not yet implemented (Phase 4)")
    }

    fn ilog(_bigint: &Value, _base: Arc<BigInt>) -> Result<Arc<BigInt>, VmRustFnError> {
        unimplemented!("bigint.ilog: not yet implemented (Phase 4)")
    }

    fn parse(text: &str) -> Result<Arc<BigInt>, VmRustFnError> {
        // Accept an optional leading sign followed by ASCII digits, matching the
        // documented behaviour: no whitespace, no underscores, no other formats.
        let (sign_str, digits) = if let Some(rest) = text.strip_prefix('-') {
            ("-", rest)
        } else if let Some(rest) = text.strip_prefix('+') {
            ("", rest)
        } else {
            ("", text)
        };

        // Reject empty string or non-digit chars after the sign.
        if digits.is_empty() || !digits.chars().all(|c| c.is_ascii_digit()) {
            return Err(VmBamlError::ParseError {
                message: format!("bigint.parse: cannot parse {text:?} as bigint"),
            }
            .into());
        }

        let full = format!("{sign_str}{digits}");
        BigInt::parse_bytes(full.as_bytes(), 10)
            .map(Arc::new)
            .ok_or_else(|| {
                VmBamlError::ParseError {
                    message: format!("bigint.parse: cannot parse {text:?} as bigint"),
                }
                .into()
            })
    }

    fn random(_lower: Arc<BigInt>, _upper: Arc<BigInt>) -> Result<Arc<BigInt>, VmRustFnError> {
        unimplemented!("bigint.random: not yet implemented (Phase 4)")
    }
}

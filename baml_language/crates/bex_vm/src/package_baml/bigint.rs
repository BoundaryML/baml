use std::sync::Arc;

/// Re-export of the shared `bigint` allocation cap. Defined in `baml_type`
/// so TIR's constant-folder can refuse to fold bigint expressions that the
/// VM would refuse to allocate.
pub(crate) use baml_type::MAX_BIGINT_BITS;
use bex_str::BexStr;
use num_bigint::BigInt;

use super::{BamlClassBigint, PackageBamlImpl};
use crate::errors::{VmPanic, VmRustFnError};

impl BamlClassBigint for PackageBamlImpl {
    fn abs(value: Arc<BigInt>) -> Arc<BigInt> {
        Arc::new(bex_vm_types::bigint::abs((*value).clone()))
    }
    fn isqrt(value: Arc<BigInt>) -> Result<Arc<BigInt>, VmRustFnError> {
        bex_vm_types::bigint::isqrt((*value).clone()).map(Arc::new)
    }
    fn pow(value: Arc<BigInt>, exp: Arc<BigInt>) -> Result<Arc<BigInt>, VmRustFnError> {
        bex_vm_types::bigint::pow((*value).clone(), (*exp).clone()).map(Arc::new)
    }
    fn ilog(value: Arc<BigInt>, base: Arc<BigInt>) -> Result<Arc<BigInt>, VmRustFnError> {
        bex_vm_types::bigint::ilog((*value).clone(), (*base).clone()).map(Arc::new)
    }
    fn to_int(value: Arc<BigInt>) -> Result<i64, VmRustFnError> {
        bex_vm_types::bigint::to_int((*value).clone())
    }
    fn parse(text: &BexStr) -> Result<Arc<BigInt>, VmRustFnError> {
        bex_vm_types::bigint::parse(text).map(Arc::new)
    }
    fn _random_byte_count(lower: Arc<BigInt>, upper: Arc<BigInt>) -> i64 {
        bex_vm_types::bigint::_random_byte_count((*lower).clone(), (*upper).clone())
    }
    fn _random_in_range(draw: &[u8], lower: Arc<BigInt>, upper: Arc<BigInt>) -> Arc<BigInt> {
        Arc::new(bex_vm_types::bigint::_random_in_range(
            draw,
            (*lower).clone(),
            (*upper).clone(),
        ))
    }
}
pub(crate) fn alloc_failure_panic(message: String) -> VmRustFnError {
    VmPanic::AllocFailure { message }.into()
}

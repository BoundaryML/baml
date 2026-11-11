use super::{BamlClassInt, PackageBamlImpl};
use crate::errors::VmRustFnError;

impl BamlClassInt for PackageBamlImpl {
    fn abs(int: i64) -> Result<i64, VmRustFnError> {
        bex_vm_types::scalar::int::abs(int)
    }

    fn isqrt(int: i64) -> Result<i64, VmRustFnError> {
        bex_vm_types::scalar::int::isqrt(int)
    }

    fn pow(int: i64, exp: i64) -> i64 {
        bex_vm_types::scalar::int::pow(int, exp)
    }

    fn parse(text: &bex_str::BexStr) -> Result<i64, VmRustFnError> {
        bex_vm_types::scalar::int::parse(text.as_str())
    }

    fn _random_in_range(draw: i64, lower: i64, upper: i64) -> i64 {
        bex_vm_types::scalar::int::_random_in_range(draw, lower, upper)
    }

    fn ilog(int: i64, base: i64) -> Result<i64, VmRustFnError> {
        bex_vm_types::scalar::int::ilog(int, base)
    }

    fn leading_zeros(int: i64) -> i64 {
        bex_vm_types::scalar::int::leading_zeros(int)
    }

    fn leading_ones(int: i64) -> i64 {
        bex_vm_types::scalar::int::leading_ones(int)
    }

    fn trailing_zeros(int: i64) -> i64 {
        bex_vm_types::scalar::int::trailing_zeros(int)
    }

    fn trailing_ones(int: i64) -> i64 {
        bex_vm_types::scalar::int::trailing_ones(int)
    }

    fn count_zeros(int: i64) -> i64 {
        bex_vm_types::scalar::int::count_zeros(int)
    }

    fn count_ones(int: i64) -> i64 {
        bex_vm_types::scalar::int::count_ones(int)
    }
}

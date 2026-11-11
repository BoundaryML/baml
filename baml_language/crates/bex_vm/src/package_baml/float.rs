use super::{BamlClassFloat, PackageBamlImpl};
use crate::errors::VmRustFnError;

impl BamlClassFloat for PackageBamlImpl {
    fn is_nan(float: f64) -> bool {
        bex_vm_types::scalar::float::is_nan(float)
    }

    fn is_infinite(float: f64) -> bool {
        bex_vm_types::scalar::float::is_infinite(float)
    }

    fn abs(float: f64) -> f64 {
        bex_vm_types::scalar::float::abs(float)
    }

    fn signum(float: f64) -> f64 {
        bex_vm_types::scalar::float::signum(float)
    }

    fn floor(float: f64) -> f64 {
        bex_vm_types::scalar::float::floor(float)
    }

    fn ceil(float: f64) -> f64 {
        bex_vm_types::scalar::float::ceil(float)
    }

    fn round(float: f64) -> f64 {
        bex_vm_types::scalar::float::round(float)
    }

    fn trunc(float: f64) -> f64 {
        bex_vm_types::scalar::float::trunc(float)
    }

    fn fract(float: f64) -> f64 {
        bex_vm_types::scalar::float::fract(float)
    }

    fn ifloor(float: f64) -> Result<i64, VmRustFnError> {
        bex_vm_types::scalar::float::ifloor(float)
    }

    fn iceil(float: f64) -> Result<i64, VmRustFnError> {
        bex_vm_types::scalar::float::iceil(float)
    }

    fn iround(float: f64) -> Result<i64, VmRustFnError> {
        bex_vm_types::scalar::float::iround(float)
    }

    fn itrunc(float: f64) -> Result<i64, VmRustFnError> {
        bex_vm_types::scalar::float::itrunc(float)
    }

    fn sqrt(float: f64) -> f64 {
        bex_vm_types::scalar::float::sqrt(float)
    }

    fn pow(float: f64, exp: f64) -> f64 {
        bex_vm_types::scalar::float::pow(float, exp)
    }

    fn log(float: f64, base: f64) -> f64 {
        bex_vm_types::scalar::float::log(float, base)
    }

    fn log10(float: f64) -> f64 {
        bex_vm_types::scalar::float::log10(float)
    }

    fn cbrt(float: f64) -> f64 {
        bex_vm_types::scalar::float::cbrt(float)
    }

    fn hypot(float: f64, other: f64) -> f64 {
        bex_vm_types::scalar::float::hypot(float, other)
    }

    fn sin(float: f64) -> f64 {
        bex_vm_types::scalar::float::sin(float)
    }

    fn cos(float: f64) -> f64 {
        bex_vm_types::scalar::float::cos(float)
    }

    fn tan(float: f64) -> f64 {
        bex_vm_types::scalar::float::tan(float)
    }

    fn asin(float: f64) -> f64 {
        bex_vm_types::scalar::float::asin(float)
    }

    fn acos(float: f64) -> f64 {
        bex_vm_types::scalar::float::acos(float)
    }

    fn atan(float: f64) -> f64 {
        bex_vm_types::scalar::float::atan(float)
    }

    fn atan2(float: f64, other: f64) -> f64 {
        bex_vm_types::scalar::float::atan2(float, other)
    }

    fn sinh(float: f64) -> f64 {
        bex_vm_types::scalar::float::sinh(float)
    }

    fn cosh(float: f64) -> f64 {
        bex_vm_types::scalar::float::cosh(float)
    }

    fn tanh(float: f64) -> f64 {
        bex_vm_types::scalar::float::tanh(float)
    }

    fn asinh(float: f64) -> f64 {
        bex_vm_types::scalar::float::asinh(float)
    }

    fn acosh(float: f64) -> f64 {
        bex_vm_types::scalar::float::acosh(float)
    }

    fn atanh(float: f64) -> f64 {
        bex_vm_types::scalar::float::atanh(float)
    }

    fn parse(text: &bex_str::BexStr) -> Result<f64, VmRustFnError> {
        bex_vm_types::scalar::float::parse(text.as_str())
    }

    fn _unit_from_draw(draw: i64) -> f64 {
        bex_vm_types::scalar::float::_unit_from_draw(draw)
    }

    fn pi() -> f64 {
        bex_vm_types::scalar::float::pi()
    }

    fn e() -> f64 {
        bex_vm_types::scalar::float::e()
    }

    fn golden_ratio() -> f64 {
        bex_vm_types::scalar::float::golden_ratio()
    }

    fn nan() -> f64 {
        bex_vm_types::scalar::float::nan()
    }

    fn inf() -> f64 {
        bex_vm_types::scalar::float::inf()
    }

    fn max_finite() -> f64 {
        bex_vm_types::scalar::float::max_finite()
    }

    fn min_finite() -> f64 {
        bex_vm_types::scalar::float::min_finite()
    }

    fn epsilon() -> f64 {
        bex_vm_types::scalar::float::epsilon()
    }
}

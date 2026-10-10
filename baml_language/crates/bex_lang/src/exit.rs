//! `baml.sys.exit(code)` at the process boundary.

/// Narrow a `baml.sys.exit(code)` value (a BAML `int`) to the `i32` that
/// `std::process::exit` and C's `exit(int)` take, saturating at the `i32`
/// bounds.
pub fn clamp_exit_code(code: i64) -> i32 {
    i32::try_from(code).unwrap_or(if code < 0 { i32::MIN } else { i32::MAX })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamp_exit_code_saturates() {
        assert_eq!(clamp_exit_code(0), 0);
        assert_eq!(clamp_exit_code(i64::from(i32::MAX)), i32::MAX);
        assert_eq!(clamp_exit_code(i64::from(i32::MIN)), i32::MIN);
        assert_eq!(clamp_exit_code(i64::from(i32::MAX) + 1), i32::MAX);
        assert_eq!(clamp_exit_code(i64::from(i32::MIN) - 1), i32::MIN);
        assert_eq!(clamp_exit_code(i64::MAX), i32::MAX);
        assert_eq!(clamp_exit_code(i64::MIN), i32::MIN);
    }
}

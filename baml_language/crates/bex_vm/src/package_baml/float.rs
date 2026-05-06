use bex_vm_types::Value;

use super::{BamlClassFloat, PackageBamlImpl};

impl BamlClassFloat for PackageBamlImpl {
    fn to_json(float: f64) -> Value {
        Value::Float(float)
    }
    // ── Predicates ────────────────────────────────────────────────────────────

    fn is_nan(float: f64) -> bool {
        float.is_nan()
    }

    fn is_infinite(float: f64) -> bool {
        float.is_infinite()
    }

    // Note: `is_finite` is implemented directly in `float.baml` as
    // `!self.is_nan() && !self.is_infinite()` per the BEP — no Rust trampoline.

    // ── Constants ─────────────────────────────────────────────────────────────

    fn pi() -> f64 {
        std::f64::consts::PI
    }

    fn e() -> f64 {
        std::f64::consts::E
    }

    fn golden_ratio() -> f64 {
        // (1 + sqrt(5)) / 2, rounded to f64.
        1.618_033_988_749_895
    }

    fn nan() -> f64 {
        f64::NAN
    }

    fn inf() -> f64 {
        f64::INFINITY
    }
}

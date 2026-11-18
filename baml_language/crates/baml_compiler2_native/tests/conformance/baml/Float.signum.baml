function float_signum_positive() -> float { halves(5).signum() }
function float_signum_negative() -> float { (0.0 - halves(5)).signum() }
function float_signum_zero() -> float { halves(0).signum() }
function float_signum_negative_zero() -> float { (halves(0) * (0.0 - halves(2))).signum() }
function float_signum_nan() -> float { (0.0 - halves(2)).sqrt().signum() }

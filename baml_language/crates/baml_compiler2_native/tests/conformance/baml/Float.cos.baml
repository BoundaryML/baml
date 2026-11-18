function float_cos_zero() -> float { halves(0).cos() }
function float_cos_half() -> float { halves(1).cos() }
function float_cos_negative() -> float { (0.0 - halves(3)).cos() }
function float_cos_large() -> float { tens(6).cos() }
function float_cos_nan() -> float { (0.0 - halves(2)).sqrt().cos() }

function float_abs_positive() -> float { halves(5).abs() }
function float_abs_negative() -> float { (0.0 - halves(5)).abs() }
function float_abs_nan() -> float { (0.0 - halves(2)).sqrt().abs() }
function float_abs_infinite() -> float { (0.0 - tens(400)).abs() }

function float_sinh_zero() -> float { halves(0).sinh() }
function float_sinh_half() -> float { halves(1).sinh() }
function float_sinh_negative() -> float { (0.0 - halves(3)).sinh() }
function float_sinh_large() -> float { tens(6).sinh() }
function float_sinh_nan() -> float { (0.0 - halves(2)).sqrt().sinh() }

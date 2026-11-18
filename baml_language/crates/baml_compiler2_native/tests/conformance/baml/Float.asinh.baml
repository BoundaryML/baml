function float_asinh_zero() -> float { halves(0).asinh() }
function float_asinh_half() -> float { halves(1).asinh() }
function float_asinh_negative() -> float { (0.0 - halves(3)).asinh() }
function float_asinh_large() -> float { tens(6).asinh() }
function float_asinh_nan() -> float { (0.0 - halves(2)).sqrt().asinh() }

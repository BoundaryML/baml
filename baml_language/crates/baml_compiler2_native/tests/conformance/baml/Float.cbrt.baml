function float_cbrt_cube() -> float { tens(3).cbrt() }
function float_cbrt_negative() -> float { (0.0 - halves(16)).cbrt() }
function float_cbrt_inexact() -> float { halves(4).cbrt() }
function float_cbrt_zero() -> float { halves(0).cbrt() }

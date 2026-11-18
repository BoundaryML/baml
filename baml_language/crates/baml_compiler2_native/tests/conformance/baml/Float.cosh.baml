function float_cosh_zero() -> float { halves(0).cosh() }
function float_cosh_half() -> float { halves(1).cosh() }
function float_cosh_negative() -> float { (0.0 - halves(3)).cosh() }
function float_cosh_large() -> float { tens(6).cosh() }
function float_cosh_nan() -> float { (0.0 - halves(2)).sqrt().cosh() }

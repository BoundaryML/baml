function float_tan_zero() -> float { halves(0).tan() }
function float_tan_half() -> float { halves(1).tan() }
function float_tan_negative() -> float { (0.0 - halves(3)).tan() }
function float_tan_large() -> float { tens(6).tan() }
function float_tan_nan() -> float { (0.0 - halves(2)).sqrt().tan() }

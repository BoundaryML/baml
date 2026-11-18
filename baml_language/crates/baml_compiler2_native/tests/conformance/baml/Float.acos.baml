function float_acos_zero() -> float { halves(0).acos() }
function float_acos_half() -> float { halves(1).acos() }
function float_acos_negative() -> float { (0.0 - halves(1)).acos() }
function float_acos_one() -> float { halves(2).acos() }
function float_acos_outside() -> float { halves(4).acos() }

function float_atan_zero() -> float { halves(0).atan() }
function float_atan_half() -> float { halves(1).atan() }
function float_atan_negative() -> float { (0.0 - halves(3)).atan() }
function float_atan_large() -> float { tens(6).atan() }
function float_atan_nan() -> float { (0.0 - halves(2)).sqrt().atan() }

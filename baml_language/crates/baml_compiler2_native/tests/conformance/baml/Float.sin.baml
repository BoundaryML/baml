function float_sin_zero() -> float { halves(0).sin() }
function float_sin_half() -> float { halves(1).sin() }
function float_sin_negative() -> float { (0.0 - halves(3)).sin() }
function float_sin_large() -> float { tens(6).sin() }
function float_sin_nan() -> float { (0.0 - halves(2)).sqrt().sin() }

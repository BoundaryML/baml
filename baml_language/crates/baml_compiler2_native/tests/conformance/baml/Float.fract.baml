function float_fract_positive() -> float { halves(5).fract() }
function float_fract_negative() -> float { (0.0 - halves(5)).fract() }
function float_fract_whole() -> float { halves(6).fract() }
function float_fract_small() -> float { (0.0 - halves(1)).fract() }
function float_fract_nan() -> float { (0.0 - halves(2)).sqrt().fract() }
function float_fract_infinite() -> float { tens(400).fract() }

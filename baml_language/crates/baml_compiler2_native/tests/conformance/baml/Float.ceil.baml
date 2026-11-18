function float_ceil_positive() -> float { halves(5).ceil() }
function float_ceil_negative() -> float { (0.0 - halves(5)).ceil() }
function float_ceil_whole() -> float { halves(6).ceil() }
function float_ceil_small() -> float { (0.0 - halves(1)).ceil() }
function float_ceil_nan() -> float { (0.0 - halves(2)).sqrt().ceil() }
function float_ceil_infinite() -> float { tens(400).ceil() }

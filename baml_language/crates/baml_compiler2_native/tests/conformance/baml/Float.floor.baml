function float_floor_positive() -> float { halves(5).floor() }
function float_floor_negative() -> float { (0.0 - halves(5)).floor() }
function float_floor_whole() -> float { halves(6).floor() }
function float_floor_small() -> float { (0.0 - halves(1)).floor() }
function float_floor_nan() -> float { (0.0 - halves(2)).sqrt().floor() }
function float_floor_infinite() -> float { tens(400).floor() }

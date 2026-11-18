function float_hypot_triple() -> float { halves(6).hypot(halves(8)) }
function float_hypot_negative() -> float { (0.0 - halves(6)).hypot(halves(8)) }
function float_hypot_large() -> float { tens(200).hypot(tens(200)) }
function float_hypot_nan() -> float { (0.0 - halves(2)).sqrt().hypot(halves(2)) }
function float_hypot_infinite_and_nan() -> float { tens(400).hypot((0.0 - halves(2)).sqrt()) }

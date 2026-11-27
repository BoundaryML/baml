function float_round_positive() -> float { halves(5).round() }
function float_round_negative() -> float { (0.0 - halves(5)).round() }
function float_round_whole() -> float { halves(6).round() }
function float_round_small() -> float { (0.0 - halves(1)).round() }
function float_round_nan() -> float { (0.0 - halves(2)).sqrt().round() }
function float_round_infinite() -> float { tens(400).round() }

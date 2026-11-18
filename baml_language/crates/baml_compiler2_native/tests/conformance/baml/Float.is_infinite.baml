function float_is_infinite_number() -> bool { halves(3).is_infinite() }
function float_is_infinite_nan() -> bool { (0.0 - halves(2)).sqrt().is_infinite() }
function float_is_infinite_positive() -> bool { tens(400).is_infinite() }
function float_is_infinite_negative() -> bool { (0.0 - tens(400)).is_infinite() }

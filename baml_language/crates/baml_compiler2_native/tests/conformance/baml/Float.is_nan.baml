function float_is_nan_number() -> bool { halves(3).is_nan() }
function float_is_nan_nan() -> bool { (0.0 - halves(2)).sqrt().is_nan() }
function float_is_nan_infinite() -> bool { tens(400).is_nan() }

function float_trunc_positive() -> float { halves(5).trunc() }
function float_trunc_negative() -> float { (0.0 - halves(5)).trunc() }
function float_trunc_whole() -> float { halves(6).trunc() }
function float_trunc_small() -> float { (0.0 - halves(1)).trunc() }
function float_trunc_nan() -> float { (0.0 - halves(2)).sqrt().trunc() }
function float_trunc_infinite() -> float { tens(400).trunc() }

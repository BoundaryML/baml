function float_itrunc_positive() -> int { halves(11).itrunc() }
function float_itrunc_negative() -> int { (0.0 - halves(11)).itrunc() }
function float_itrunc_large() -> int { tens(15).itrunc() }
function float_itrunc_nan() -> int { (0.0 - halves(2)).sqrt().itrunc() }
function float_itrunc_out_of_range() -> int { tens(19).itrunc() }
function float_itrunc_infinite() -> int { tens(400).itrunc() }
function float_itrunc_one_past_largest() -> int { halves(4).pow(halves(124)).itrunc() }
function float_itrunc_smallest() -> int { (0.0 - halves(4).pow(halves(124))).itrunc() }

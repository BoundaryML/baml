function float_itrunc_positive() -> int { halves(11).itrunc() }
function float_itrunc_negative() -> int { (0.0 - halves(11)).itrunc() }
function float_itrunc_large() -> int { tens(15).itrunc() }
function float_itrunc_nan() -> int { (0.0 - halves(2)).sqrt().itrunc() }
function float_itrunc_out_of_range() -> int { tens(19).itrunc() }

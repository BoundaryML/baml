function float_iround_positive() -> int { halves(5).iround() }
function float_iround_negative() -> int { (0.0 - halves(5)).iround() }
function float_iround_half() -> int { (0.0 - halves(1)).iround() }
function float_iround_nan() -> int { (0.0 - halves(2)).sqrt().iround() }
function float_iround_out_of_range() -> int { tens(19).iround() }
function float_iround_infinite() -> int { (0.0 - tens(400)).iround() }
function float_iround_one_past_largest() -> int { halves(4).pow(halves(124)).iround() }
function float_iround_smallest() -> int { (0.0 - halves(4).pow(halves(124))).iround() }

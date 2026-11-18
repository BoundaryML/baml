function float_ifloor_positive() -> int { halves(5).ifloor() }
function float_ifloor_negative() -> int { (0.0 - halves(5)).ifloor() }
function float_ifloor_half() -> int { (0.0 - halves(1)).ifloor() }
function float_ifloor_nan() -> int { (0.0 - halves(2)).sqrt().ifloor() }
function float_ifloor_out_of_range() -> int { tens(19).ifloor() }
function float_ifloor_infinite() -> int { (0.0 - tens(400)).ifloor() }
function float_ifloor_one_past_largest() -> int { halves(4).pow(halves(124)).ifloor() }
function float_ifloor_smallest() -> int { (0.0 - halves(4).pow(halves(124))).ifloor() }

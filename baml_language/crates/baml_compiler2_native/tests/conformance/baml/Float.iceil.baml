function float_iceil_positive() -> int { halves(5).iceil() }
function float_iceil_negative() -> int { (0.0 - halves(5)).iceil() }
function float_iceil_half() -> int { (0.0 - halves(1)).iceil() }
function float_iceil_nan() -> int { (0.0 - halves(2)).sqrt().iceil() }
function float_iceil_out_of_range() -> int { tens(19).iceil() }
function float_iceil_infinite() -> int { (0.0 - tens(400)).iceil() }
function float_iceil_one_past_largest() -> int { halves(4).pow(halves(124)).iceil() }
function float_iceil_smallest() -> int { (0.0 - halves(4).pow(halves(124))).iceil() }

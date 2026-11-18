function float_atan2_first() -> float { halves(2).atan2(halves(2)) }
function float_atan2_second() -> float { halves(2).atan2((0.0 - halves(2))) }
function float_atan2_third() -> float { (0.0 - halves(2)).atan2((0.0 - halves(2))) }
function float_atan2_zeros() -> float { halves(0).atan2(halves(0)) }
function float_atan2_vertical() -> float { halves(3).atan2(halves(0)) }

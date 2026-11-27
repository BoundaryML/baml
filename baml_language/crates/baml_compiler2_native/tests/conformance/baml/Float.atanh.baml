function float_atanh_zero() -> float { halves(0).atanh() }
function float_atanh_half() -> float { halves(1).atanh() }
function float_atanh_negative() -> float { (0.0 - halves(1)).atanh() }
function float_atanh_one() -> float { halves(2).atanh() }
function float_atanh_outside() -> float { halves(4).atanh() }

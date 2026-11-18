function float_pow_whole() -> float { halves(4).pow(halves(20)) }
function float_pow_root() -> float { halves(4).pow(halves(1)) }
function float_pow_negative_base() -> float { (0.0 - halves(16)).pow(halves(1)) }
function float_pow_zero_to_zero() -> float { halves(0).pow(halves(0)) }
function float_pow_negative_exponent() -> float { halves(4).pow((0.0 - halves(4))) }
function float_pow_overflow() -> float { tens(200).pow(halves(4)) }

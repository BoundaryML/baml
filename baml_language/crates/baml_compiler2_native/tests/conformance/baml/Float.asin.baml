function float_asin_zero() -> float { halves(0).asin() }
function float_asin_half() -> float { halves(1).asin() }
function float_asin_negative() -> float { (0.0 - halves(1)).asin() }
function float_asin_one() -> float { halves(2).asin() }
function float_asin_outside() -> float { halves(4).asin() }

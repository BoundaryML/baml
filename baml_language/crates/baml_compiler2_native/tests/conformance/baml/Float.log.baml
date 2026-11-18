function float_log_exact() -> float { halves(16).log(halves(4)) }
function float_log_inexact() -> float { halves(20).log(halves(6)) }
function float_log_negative() -> float { (0.0 - halves(2)).log(halves(4)) }
function float_log_zero() -> float { halves(0).log(halves(4)) }
function float_log_base_one() -> float { halves(4).log(halves(2)) }

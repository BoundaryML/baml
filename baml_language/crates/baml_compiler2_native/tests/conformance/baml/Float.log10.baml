function float_log10_exact() -> float { tens(3).log10() }
function float_log10_inexact() -> float { halves(5).log10() }
function float_log10_zero() -> float { halves(0).log10() }
function float_log10_negative() -> float { (0.0 - halves(2)).log10() }

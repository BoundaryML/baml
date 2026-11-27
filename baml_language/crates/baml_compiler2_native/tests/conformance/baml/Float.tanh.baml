function float_tanh_zero() -> float { halves(0).tanh() }
function float_tanh_half() -> float { halves(1).tanh() }
function float_tanh_negative() -> float { (0.0 - halves(3)).tanh() }
function float_tanh_large() -> float { tens(6).tanh() }
function float_tanh_nan() -> float { (0.0 - halves(2)).sqrt().tanh() }

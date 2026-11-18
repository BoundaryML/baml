function trunc_to_int_positive() -> int { baml._trunc_to_int(halves(5)) }
function trunc_to_int_negative() -> int { baml._trunc_to_int(0.0 - halves(5)) }
function trunc_to_int_nan() -> int { baml._trunc_to_int((0.0 - halves(2)).sqrt()) }
function trunc_to_int_large() -> int { baml._trunc_to_int(tens(15)) }
// native prints: 4611686018427387904
function trunc_to_int_past_63_bits() -> int { baml._trunc_to_int(halves(4).pow(halves(124))) }
// native prints: 9223372036854775807
function trunc_to_int_saturates() -> int { baml._trunc_to_int(tens(30)) }
// native prints: -9223372036854775808
function trunc_to_int_negative_infinity() -> int { baml._trunc_to_int(0.0 - tens(400)) }

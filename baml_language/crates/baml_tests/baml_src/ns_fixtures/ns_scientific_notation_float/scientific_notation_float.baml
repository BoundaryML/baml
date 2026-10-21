// Phase 3 snapshot test: scientific-notation float literals.
//
// Exercises the lexer, parser, HIR, TIR, MIR, and codegen stages for floats
// written in exponent form. Before the lexer learned scientific notation,
// `1e10` lexed as the integer `1` followed by the identifier `e10`, producing
// a misleading "unresolved name: e10" error.

function IntegerMantissa() -> float {
    1e10
}

function UppercaseExponent() -> float {
    1E3
}

function NegativeExponent() -> float {
    1e-3
}

function PositiveExponent() -> float {
    2e+5
}

function DecimalMantissa() -> float {
    1.5e3
}

function DecimalMantissaNegativeExponent() -> float {
    1.5e-3
}

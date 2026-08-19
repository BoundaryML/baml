// Phase 3 snapshot test: `42n` bigint literal syntax.
// Exercises the lexer, parser, HIR, TIR, MIR, and codegen stages for bigint
// literals in both expression and type-annotation positions.

function GetBigint() -> bigint {
    let x: bigint = 42n;
    x
}

function GetLargeBigint() -> bigint {
    99999999999999999999n
}

function GetNegativeBigint() -> bigint {
    -7n
}

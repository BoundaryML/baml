// Regression tests: numeric invariance still permits the legitimate cases.

// Explicit bigint literals satisfy a `bigint[]` slot.
function BigintLiteralArray() -> bigint[] {
    [1n, 2n]
}

// An empty array literal is assignable to any typed list.
function EmptyBigintArray() -> bigint[] {
    []
}

// An empty evolving list is retyped by its first typed use.
function EvolvingRetype() -> bigint[] {
    let a = [];
    a
}

// `int[]` is assignable to `int[]` (reflexive).
function IntArrayIdentity() -> int[] {
    let a = [1, 2];
    a
}

// A map literal with bigint values satisfies a `map<string, bigint>` slot.
function BigintMapLiteral() -> map<string, bigint> {
    {"a": 1n}
}

// A class literal with bigint field literals satisfies the class type.
class BigintCounter { count bigint }
function BigintFieldOk() -> BigintCounter {
    BigintCounter { count: 1n }
}

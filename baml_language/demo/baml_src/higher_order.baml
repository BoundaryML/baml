// ── Functions as Arguments (Higher-Order) ───────────────────────

function Apply(f: (x: int) -> int, x: int) -> int {
    return f(x);
}

function ApplyTwice(f: (x: int) -> int, x: int) -> int {
    return Apply(f, Apply(f, x));
}

// pass a concrete function
function UseApply() -> int {
    return Apply(Add1, 10);
}

function UseApplyTwice() -> int {
    return ApplyTwice(Add1, 10);
}

// wrong argument type to higher-order function → error
function BadApply() -> int {
    return Apply(IntToString, 10);
}

// ── Functions Returning Functions ────────────────────────────────

function MakeAdder(n: int) -> (x: int) -> int {
    Add1
}

function UseMakeAdder() -> int {
    let f = MakeAdder(5);
    return f(10);
}

// call a returned function immediately
function ImmediateCall() -> int {
    return MakeAdder(5)(10);
}

// ── Compose ─────────────────────────────────────────────────────

function Compose(
    f: (x: int) -> int,
    g: (x: int) -> int,
    x: int
) -> int {
    return f(g(x));
}

function UseCompose() -> int {
    return Compose(Add1, Add2, 0);
}

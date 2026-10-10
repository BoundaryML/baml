// Native backend fixture: parameters with constant defaults.

function scale(x: int, by: int = 3, offset: int = -1) -> int {
    x * by + offset
}

function label(name: string, greeting: string = "hi", loud: bool = false) -> string {
    let text = greeting + ", " + name;
    if (loud) { text + "!" } else { text }
}

function halve(x: float, by: float = 2.0) -> float {
    x / by
}

function capped(n: int, cap: int | null = null) -> int {
    if (cap == null) { n } else { n - 1 }
}

class Counter {
    n: int
    function bump(self, by: int = 1) -> int {
        self.n = self.n + by;
        self.n
    }
}

function use_defaults(n: int) -> int {
    scale(n) + scale(n, by = 2) + scale(n, offset = 10) + scale(n, by = n, offset = n)
}

function use_named(n: int) -> string {
    label("a") + label("b", loud = true) + label("c", greeting = "yo") + capped(n).to_string()
        + capped(n, cap = 0).to_string()
}

function use_float(x: float) -> float {
    halve(x) + halve(x, by = 4.0)
}

function use_method(k: int) -> int {
    let c = Counter { n: k };
    c.bump();
    c.bump(by = 10);
    c.bump()
}

// Computed defaults: the callee's prologue fills them, from the other
// parameters, after the call, so a throwing default throws in the callee.
function grow(a: int, b: int = a + 1, c: int = b * 2) -> int {
    a + b + c
}

function risky(a: int, b: int = 10 / a) -> int {
    a + b
}

function captured_default(n: int, step: int = n * 3) -> int {
    let bump = () -> int { step = step + 1; step };
    bump() + bump()
}

function use_computed(n: int) -> int {
    grow(n) + grow(n, b = 1) + grow(n, c = 5) + grow(n, b = 2, c = 3)
        + captured_default(n) + captured_default(n, step = 100)
}

function use_risky(n: int) -> int {
    risky(n) + risky(n, b = 7)
}

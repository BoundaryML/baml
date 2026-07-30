// Local bindings. Every introducer (`let`, `const`, for-in, match arm,
// destructure shorthand, `catch`) funnels through the same pattern-lowering
// sites, so one occurrence of each shape pins the whole set.

function boom() -> int {
    throw "boom";
}

function let_binding() -> int throws never {
    let try = 1;
    try
}

// `const` is both an introducer (a warning: it behaves like `let` today) and a
// reserved name, so the introducer keyword is fine and the binding name is not.
function const_binding() -> int throws never {
    const pub = 1;
    pub
}

// Destructure shorthand binds a local named after the field, so it reports at
// the shorthand too — on top of the field declaration in `decls.baml`.
function destructure_shorthand(d: do) -> int throws never {
    let do { readonly } = d;
    readonly
}

function for_in_binding(items: int[]) -> int throws never {
    let total = 0;
    for (let yield in items) {
        total += yield;
    }
    total
}

function match_arm_binding(v: int | string) -> int throws never {
    match (v) {
        let do: int => do,
        _ => 0,
    }
}

function catch_bindings() -> int throws never {
    boom() catch (unsafe, virtual) {
        _ => 0,
    }
}

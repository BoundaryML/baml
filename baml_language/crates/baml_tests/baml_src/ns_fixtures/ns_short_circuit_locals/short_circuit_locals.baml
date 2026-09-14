// Bindings whose value the emitter may leave on the operand stack instead of
// storing to a slot. Each function pins one shape of the predecessor-coverage
// rule in baml_compiler2_emit::analysis::is_stack_covered_phi, so the bytecode
// snapshot is the point of these — a store_var/load_var pair appearing or
// disappearing here is a classification change.

// Every edge into the use of `ok` pushes its value: the two ShortCircuit
// terminators, and the trailing assignment in the innermost rhs block. Stack
// carried, so `ok` gets no store_var/load_var.
function chained_and(a: bool, b: bool, c: bool) -> bool {
    let ok = a && b && c;
    ok
}

// The short circuit's join is merged into the `if` join, which is also the use
// block. The false edge of the `if` still reaches that block without pushing
// anything, and `let x = false` defines `x` a second time, so `x` needs a slot.
function conditional_short_circuit(a: bool, b: bool, cond: bool) -> bool {
    let x = false;
    if (cond) {
        x = a && b
    }
    x
}

// The same shape inside a loop, where stack-carrying `x` left one extra value
// on the operand stack per iteration.
function conditional_short_circuit_in_loop(items: int[], a: bool, b: bool) -> int {
    let x = false;
    let hits = 0;
    for (let i in items) {
        if (i > 0) {
            x = a && b
        }
        if (x) {
            hits = hits + 1
        }
    }
    hits
}

// Both arms assign `x` and fall through to its use, but `let x = 0` defines it
// once more outside them. Stack-carrying `x` pushed that initializer every
// iteration with nothing to pop it.
function initialized_local_in_branch_loop(items: int[]) -> int {
    let total = 0;
    for (let i in items) {
        let x = 0;
        if (i > 0) {
            x = 1
        } else {
            x = 2
        }
        total = x + total
    }
    total
}

// One predecessor of the use is a ShortCircuit joining there, the other a plain
// assignment: a mix that neither half of the old rule accepted on its own.
function mixed_join(a: bool, b: bool, cond: bool) -> bool {
    let x = if (cond) {
        a && b
    } else {
        false
    };
    x
}

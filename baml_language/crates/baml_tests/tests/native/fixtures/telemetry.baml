// Native backend fixture: call trees for telemetry. `native_telemetry.rs`
// records these on the VM and natively and compares the recordings; the
// module is also emitted with every function observed (`telemetry_all`).

class Failure { code: int }

// A one-line leaf: folded.
function leaf(x: int) -> int { x + 1 }

// A loop: observed.
function work(n: int) -> int {
    let total = 0;
    let i = 0;
    while (i < n) {
        total += leaf(i);
        i += 1;
    }
    total
}

// Folded, between two observed functions.
function middle(x: int) -> int { work(x) + leaf(x) }

// Folded, and throws.
function fails(x: int) -> int {
    if (x > 2) { throw Failure { code: x } }
    x
}

// Folded, and panics past the end of its array.
function out_of_range(x: int) -> int {
    let xs = [1, 2];
    xs[x]
}

// An observed loop reaching observed `work` directly from two sites,
// through folded `middle`, through a callback, and catching what folded
// `fails` throws.
function run(n: int) -> int {
    let total = 0;
    let i = 0;
    while (i < n) {
        total += middle(i);
        total += work(i) + work(i + 1);
        let caught = fails(i) catch (e) { let f: Failure => f.code, };
        total += caught;
        i += 1;
    }
    let mapped = [1, 2, 3].map((x: int) -> int { work(x) });
    total + mapped.length()
}

// An observed call, then a panic out of a folded one.
function run_panics(n: int) -> int { work(n) + out_of_range(n) }

// Direct self-recursion of an observed function.
function recurse(n: int) -> int {
    let total = 0;
    for (let i in [1, 2]) { total += i; }
    if (n <= 0) { total } else { total + recurse(n - 1) }
}

// Native backend fixture: self and mutual recursion, and the depth limit.

function fact(n: int) -> int { if (n <= 1) { 1 } else { n * fact(n - 1) } }

function is_even(n: int) -> bool { if (n == 0) { true } else { is_odd(n - 1) } }

function is_odd(n: int) -> bool { if (n == 0) { false } else { is_even(n - 1) } }

function deep(n: int) -> int { if (n == 0) { 0 } else { deep(n - 1) + 1 } }

function fib(n: int) -> int { if (n < 2) { n } else { fib(n - 1) + fib(n - 2) } }

// The frame limit is the VM's exactly: every frame counts, a stdlib method
// with a BAML body is a frame of its own, and the limit is checked before a
// Rust builtin that pushes none. Each bottom reaches the limit through one
// builtin; `start` adds a frame that is on no cycle.
function start(n: int) -> int { deep(n) }

function deep_length(n: int) -> int {
    if (n == 0) { [1, 2].length() } else { deep_length(n - 1) + 1 }
}

function deep_str_length(n: int) -> int {
    if (n == 0) { "ab".length() } else { deep_str_length(n - 1) + 1 }
}

function deep_for_in(n: int) -> int {
    if (n == 0) {
        let s = 0;
        for (let x in [1, 2]) { s = s + x; }
        s
    } else {
        deep_for_in(n - 1) + 1
    }
}

function deep_map(n: int) -> int {
    if (n == 0) { [1, 2].map((x: int) -> int { x * 2 }).length() } else { deep_map(n - 1) + 1 }
}

function deep_for_each(n: int) -> int {
    if (n == 0) {
        let acc = 0;
        [1, 2].for_each((x: int) -> void { acc = acc + x; });
        acc
    } else {
        deep_for_each(n - 1) + 1
    }
}

function deep_for_each_empty(n: int) -> int {
    if (n == 0) {
        let xs: int[] = [];
        xs.for_each((x: int) -> void {});
        0
    } else {
        deep_for_each_empty(n - 1) + 1
    }
}

function deep_sort_by_key(n: int) -> int {
    if (n == 0) { [2, 1].sort_by_key((x: int) -> int { x })[0] } else { deep_sort_by_key(n - 1) + 1 }
}

function deep_json(n: int) -> int {
    if (n == 0) { baml.json.to_string([1]).length() } else { deep_json(n - 1) + 1 }
}

function deep_panic(n: int) -> int {
    if (n == 0) { baml.sys.panic("bottom") } else { deep_panic(n - 1) + 1 }
}

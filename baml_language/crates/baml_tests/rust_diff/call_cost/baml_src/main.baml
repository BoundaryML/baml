function identity(n: int) -> int { n }

function bounded() -> int {
    let sum = 0;
    for (let i = -3; i < 8; i += 3) {
        for (let j = -2; j < 9; j += 2) { sum += i * j; };
        sum += identity(i);
    };
    sum
}

function overflowing_sum() -> int {
    let sum = 4611686018427387902;
    for (let i = 0; i < 3; i += 1) { sum += 1; };
    sum
}

function overflowing_counter() -> int {
    let sum = 0;
    for (let i = 0; i < 5; i += 1) {
        if i == 2 { i = 4611686018427387903; };
        sum += 1;
    };
    sum
}

function indirect_recurrence() -> int {
    let sum = 1;
    for (let i = 0; i < 80; i += 1) { let delta = sum; sum += delta; };
    sum
}

function reset_and_continue() -> int {
    let sum = 0;
    let resets = 0;
    for (let i = 0; i < 5; i += 1) {
        if i == 2 && resets == 0 { i = 0; resets = 1; };
        sum += i;
    };
    let i = 0;
    let again = true;
    while (i < 5) {
        if i == 2 && again { again = false; continue; };
        sum += i;
        i += 1;
    };
    sum
}

interface Reading {
    function read(self) -> int throws never
}
class Direct {
    n: int
    implements Reading {
        function read(self) -> int { self.n }
    }
}
class Doubled {
    n: int
    implements Reading {
        function read(self) -> int { self.n * 2 }
    }
}
function choose(i: int) -> Reading {
    if i % 2 == 0 { return Direct { n: i }; };
    Doubled { n: i }
}
function virtual_calls() -> int {
    let sum = 0;
    for (let i = 0; i < 8; i += 1) { sum += choose(i).read(); };
    sum
}
function fib(n: int) -> int {
    if n <= 1 { n } else { fib(n - 1) + fib(n - 2) }
}

function main() -> int {
    let a = overflowing_sum() catch_all_panics (e) { _ => 101, };
    let b = overflowing_counter() catch_all_panics (e) { _ => 103, };
    let c = indirect_recurrence() catch_all_panics (e) { _ => 107, };
    bounded() + reset_and_continue() + virtual_calls() + fib(8) + a + b + c
}

function error_leaf(n: int) -> int { n + 1 }
function error_chain(depth: int) -> int {
    if depth > 0 { error_chain(depth - 1) } else { error_leaf(4611686018427387903) }
}
function uncaught() -> int { error_chain(4) }

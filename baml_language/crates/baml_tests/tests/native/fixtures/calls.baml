// Native backend fixture: direct calls between compiled functions and panics.

function square(x: int) -> int {
    x * x
}

function sum_of_squares(n: int) -> int {
    let total = 0;
    let i = 1;
    while (i <= n) {
        total = total + square(i);
        i = i + 1;
    }
    total
}

function checked_step(x: int) -> int {
    if (x < 0) {
        baml.sys.panic("negative input");
    }
    x + 1
}

function step_twice(x: int) -> int {
    checked_step(checked_step(x))
}

function is_even(x: int) -> bool {
    x % 2 == 0
}

function count_even(n: int) -> int {
    let count = 0;
    for (let i = 0; i < n; i += 1) {
        if (is_even(i)) {
            count = count + 1;
        }
    }
    count
}

function noop(x: int) -> void {
    let y = x + 1;
}

// Native backend fixture: loops with break, continue and early return.

function collatz_steps(n: int) -> int {
    let steps = 0;
    let x = n;
    while (x != 1) {
        if (x % 2 == 0) {
            x = x / 2;
        } else {
            x = 3 * x + 1;
        }
        steps = steps + 1;
    }
    steps
}

function sum_odd_squares(limit: int) -> int {
    let total = 0;
    for (let i = 0; i < 1000000; i += 1) {
        if (i % 2 == 0) {
            continue;
        }
        let sq = i * i;
        if (total + sq > limit) {
            break;
        }
        total = total + sq;
    }
    total
}

function gcd(a: int, b: int) -> int {
    let x = a;
    let y = b;
    while (y != 0) {
        let t = y;
        y = x % y;
        x = t;
    }
    x
}

function first_factor(n: int) -> int {
    let d = 2;
    while (d * d <= n) {
        if (n % d == 0) {
            return d;
        }
        d = d + 1;
    }
    n
}

function nested(n: int) -> int {
    let count = 0;
    let i = 0;
    while (i < n) {
        let j = 0;
        while (true) {
            if (j > i) {
                break;
            }
            if ((i + j) % 3 == 0) {
                j = j + 1;
                continue;
            }
            count = count + 1;
            j = j + 1;
        }
        i = i + 1;
    }
    count
}

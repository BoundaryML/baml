// Native backend fixture: integer arrays with identity semantics.

function sum(xs: int[]) -> int {
    let total = 0;
    for (let x in xs) {
        total = total + x;
    }
    total
}

function build(n: int) -> int {
    let xs: int[] = [];
    let i = 0;
    while (i < n) {
        xs.push(i * i);
        i = i + 1;
    }
    sum(xs) + xs.length()
}

function alias_mutation(n: int) -> int {
    let a = [1, 2, 3];
    let b = a;
    b[0] = n;
    a[1] = a[1] + b[0];
    a[0] + a[1] + a[2] + b[2]
}

function negative_index(n: int) -> int {
    let xs = [10, 20, 30];
    xs[n]
}

function bubble(n: int) -> int {
    let xs: int[] = [];
    let i = 0;
    while (i < n) {
        xs.push((n - i) * 7 % 11);
        i = i + 1;
    }
    let len = xs.length();
    let outer = 0;
    while (outer < len) {
        let inner = 0;
        while (inner + 1 < len - outer) {
            if (xs[inner] > xs[inner + 1]) {
                let t = xs[inner];
                xs[inner] = xs[inner + 1];
                xs[inner + 1] = t;
            }
            inner = inner + 1;
        }
        outer = outer + 1;
    }
    let acc = 0;
    for (let x in xs) {
        acc = acc * 13 + x;
    }
    acc
}

function push_during_iteration(n: int) -> int {
    let xs = [n];
    let seen = 0;
    for (let x in xs) {
        seen = seen + 1;
        if (xs.length() < 4) {
            xs.push(x + 1);
        }
    }
    seen
}

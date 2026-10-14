// Native backend fixture: `T | null` locals, returns, fields and elements.

class Node { value: int, next: Node | null }

function pick(n: int) -> int | null { if (n > 0) { n } else { null } }

function or_zero(n: int) -> int {
    let p = pick(n);
    if (p == null) { 0 } else { p }
}

function describe(n: int) -> string {
    let p = pick(n);
    p.to_string()
}

function build(n: int) -> Node | null {
    if (n == 0) { null } else { Node { value: n, next: build(n - 1) } }
}

function chain(n: int) -> string {
    let head = build(n);
    baml.json.to_string(head)
}

function sparse(n: int) -> string {
    let xs: (int | null)[] = [n, null, n + 1];
    xs.to_string()
}

// A call's type argument whose values cannot meet a demand made on it: the
// values flowing in (here the `"s"` passed to `id`) and the type required of
// the result have no type in common, so no type argument makes the call
// well typed. Each is reported, at the demand or at the value that fails
// it, rather than erased.

class Box<T> {
    v T
}

function id<T>(x: T) -> T {
    x
}

function make<T>(x: T) -> Box<T> {
    Box { v: x }
}

function take(b: Box<int>) -> int {
    b.v
}

function takes_ints(xs: int[]) -> int {
    0
}

function annotated_result() -> int {
    let n: int = id("s");
    n
}

// The map's value type is `int | T` while `T` is still open, and the
// annotation demands exactly `int`.
function open_map_member() -> int {
    let m = { "a": 1, "b": id("s") };
    let n: map<string, int> = m;
    0
}

function open_list_member() -> int {
    let xs = [1, id("s")];
    takes_ints(xs)
}

function open_branch_member(flag: bool) -> int {
    let b = if (flag) { make(1) } else { make(id("s")) };
    take(b)
}

function whole_container(b: Box<int | string>) -> int {
    take(id(b))
}

// Accepted: the open member's values meet the demand.
function accepted(b: Box<int>) -> int {
    let n: int = id(1);
    let m = { "a": 1, "b": id(2) };
    let k: map<string, int> = m;
    let xs = [1, id(2)];
    take(id(b)) + takes_ints(xs) + n
}

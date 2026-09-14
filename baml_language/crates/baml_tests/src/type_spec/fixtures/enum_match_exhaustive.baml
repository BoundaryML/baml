enum Status {
    Active
    Inactive
}

function f(s: Status, a: int, b: int) -> int {
    let r = match (s) {
        Status.Active => a,
        Status.Inactive => b,
    };
    r
 // ^ int
}

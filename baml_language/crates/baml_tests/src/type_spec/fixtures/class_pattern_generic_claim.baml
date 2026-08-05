// Class patterns claim their union member by INSTANTIATION
//
// Against `Box<int> | Box<string>`, the pattern `Box<int> { .. }`
// claims exactly the `Box<int>` member (same class AND agreeing args),
// so per-instantiation arms compose to exhaustive coverage - the same
// member attribution bare type patterns already get.
class PgBox<T> {
    value T
}

function pg_field(box: PgBox<int> | PgBox<string>) -> int throws never {
    let n = match (box) {
     // ^ int
        PgBox<int> { value: let v: int } => v,
        PgBox<string> { value: let s: string } => s.length(),
    };
    n
}

function pg_unit(box: PgBox<int> | PgBox<string>) -> int throws never {
    match (box) {
        PgBox<int> {} | PgBox<string> {} => 7
    }
}

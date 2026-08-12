//
// TYPE_SYSTEM.md (Interfaces section, `_` wildcard): a `_` generic arg is an
// inference hole filled exactly from context, including expression positions
// (`Box<_> { .. }` constructors, turbofish call args), which TIR rejects as
// E0147 hard errors. See also the `#[ignore]`d runtime tests in
// `tests/wildcard_expression_holes.rs`, which unlock with this.
class Box<T> { v T }

function f() -> int {
    let b = Box<_> { v: 5 };
     // ^ user.Box<int>
    b.v
}

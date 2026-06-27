// Destructure pattern in let-else: refutable on a union, irrefutable on a
// non-union (warns).
class User {
  name string
  age int
}
class Admin {
  handle string
}

// Refutable: pattern picks one variant of the union; else diverges.
function PickUserName(u: User | Admin) -> string {
  let User { name, age } = u else { return "admin"; };
  name
}

// Irrefutable: scrutinee is exactly `User`, so the else branch is dead and
// we warn (E0114).
function IrrefutableDestructure(u: User) -> string {
  let User { name, age } = u else { return "unreachable"; };
  name
}

//----
//- diagnostics
// warning: irrefutable `let … else` pattern; the `else` branch is unreachable — use a plain `let` binding instead
//     ╭─[ let_else_let_else_destructure.baml:20:3 ]
//     │
//  20 │   let User { name, age } = u else { return "unreachable"; };
//     │   ───────────┬──────────
//     │              ╰──────────── irrefutable `let … else` pattern; the `else` branch is unreachable — use a plain `let` binding instead
//     │
//     │ Note: Error code: E0114
// ────╯

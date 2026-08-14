class User {
  name string
  age int
}

function ConstBindings(user: User, value: int | string) -> int {
  const x = 1;
  x = 2;
  const y: int = x;
  const User { age } = user;
  const shadowed = if (true) {
    const x = age;
    x
  } else {
    0
  };
  const n: int = value else { return 0; };
  let const = y;
  const + shadowed + n
}

//----
//- diagnostics
// E0010
//
//   ⚠ `const` is currently treated like `let`; BAML does not enforce immutability yet. Use `let` for current BAML semantics.
//    ╭─[expr_const_binding.baml:7:3]
//  7 │   const x = 1;
//    ·   ──┬──
//    ·     ╰── `const` behaves like `let` for now
//    ╰────
// E0010
//
//   ⚠ `const` is currently treated like `let`; BAML does not enforce immutability yet. Use `let` for current BAML semantics.
//    ╭─[expr_const_binding.baml:9:3]
//  9 │   const y: int = x;
//    ·   ──┬──
//    ·     ╰── `const` behaves like `let` for now
//    ╰────
// E0010
//
//   ⚠ `const` is currently treated like `let`; BAML does not enforce immutability yet. Use `let` for current BAML semantics.
//     ╭─[expr_const_binding.baml:10:3]
//  10 │   const User { age } = user;
//     ·   ──┬──
//     ·     ╰── `const` behaves like `let` for now
//     ╰────
// E0010
//
//   ⚠ `const` is currently treated like `let`; BAML does not enforce immutability yet. Use `let` for current BAML semantics.
//     ╭─[expr_const_binding.baml:11:3]
//  11 │   const shadowed = if (true) {
//     ·   ──┬──
//     ·     ╰── `const` behaves like `let` for now
//     ╰────
// E0010
//
//   ⚠ `const` is currently treated like `let`; BAML does not enforce immutability yet. Use `let` for current BAML semantics.
//     ╭─[expr_const_binding.baml:12:5]
//  12 │     const x = age;
//     ·     ──┬──
//     ·       ╰── `const` behaves like `let` for now
//     ╰────
// E0010
//
//   ⚠ `const` is currently treated like `let`; BAML does not enforce immutability yet. Use `let` for current BAML semantics.
//     ╭─[expr_const_binding.baml:17:3]
//  17 │   const n: int = value else { return 0; };
//     ·   ──┬──
//     ·     ╰── `const` behaves like `let` for now
//     ╰────
// E0010
//
//   × `const` is reserved and cannot be used as a binding name
//     ╭─[expr_const_binding.baml:18:7]
//  18 │   let const = y;
//     ·       ──┬──
//     ·         ╰── `const` is reserved here
//     ╰────

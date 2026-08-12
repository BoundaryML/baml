function ConstForLoops(items: int[]) -> int {
  const sum = 0;
  for (const i = 0; i < 3; i += 1) {
    sum = sum + i;
  }
  for (const item in items) {
    sum = sum + item;
  }
  sum
}

//----
//- diagnostics
// E0010
//
//   ⚠ `const` is currently treated like `let`; BAML does not enforce immutability yet. Use `let` for current BAML semantics.
//    ╭─[loops_const_for_loops.baml:2:3]
//  2 │   const sum = 0;
//    ·   ──┬──
//    ·     ╰── `const` behaves like `let` for now
//    ╰────
// E0010
//
//   ⚠ `const` is currently treated like `let`; BAML does not enforce immutability yet. Use `let` for current BAML semantics.
//    ╭─[loops_const_for_loops.baml:3:8]
//  3 │   for (const i = 0; i < 3; i += 1) {
//    ·        ──┬──
//    ·          ╰── `const` behaves like `let` for now
//    ╰────
// E0010
//
//   ⚠ `const` is currently treated like `let`; BAML does not enforce immutability yet. Use `let` for current BAML semantics.
//    ╭─[loops_const_for_loops.baml:6:8]
//  6 │   for (const item in items) {
//    ·        ──┬──
//    ·          ╰── `const` behaves like `let` for now
//    ╰────

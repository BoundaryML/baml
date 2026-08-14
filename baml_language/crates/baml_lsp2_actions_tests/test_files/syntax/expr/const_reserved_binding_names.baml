class Box {
  value int
}

class WithConstField {
  const int
}

function ConstConst() -> int {
  const const = 1;
  const
}

function DestructureReservedName(box: Box) -> int {
  let Box { value: let const } = box;
  0
}

function MatchReservedName(value: int | string) -> int {
  match (value) {
    let const: int => 1,
    _ => 0
  }
}

function ForIteratorReservedName(items: int[]) -> int {
  let total = 0;
  for (let const in items) {
    total += const;
  }
  total
}

function ForCStyleReservedName() -> int {
  let total = 0;
  for (let const = 0; const < 1; const += 1) {
    total += const;
  }
  total
}

function IfWhileConstArrays(items: int[]) -> int {
  let total = 0;
  if const [let x] = items {
    total += x;
  } else {
    total += 0;
  };
  while const [let y] = items {
    break;
  }
  total
}

function NonBindingConstField(record: WithConstField) -> int {
  record.const
}

//----
//- diagnostics
// E0010
//
//   ⚠ `const` is currently treated like `let`; BAML does not enforce immutability yet. Use `let` for current BAML semantics.
//     ╭─[expr_const_reserved_binding_names.baml:10:3]
//  10 │   const const = 1;
//     ·   ──┬──
//     ·     ╰── `const` behaves like `let` for now
//     ╰────
// E0010
//
//   × `const` is reserved and cannot be used as a binding name
//     ╭─[expr_const_reserved_binding_names.baml:10:9]
//  10 │   const const = 1;
//     ·         ──┬──
//     ·           ╰── `const` is reserved here
//     ╰────
// E0010
//
//   × `const` is reserved and cannot be used as a binding name
//     ╭─[expr_const_reserved_binding_names.baml:15:24]
//  15 │   let Box { value: let const } = box;
//     ·                        ──┬──
//     ·                          ╰── `const` is reserved here
//     ╰────
// E0010
//
//   × `const` is reserved and cannot be used as a binding name
//     ╭─[expr_const_reserved_binding_names.baml:21:9]
//  21 │     let const: int => 1,
//     ·         ──┬──
//     ·           ╰── `const` is reserved here
//     ╰────
// E0010
//
//   × `const` is reserved and cannot be used as a binding name
//     ╭─[expr_const_reserved_binding_names.baml:28:12]
//  28 │   for (let const in items) {
//     ·            ──┬──
//     ·              ╰── `const` is reserved here
//     ╰────
// E0010
//
//   × `const` is reserved and cannot be used as a binding name
//     ╭─[expr_const_reserved_binding_names.baml:36:12]
//  36 │   for (let const = 0; const < 1; const += 1) {
//     ·            ──┬──
//     ·              ╰── `const` is reserved here
//     ╰────
// E0010
//
//   ⚠ `const` is currently treated like `let`; BAML does not enforce immutability yet. Use `let` for current BAML semantics.
//     ╭─[expr_const_reserved_binding_names.baml:44:6]
//  44 │   if const [let x] = items {
//     ·      ──┬──
//     ·        ╰── `const` behaves like `let` for now
//     ╰────
// E0010
//
//   ⚠ `const` is currently treated like `let`; BAML does not enforce immutability yet. Use `let` for current BAML semantics.
//     ╭─[expr_const_reserved_binding_names.baml:49:9]
//  49 │   while const [let y] = items {
//     ·         ──┬──
//     ·           ╰── `const` behaves like `let` for now
//     ╰────

// `const` is both a binding introducer (a `let` alias for now) and a word held
// back for future language surface, so the introducer keyword is accepted and
// every *declared* `const` name is rejected.
//
// `NonBindingConstField` below is the load-bearing negative: reading a member
// named `const` is not a declaration, so it must stay clean even though the
// field's declaration is itself reported. Access and declaration are separate
// rules and only one of them fires.

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
// E0158
//
//   × `const` is reserved for future use and cannot be used as a field name
//     ╭─[expr_const_reserved_binding_names.baml:15:3]
//  15 │   const int
//     ·   ──┬──
//     ·     ╰── reserved for future use
//     ╰────
// E0010
//
//   ⚠ `const` is currently treated like `let`; BAML does not enforce immutability yet. Use `let` for current BAML semantics.
//     ╭─[expr_const_reserved_binding_names.baml:19:3]
//  19 │   const const = 1;
//     ·   ──┬──
//     ·     ╰── `const` behaves like `let` for now
//     ╰────
// E0158
//
//   × `const` is reserved for future use and cannot be used as a binding name
//     ╭─[expr_const_reserved_binding_names.baml:19:9]
//  19 │   const const = 1;
//     ·         ──┬──
//     ·           ╰── reserved for future use
//     ╰────
// E0158
//
//   × `const` is reserved for future use and cannot be used as a binding name
//     ╭─[expr_const_reserved_binding_names.baml:24:24]
//  24 │   let Box { value: let const } = box;
//     ·                        ──┬──
//     ·                          ╰── reserved for future use
//     ╰────
// E0158
//
//   × `const` is reserved for future use and cannot be used as a binding name
//     ╭─[expr_const_reserved_binding_names.baml:30:9]
//  30 │     let const: int => 1,
//     ·         ──┬──
//     ·           ╰── reserved for future use
//     ╰────
// E0158
//
//   × `const` is reserved for future use and cannot be used as a binding name
//     ╭─[expr_const_reserved_binding_names.baml:37:12]
//  37 │   for (let const in items) {
//     ·            ──┬──
//     ·              ╰── reserved for future use
//     ╰────
// E0158
//
//   × `const` is reserved for future use and cannot be used as a binding name
//     ╭─[expr_const_reserved_binding_names.baml:45:12]
//  45 │   for (let const = 0; const < 1; const += 1) {
//     ·            ──┬──
//     ·              ╰── reserved for future use
//     ╰────
// E0010
//
//   ⚠ `const` is currently treated like `let`; BAML does not enforce immutability yet. Use `let` for current BAML semantics.
//     ╭─[expr_const_reserved_binding_names.baml:53:6]
//  53 │   if const [let x] = items {
//     ·      ──┬──
//     ·        ╰── `const` behaves like `let` for now
//     ╰────
// E0010
//
//   ⚠ `const` is currently treated like `let`; BAML does not enforce immutability yet. Use `let` for current BAML semantics.
//     ╭─[expr_const_reserved_binding_names.baml:58:9]
//  58 │   while const [let y] = items {
//     ·         ──┬──
//     ·           ╰── `const` behaves like `let` for now
//     ╰────

// Direct cycle: A requires B; B requires A.

interface A requires B {}

interface B requires A {}

//----
//- diagnostics
// E0118
//
//   × interface `requires` cycle: A → B → A
//    ╭─[interface_extends_cycle.baml:3:1]
//  3 │ interface A requires B {}
//    · ─────────────────────────
//    ╰────
// E0118
//
//   × interface `requires` cycle: B → A → B
//    ╭─[interface_extends_cycle.baml:5:1]
//  5 │ interface B requires A {}
//    · ─────────────────────────
//    ╰────

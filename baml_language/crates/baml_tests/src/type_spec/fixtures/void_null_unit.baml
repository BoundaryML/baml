// Ruling 1 (S15): void and null are the same unit type (interim).
//
// Until tuple types give `void` a home as the empty tuple, `void` and
// `null` are mutually-subtyping spellings of the unit type (TIR's
// behavior; kills the 2,680 void-vs-null error-channel entries).
// Implemented leaf-side in hir_ty's subtype discharge; renders keep
// the written spelling.
function g() -> void throws never {
    null
}

function f() -> int throws never {
    let v: void = null;
     // ^ void
    0
}

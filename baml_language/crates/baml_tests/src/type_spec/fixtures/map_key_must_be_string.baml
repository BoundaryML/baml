//
// B-267 part 2 (https://linear.app/boundaryml2/issue/B-267): map keys
// are strings by the language contract - the VM's backing store assumes
// it, and a map literal cannot even spell a non-string key. TIR admitted
// `map<int, int>` SILENTLY and crashed in the VM at `set`; hir_ty
// diagnoses the provably-non-string key at the annotation (E-code -> the
// program is rejected before the VM can see it) while the WRITTEN key
// survives in the lowered type, so tooling surfaces (codegen schemas,
// renders) show what the user wrote. String-literal keys are subtypes of
// string and stay; symbolic keys (the stdlib's own `Map<K, V>` scope)
// stay fail-safe.
function f() -> int throws never {
    let m: map<int, int> = {};
     // ^ map<int, int>
    let k: map<"a" | "b", int> = {};
     // ^ map<"a" | "b", int>
    0
}

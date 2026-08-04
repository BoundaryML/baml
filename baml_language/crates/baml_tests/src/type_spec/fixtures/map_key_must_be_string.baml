// tir: fails
//
// B-267 part 2 (https://linear.app/boundaryml2/issue/B-267): map keys
// are strings by the language contract - the VM's backing store assumes
// it, and a map literal cannot even spell a non-string key. TIR admits
// `map<int, int>` and crashes in the VM at `set`; hir_ty lowers the
// provably-non-string key to the Error sentinel at the annotation (S17
// renders the diagnostic). String-literal keys are subtypes of string
// and stay; symbolic keys (the stdlib's own `Map<K, V>` scope) stay
// fail-safe.
function f() -> int throws never {
    let m: map<int, int> = {};
     // ^ map<!error, int>
    let k: map<"a" | "b", int> = {};
     // ^ map<"a" | "b", int>
    0
}

// `if let` pattern bindings must not leak past the then-branch.
class Ok {
  value string
}
class Err {
  message string
}

// `o` is bound inside the then-branch, must NOT escape after the if-let.
function BindingDoesNotEscapeAfter(r: Ok | Err) -> string {
  if let o: Ok = r {
    let _ = o.value;
  }
  // `o` is out of scope here — must error.
  o.value
}

// Same binding name, different if-let — no cross-contamination.
function BindingDoesNotEscapeIntoSibling(r: Ok | Err) -> string {
  if let o: Ok = r {
    let _ = o.value;
  }
  if let o: Ok = r {
    return o.value;
  }
  // `o` is out of scope here — must error.
  o.value
}

// Else branch must NOT see the then-branch's binding.
function BindingDoesNotLeakIntoElse(r: Ok | Err) -> string {
  if let o: Ok = r {
    o.value
  } else {
    // `o` is not visible here — must error.
    o.value
  }
}

//----
//- diagnostics
// E0003
//
//   × unresolved name: `o.value`
//     ╭─[if_let_if_let_binding_scope.baml:15:3]
//  15 │   o.value
//     ·   ───────
//     ╰────
// E0003
//
//   × unresolved name: `o.value`
//     ╭─[if_let_if_let_binding_scope.baml:27:3]
//  27 │   o.value
//     ·   ───────
//     ╰────
// E0003
//
//   × unresolved name: `o.value`
//     ╭─[if_let_if_let_binding_scope.baml:36:5]
//  36 │     o.value
//     ·     ───────
//     ╰────

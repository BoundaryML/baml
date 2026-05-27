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
// error: unresolved name: o
//     ╭─[ if_let_if_let_binding_scope.baml:13:4 ]
//     │
//  13 │ ╭─▶   }
//     ┆ ┆   
//  15 │ ├─▶   o.value
//     │ │               
//     │ ╰─────────────── unresolved name: o
//     │     
//     │     Note: Error code: E0003
// ────╯
// error: unresolved name: o
//     ╭─[ if_let_if_let_binding_scope.baml:25:4 ]
//     │
//  25 │ ╭─▶   }
//     ┆ ┆   
//  27 │ ├─▶   o.value
//     │ │               
//     │ ╰─────────────── unresolved name: o
//     │     
//     │     Note: Error code: E0003
// ────╯
// error: unresolved name: o
//     ╭─[ if_let_if_let_binding_scope.baml:34:11 ]
//     │
//  34 │ ╭─▶   } else {
//     ┆ ┆   
//  36 │ ├─▶     o.value
//     │ │                 
//     │ ╰───────────────── unresolved name: o
//     │     
//     │     Note: Error code: E0003
// ────╯

function Repro() -> int {
  let _ = 1;
  let _ = 2;
  return 3;
}

function TypedRepro() -> int {
  let _: int = 1;
  let _: int = 2;
  return 3;
}

function ReadDiscard() -> int {
  let _ = 1;
  return _;
}

function MatchDoesNotBindUnderscore(x: int | string) -> int {
  let y = match (x) {
    _ => 1
  };
  return _;
}

function TypedDiscardMismatch() -> int {
  let _: int = "bad";
  return 0;
}

//----
//- diagnostics
// Error: unresolved name: _
//     ╭─[ expr_discard_bindings.baml:15:10 ]
//     │
//  15 │   return _;
//     │          ┬  
//     │          ╰── unresolved name: _
//     │ 
//     │ Note: Error code: E0003
// ────╯
// Error: unresolved name: _
//     ╭─[ expr_discard_bindings.baml:22:10 ]
//     │
//  22 │   return _;
//     │          ┬  
//     │          ╰── unresolved name: _
//     │ 
//     │ Note: Error code: E0003
// ────╯
// Error: type mismatch: expected int, got "bad"
//     ╭─[ expr_discard_bindings.baml:26:16 ]
//     │
//  26 │   let _: int = "bad";
//     │                ──┬──  
//     │                  ╰──── type mismatch: expected int, got "bad"
//     │ 
//     │ Note: Error code: E0001
// ────╯

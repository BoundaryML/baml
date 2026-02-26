function MayFail(x: int) -> string {
  "ok"
}

function MissingBindingPattern(x: int) -> string {
  MayFail(x) catch {
    let y = "hi";
    y
  }
}

//----
//- diagnostics
// Error: Expected '(', found '{'
//    ╭─[ catch_missing_binding_pattern.baml:6:20 ]
//    │
//  6 │   MayFail(x) catch {
//    │                    ┬  
//    │                    ╰── Expected '(', found '{'
//    │ 
//    │ Note: Error code: E0010
// ───╯

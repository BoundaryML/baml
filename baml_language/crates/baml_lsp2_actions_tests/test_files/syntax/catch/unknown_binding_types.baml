function MayFail(x: int) -> string {
  "ok"
}

function UnknownBindingTypes(x: int) -> string {
  MayFail(x) catch (e) {
    _ => "caught any"
  } catch (other) {
    _ => "caught unknown"
  }
}

//----
//- diagnostics
// Warning: unreachable arm
//    ╭─[ catch_unknown_binding_types.baml:7:10 ]
//    │
//  7 │     _ => "caught any"
//    │          ──────┬─────  
//    │                ╰─────── unreachable arm
//    │ 
//    │ Note: Error code: E0063
// ───╯
// Warning: unreachable arm
//    ╭─[ catch_unknown_binding_types.baml:9:10 ]
//    │
//  9 │     _ => "caught unknown"
//    │          ────────┬───────  
//    │                  ╰───────── unreachable arm
//    │ 
//    │ Note: Error code: E0063
// ───╯

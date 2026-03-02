// Multiple catch clauses chained on same expression — no diagnostics expected
function MayFail(x: int) -> string {
  "ok"
}

function ChainedCatchBasic(x: int) -> string {
  MayFail(x) catch (e) {
    _: string => "string error"
  } catch (e2) {
    _: int => "int error"
  }
}

// Three chained catches
function TripleChainedCatch(x: int) -> string {
  MayFail(x) catch (e) {
    _: string => "first"
  } catch (e2) {
    _: int => "second"
  } catch (e3) {
    _ => "third"
  }
}

//----
//- diagnostics
// Warning: Unreachable catch arm
//    ╭─[ catch_chained_catch.baml:8:7 ]
//    │
//  8 │     _: string => "string error"
//    │       ────┬────  
//    │           ╰────── Unreachable catch arm
//    │ 
//    │ Note: Error code: E0095
// ───╯
// Warning: Unreachable catch arm
//     ╭─[ catch_chained_catch.baml:10:7 ]
//     │
//  10 │     _: int => "int error"
//     │       ───┬──  
//     │          ╰──── Unreachable catch arm
//     │ 
//     │ Note: Error code: E0095
// ────╯
// Warning: Unreachable catch arm
//     ╭─[ catch_chained_catch.baml:17:7 ]
//     │
//  17 │     _: string => "first"
//     │       ────┬────  
//     │           ╰────── Unreachable catch arm
//     │ 
//     │ Note: Error code: E0095
// ────╯
// Warning: Unreachable catch arm
//     ╭─[ catch_chained_catch.baml:19:7 ]
//     │
//  19 │     _: int => "second"
//     │       ───┬──  
//     │          ╰──── Unreachable catch arm
//     │ 
//     │ Note: Error code: E0095
// ────╯
// Warning: Unreachable catch arm
//     ╭─[ catch_chained_catch.baml:21:7 ]
//     │
//  21 │     _ => "third"
//     │       ┬  
//     │       ╰── Unreachable catch arm
//     │ 
//     │ Note: Error code: E0095
// ────╯

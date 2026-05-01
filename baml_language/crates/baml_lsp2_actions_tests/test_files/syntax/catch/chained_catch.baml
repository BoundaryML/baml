// Multiple catch clauses chained on same expression — unreachable because MayFail never throws
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
// Warning: unreachable arm
//    ╭─[ catch_chained_catch.baml:8:18 ]
//    │
//  8 │     _: string => "string error"
//    │                  ───────┬──────  
//    │                         ╰──────── unreachable arm
//    │ 
//    │ Note: Error code: E0063
// ───╯
// Warning: unreachable arm
//     ╭─[ catch_chained_catch.baml:10:15 ]
//     │
//  10 │     _: int => "int error"
//     │               ─────┬─────  
//     │                    ╰─────── unreachable arm
//     │ 
//     │ Note: Error code: E0063
// ────╯
// Warning: unreachable arm
//     ╭─[ catch_chained_catch.baml:17:18 ]
//     │
//  17 │     _: string => "first"
//     │                  ───┬───  
//     │                     ╰───── unreachable arm
//     │ 
//     │ Note: Error code: E0063
// ────╯
// Warning: unreachable arm
//     ╭─[ catch_chained_catch.baml:19:15 ]
//     │
//  19 │     _: int => "second"
//     │               ────┬───  
//     │                   ╰───── unreachable arm
//     │ 
//     │ Note: Error code: E0063
// ────╯
// Warning: unreachable arm
//     ╭─[ catch_chained_catch.baml:21:10 ]
//     │
//  21 │     _ => "third"
//     │          ───┬───  
//     │             ╰───── unreachable arm
//     │ 
//     │ Note: Error code: E0063
// ────╯

// Multiple catch clauses chained on same expression — unreachable because MayFail never throws
function MayFail(x: int) -> string {
  "ok"
}

function ChainedCatchBasic(x: int) -> string {
  MayFail(x) catch (e) {
    string => "string error"
  } catch (e2) {
    int => "int error"
  }
}

// Three chained catches
function TripleChainedCatch(x: int) -> string {
  MayFail(x) catch (e) {
    string => "first"
  } catch (e2) {
    int => "second"
  } catch (e3) {
    _ => "third"
  }
}

//----
//- diagnostics
// warning: unreachable arm
//    ╭─[ catch_chained_catch.baml:8:15 ]
//    │
//  8 │     string => "string error"
//    │               ───────┬──────  
//    │                      ╰──────── unreachable arm
//    │ 
//    │ Note: Error code: E0063
// ───╯
// warning: unreachable arm
//     ╭─[ catch_chained_catch.baml:10:12 ]
//     │
//  10 │     int => "int error"
//     │            ─────┬─────  
//     │                 ╰─────── unreachable arm
//     │ 
//     │ Note: Error code: E0063
// ────╯
// warning: unreachable arm
//     ╭─[ catch_chained_catch.baml:17:15 ]
//     │
//  17 │     string => "first"
//     │               ───┬───  
//     │                  ╰───── unreachable arm
//     │ 
//     │ Note: Error code: E0063
// ────╯
// warning: unreachable arm
//     ╭─[ catch_chained_catch.baml:19:12 ]
//     │
//  19 │     int => "second"
//     │            ────┬───  
//     │                ╰───── unreachable arm
//     │ 
//     │ Note: Error code: E0063
// ────╯
// warning: unreachable arm
//     ╭─[ catch_chained_catch.baml:21:10 ]
//     │
//  21 │     _ => "third"
//     │          ───┬───  
//     │             ╰───── unreachable arm
//     │ 
//     │ Note: Error code: E0063
// ────╯

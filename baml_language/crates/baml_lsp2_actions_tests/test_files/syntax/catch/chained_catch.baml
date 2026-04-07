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
//    ╭─[ catch_chained_catch.baml:8:20 ]
//    │
//  8 │ ╭─▶     _: string => "string error"
//  9 │ ├─▶   } catch (e2) {
//    │ │                      
//    │ ╰────────────────────── unreachable arm
//    │     
//    │     Note: Error code: E0001
// ───╯
// Warning: unreachable arm
//     ╭─[ catch_chained_catch.baml:10:17 ]
//     │
//  10 │ ╭─▶     _: int => "int error"
//  11 │ ├─▶   }
//     │ │         
//     │ ╰───────── unreachable arm
//     │     
//     │     Note: Error code: E0001
// ────╯
// Warning: unreachable arm
//     ╭─[ catch_chained_catch.baml:17:20 ]
//     │
//  17 │ ╭─▶     _: string => "first"
//  18 │ ├─▶   } catch (e2) {
//     │ │                      
//     │ ╰────────────────────── unreachable arm
//     │     
//     │     Note: Error code: E0001
// ────╯
// Warning: unreachable arm
//     ╭─[ catch_chained_catch.baml:19:17 ]
//     │
//  19 │ ╭─▶     _: int => "second"
//  20 │ ├─▶   } catch (e3) {
//     │ │                      
//     │ ╰────────────────────── unreachable arm
//     │     
//     │     Note: Error code: E0001
// ────╯
// Warning: unreachable arm
//     ╭─[ catch_chained_catch.baml:21:12 ]
//     │
//  21 │ ╭─▶     _ => "third"
//  22 │ ├─▶   }
//     │ │         
//     │ ╰───────── unreachable arm
//     │     
//     │     Note: Error code: E0001
// ────╯

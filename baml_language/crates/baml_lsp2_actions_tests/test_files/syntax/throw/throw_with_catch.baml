// Throw + catch interaction patterns
function MayFail(x: int) -> string {
  "ok"
}

function AlsoMayFail(x: int) -> string {
  "also ok"
}

// Throw inside function, caught by caller
function ThrowingFunction(x: int) -> string {
  if (x < 0) {
    throw "negative input"
  }
  "ok"
}

function CallAndCatch(x: int) -> string {
  ThrowingFunction(x) catch (e) {
    _ => "caught: " + e
  }
}

// Chained: first catch rethrows, second catch handles
function ChainedRethrow(x: int) -> string {
  MayFail(x) catch (e) {
    _ => throw "wrapped"
  } catch (e2) {
    _ => "ultimately caught"
  }
}

// Nested throwing functions with catch at outer level
function OuterCatch(x: int) -> string {
  let a = MayFail(x);
  let b = AlsoMayFail(x);
  a + b
}

function CatchOuter(x: int) -> string {
  OuterCatch(x) catch (e) {
    _ => "caught outer"
  }
}

//----
//- diagnostics
// warning: unreachable arm
//     ╭─[ throw_throw_with_catch.baml:27:9 ]
//     │
//  27 │     _ => throw "wrapped"
//     │         ────────┬───────  
//     │                 ╰───────── unreachable arm
//     │ 
//     │ Note: Error code: E0063
// ────╯
// warning: unreachable arm
//     ╭─[ throw_throw_with_catch.baml:29:10 ]
//     │
//  29 │     _ => "ultimately caught"
//     │          ─────────┬─────────  
//     │                   ╰─────────── unreachable arm
//     │ 
//     │ Note: Error code: E0063
// ────╯
// warning: unreachable arm
//     ╭─[ throw_throw_with_catch.baml:42:10 ]
//     │
//  42 │     _ => "caught outer"
//     │          ───────┬──────  
//     │                 ╰──────── unreachable arm
//     │ 
//     │ Note: Error code: E0063
// ────╯

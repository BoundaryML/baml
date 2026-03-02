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
// Warning: Unreachable catch arm
//     ╭─[ throw_throw_with_catch.baml:27:5 ]
//     │
//  27 │     _ => throw "wrapped"
//     │     ┬  
//     │     ╰── Unreachable catch arm
//     │ 
//     │ Note: Error code: E0095
// ────╯
// Warning: Unreachable catch arm
//     ╭─[ throw_throw_with_catch.baml:29:5 ]
//     │
//  29 │     _ => "ultimately caught"
//     │     ┬  
//     │     ╰── Unreachable catch arm
//     │ 
//     │ Note: Error code: E0095
// ────╯
// Warning: Unreachable catch arm
//     ╭─[ throw_throw_with_catch.baml:42:5 ]
//     │
//  42 │     _ => "caught outer"
//     │     ┬  
//     │     ╰── Unreachable catch arm
//     │ 
//     │ Note: Error code: E0095
// ────╯

// Test: catch arm reachability with known vs unknown throw sets.
//
// Case 1: known throw set (string), non-matching arm (int) → unreachable
// Case 2: known throw set (string), matching arm (string) → reachable
// Case 3: panic arm is always reachable regardless of residual

function ThrowsString(x: int) -> string {
  match (x) {
    0 => throw "error",
    _ => "ok"
  }
}

// Case 1: int arm can never match string throws → unreachable warning
function UnreachableIntArm(x: int) -> string {
  ThrowsString(x) catch (e) {
    int => "unreachable"
  }
}

// Case 2: string arm matches string throws → reachable, no warning
function ReachableStringArm(x: int) -> string {
  ThrowsString(x) catch (e) {
    string => "caught"
  }
}

// Case 3: panic arm is always reachable even after residual is exhausted
function PanicArmAfterExhaustion(x: int) -> string {
  ThrowsString(x) catch (e) {
    string => "caught string",
    baml.panics.DivisionByZero => "caught panic"
  }
}

//----
//- diagnostics
// warning: unreachable arm
//     ╭─[ catch_catch_throw_set_reachability.baml:17:12 ]
//     │
//  17 │     int => "unreachable"
//     │            ──────┬──────  
//     │                  ╰──────── unreachable arm
//     │ 
//     │ Note: Error code: E0063
// ────╯

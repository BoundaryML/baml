// Throw inside catch handler — rethrow pattern
function MayFail(x: int) -> string {
  "ok"
}

// Rethrow caught error
function RethrowError(x: int) -> string {
  MayFail(x) catch (e) {
    _ => throw e
  }
}

// Conditional rethrow
function ConditionalRethrow(x: int) -> string {
  MayFail(x) catch (e) {
    _: string => "handled string",
    other => throw other
  }
}

// Throw new error inside catch
function ThrowNewInCatch(x: int) -> string {
  MayFail(x) catch (e) {
    _ => throw "wrapped: original error"
  }
}

//----
//- diagnostics
// Error: unreachable arm
//     ╭─[ throw_throw_in_catch.baml:16:20 ]
//     │
//  16 │     _: string => "handled string",
//     │                    ────────┬───────  
//     │                            ╰───────── unreachable arm
//     │ 
//     │ Note: Error code: E0001
// ────╯

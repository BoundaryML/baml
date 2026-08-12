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
    string => "handled string",
    let other => throw other
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
// E0063
//
//   ⚠ unreachable arm
//    ╭─[throw_throw_in_catch.baml:9:10]
//  9 │     _ => throw e
//    ·          ───────
//    ╰────
// E0063
//
//   ⚠ unreachable arm
//     ╭─[throw_throw_in_catch.baml:16:15]
//  16 │     string => "handled string",
//     ·               ────────────────
//     ╰────
// E0063
//
//   ⚠ unreachable arm
//     ╭─[throw_throw_in_catch.baml:17:18]
//  17 │     let other => throw other
//     ·                  ───────────
//     ╰────
// E0063
//
//   ⚠ unreachable arm
//     ╭─[throw_throw_in_catch.baml:24:10]
//  24 │     _ => throw "wrapped: original error"
//     ·          ───────────────────────────────
//     ╰────

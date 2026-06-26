// catch_all (exhaustive): partial handling is an error

function MayFailIntOrString(x: int) -> string {
    match (x) {
        0 => throw "string error",
        1 => throw 1,
        _ => "ok"
    }
}

// catch_all with only string arm — int is unhandled → E0062
function PartialCatchAll(x: int) -> string {
  MayFailIntOrString(x) catch_all (e) {
    string => "caught string"
  }
}

//----
//- diagnostics
// error: non-exhaustive catch_all on type int | string; missing: int
//     ╭─[ catch_catch_all_exhaustive.baml:13:3 ]
//     │
//  13 │ ╭─▶   MayFailIntOrString(x) catch_all (e) {
//     ┆ ┆
//  15 │ ├─▶   }
//     │ │
//     │ ╰───────── non-exhaustive catch_all on type int | string; missing: int
//     │
//     │     Note: Error code: E0062
// ────╯

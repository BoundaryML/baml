// End-to-end test for the catch_all_panics keyword: parsing, HIR, TIR, MIR, codegen.
// `catch_all_panics` is a contextual keyword (lexed as a plain identifier) that, like
// `catch_all`, exhaustively handles errors — but it also swallows panics under a wildcard
// arm instead of re-throwing them.

function MayFailIntOrString(x: int) -> string {
    match (x) {
        0 => throw "string error",
        1 => throw 42,
        _ => "ok"
    }
}

// Exhaustive catch_all_panics — wildcard covers everything, including panics.
function CatchAllPanicsWildcard(x: int) -> string {
    MayFailIntOrString(x) catch_all_panics (e) {
        _ => "caught all"
    }
}

// catch_all_panics with all types explicitly handled: every error type, and
// every panic through the `baml.panics.Panic` alias (typed error arms alone
// would leave the panics unhandled, E0062).
function CatchAllPanicsTyped(x: int) -> string {
    MayFailIntOrString(x) catch_all_panics (e) {
        string => "caught string",
        int => "caught int",
        baml.panics.Panic => "caught panic"
    }
}

// Chained: catch handles string, catch_all_panics handles the rest (and panics).
function CatchThenCatchAllPanics(x: int) -> string {
    MayFailIntOrString(x) catch (e) {
        string => "caught string"
    } catch_all_panics (e2) {
        _ => "caught rest"
    }
}

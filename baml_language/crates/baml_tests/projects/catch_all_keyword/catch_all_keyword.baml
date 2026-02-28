// End-to-end test for catch_all keyword: parsing, HIR, TIR, codegen

function MayFailIntOrString(x: int) -> string {
    match (x) {
        0 => throw "string error",
        1 => throw 42,
        _ => "ok"
    }
}

// Non-exhaustive catch — partial handling is fine
function PartialCatch(x: int) -> string {
    MayFailIntOrString(x) catch (e) {
        _: string => "caught string"
    }
}

// Exhaustive catch_all — wildcard covers everything
function CatchAllWildcard(x: int) -> string {
    MayFailIntOrString(x) catch_all (e) {
        _ => "caught all"
    }
}

// catch_all with all types explicitly handled
function CatchAllTyped(x: int) -> string {
    MayFailIntOrString(x) catch_all (e) {
        _: string => "caught string",
        _: int => "caught int"
    }
}

// Chained: catch handles string, catch_all handles rest
function CatchThenCatchAll(x: int) -> string {
    MayFailIntOrString(x) catch (e) {
        _: string => "caught string"
    } catch_all (e2) {
        _ => "caught rest"
    }
}

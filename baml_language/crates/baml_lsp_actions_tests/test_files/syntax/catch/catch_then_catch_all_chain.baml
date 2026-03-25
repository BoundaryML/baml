// catch (non-exhaustive) followed by catch_all (exhaustive) — residual from catch
// flows into catch_all which must handle it

function MayFailIntOrString(x: int) -> string {
    match (x) {
        0 => throw "string error",
        1 => throw 1,
        _ => "ok"
    }
}

// catch handles string, catch_all handles the rest with wildcard — OK
function CatchThenCatchAllOk(x: int) -> string {
  MayFailIntOrString(x) catch (e) {
    _: string => "caught string"
  } catch_all (e2) {
    _ => "caught rest"
  }
}

// catch handles string, catch_all only handles int but nothing else
// remains — OK since string+int covers all thrown types
function CatchThenCatchAllTyped(x: int) -> string {
  MayFailIntOrString(x) catch (e) {
    _: string => "caught string"
  } catch_all (e2) {
    _: int => "caught int"
  }
}

//----
//- diagnostics
// <no-diagnostics-expected>

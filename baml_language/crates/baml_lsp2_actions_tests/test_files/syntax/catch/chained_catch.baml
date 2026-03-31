// Multiple catch clauses chained on same expression — no diagnostics expected
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
// <no-diagnostics-expected>

// Multiple catch clauses chained on same expression — unreachable because MayFail never throws
function MayFail(x: int) -> string {
  "ok"
}

function ChainedCatchBasic(x: int) -> string {
  MayFail(x) catch (e) {
    string => "string error"
  } catch (e2) {
    int => "int error"
  }
}

// Three chained catches
function TripleChainedCatch(x: int) -> string {
  MayFail(x) catch (e) {
    string => "first"
  } catch (e2) {
    int => "second"
  } catch (e3) {
    _ => "third"
  }
}

//----
//- diagnostics
// <no-diagnostics-expected>

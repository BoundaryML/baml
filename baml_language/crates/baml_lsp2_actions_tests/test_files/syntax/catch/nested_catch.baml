// Nested catch expressions — inner catch inside outer catch arm
function MayFail(x: int) -> string {
  "ok"
}

function AlsoMayFail(x: int) -> string {
  "also ok"
}

function NestedCatchInArm(x: int) -> string {
  MayFail(x) catch (e) {
    _ => AlsoMayFail(x) catch (e2) {
      _ => "double caught"
    }
  }
}

// Catch result used in let binding
function CatchInLet(x: int) -> string {
  let result = MayFail(x) catch (e) {
    _ => "fallback"
  };
  result
}

//----
//- diagnostics
// <no-diagnostics-expected>

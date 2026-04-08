// === catch should absorb throws ===

function caught_may_fail(x: int) -> string throws string {
  if (x == 0) { throw "zero" }
  "ok"
}

// Calling a throwing function without catch - should propagate throws
function test_no_catch() -> string {
  caught_may_fail(1)
}

// Catching with wildcard - should absorb throws
function test_catch_all() -> string {
  caught_may_fail(1) catch (e) {
    _ => "caught"
  }
}

// Catching specific type - should absorb that type
function test_catch_string() -> string {
  caught_may_fail(1) catch (e) {
    _: string => "caught string"
  }
}

// Catch then re-throw - should propagate the re-thrown type
function test_catch_rethrow() -> string {
  caught_may_fail(1) catch (e) {
    _ => throw 42
  }
}

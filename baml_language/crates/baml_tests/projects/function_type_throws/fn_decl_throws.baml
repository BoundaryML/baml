// === Function declaration throws preserved in type ===

// Function with explicit throws - should show throws string in type
function declared_may_fail(x: int) -> string throws string {
  if (x == 0) { throw "zero" }
  "ok"
}

// Function with no throws - should show throws never
function always_ok(x: int) -> string {
  "always ok"
}

// Function calling a throwing function - should propagate throws
function caller() -> string {
  declared_may_fail(1)
}

// Function calling a throwing function with catch - should not propagate
function safe_caller() -> string {
  declared_may_fail(1) catch (e) {
    _ => "caught"
  }
}

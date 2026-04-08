// === Explicit throws on callback param should NOT create effect var ===

// When the callback param has explicit throws, no __throws_ var should appear
function apply_explicit(f: () -> int throws string) -> int {
  f()
}

function test_explicit_param_pure() -> int {
  apply_explicit(() -> int { 42 })
}

function test_explicit_param_throwing() -> int {
  apply_explicit(() -> int { throw "ok" })
}

// Explicit throws never on param - pure callbacks only
function apply_pure_only(f: () -> int throws never) -> int {
  f()
}

function test_pure_only_pure() -> int {
  apply_pure_only(() -> int { 42 })
}

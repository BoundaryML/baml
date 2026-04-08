// === Optional call should propagate callback throws ===

function optional_call_rethrows(cb: (() -> int throws string)?) -> int? {
  cb?.()
}

function optional_call_caught(cb: (() -> int throws string)?) -> int? {
  cb?.() catch (e) {
    _ => null
  }
}

function test_optional_call_with_throwing_callback() -> int? {
  optional_call_rethrows(() -> int { throw "boom" })
}

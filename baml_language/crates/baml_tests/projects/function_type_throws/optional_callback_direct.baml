// === Optional direct callback invocation with omitted throws ===

function optional_apply(cb: (() -> int)?) -> int? {
  cb?.()
}

function optional_apply_with_body(cb: (() -> int)?) -> int? {
  if (cb == null) {
    throw "missing callback"
  }
  cb()
}

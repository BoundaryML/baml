// === Stored callback enforcement tests ===
// These test that storing a throwing function in a position typed `throws never` produces an error.

// --- Class field assignments ---

class StoredPureHandler {
  run: () -> null
}

// ERROR: assigning throwing lambda to field typed throws never
function make_bad_stored_handler() -> StoredPureHandler {
  StoredPureHandler { run: () -> null { throw "error" } }
}

// OK: assigning pure lambda to field typed throws never
function make_good_stored_handler() -> StoredPureHandler {
  StoredPureHandler { run: () -> null { null } }
}

// --- Class with explicit throws on field ---

class StoredThrowingHandler {
  run: () -> null throws string
}

// OK: assigning throwing lambda to field typed throws string
function make_stored_throwing_handler() -> StoredThrowingHandler {
  StoredThrowingHandler { run: () -> null { throw "error" } }
}

// --- Local variable assignment ---

// ERROR: assigning throwing lambda to variable typed throws never
function test_stored_local_error() -> null {
  let f: () -> null = () -> null { throw "oops" }
  null
}

// ERROR: wrong function shape should report the structural mismatch, not the
// stored-throws diagnostic.
function test_stored_local_shape_mismatch() -> null {
  let f: (int) -> null = () -> null { throw "oops" }
  null
}

// OK: assigning pure lambda to variable typed throws never
function test_stored_local_ok() -> null {
  let f: () -> null = () -> null { null }
  null
}

// OK: assigning throwing lambda to variable typed throws string
function test_stored_local_with_throws() -> null {
  let f: () -> null throws string = () -> null { throw "oops" }
  null
}

// --- Type alias usage ---

type StoredPureCb = () -> int

// ERROR: assigning throwing lambda to type alias that defaults to throws never
function test_stored_alias_error() -> null {
  let cb: StoredPureCb = () -> int { throw "error" }
  null
}

// OK: assigning pure lambda
function test_stored_alias_ok() -> null {
  let cb: StoredPureCb = () -> int { 42 }
  null
}

// --- Return position ---

// ERROR: returning throwing closure when return type defaults to throws never
function make_stored_closure_bad() -> (() -> int) {
  return () -> int { throw "oops" }
}

// OK: returning pure closure
function make_stored_closure_good() -> (() -> int) {
  return () -> int { 42 }
}

// OK: returning throwing closure when return type has explicit throws
function make_stored_closure_with_throws() -> (() -> int throws string) {
  return () -> int { throw "oops" }
}

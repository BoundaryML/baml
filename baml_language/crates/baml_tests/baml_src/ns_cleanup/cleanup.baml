// BEP-042 `cleanup` magic-method tests (Commit 1: the run-once latch).
//
// `cleanup` is recognized by name — a class that defines `function cleanup(self) -> void { ... }`
// gets a finalizer whose body runs at most once per instance, whether invoked explicitly or via
// `defer`. Commit 1 covers those two trigger paths (the GC finalizer path is Commit 2). The
// run-once guarantee is the per-instance latch flipped by `root._cleanup_begin`.
//
// Helper functions use the test-block local-boxing VM workaround; resource classes are prefixed
// (CleanupResource, CleanupThrowsNeverR) to coexist in the single corpus namespace.
//
// Compile-time diagnostics (invalid cleanup signatures) are not expressible as corpus test
// blocks:
//   - cleanup_with_extra_param_is_compile_error
//   - cleanup_with_non_void_return_is_compile_error
//   - cleanup_with_default_on_self_is_compile_error
//   - cleanup_with_throws_is_compile_error

// A class whose `cleanup` appends a marker to its own `log` field, so the
// number of markers observed after the fact is exactly the number of times the
// body ran.
class CleanupResource {
  log string[]
  function cleanup(self) -> void {
    self.log.push("cleaned")
  }
}

// Two explicit `cleanup()` calls run the body exactly once: the latch is set
// on the first call, so the second is a no-op.
function cleanup_explicit_double_call_main() -> string[] {
  let r = CleanupResource { log: [] }
  r.cleanup()
  r.cleanup()
  r.log
}

test "cleanup_runs_once_on_explicit_double_call" {
    assert.equal(cleanup_explicit_double_call_main(), ["cleaned"])
}

// `defer { r.cleanup() }` invokes the finalizer at scope exit — after the
// block body — so the markers are ["body", "cleaned"].
function cleanup_via_defer_main() -> string[] {
  let r = CleanupResource { log: [] }
  {
    defer { r.cleanup() }
    r.log.push("body")
  }
  r.log
}

test "cleanup_runs_via_defer" {
    assert.equal(cleanup_via_defer_main(), ["body", "cleaned"])
}

// An explicit `cleanup()` runs the body and sets the latch; a later
// `defer { r.cleanup() }` sees the latch set and is a no-op. The body runs
// exactly once across both paths.
function cleanup_explicit_then_defer_main() -> string[] {
  let r = CleanupResource { log: [] }
  r.cleanup()
  {
    defer { r.cleanup() }
  }
  r.log
}

test "cleanup_explicit_then_defer_shares_one_latch" {
    assert.equal(cleanup_explicit_then_defer_main(), ["cleaned"])
}

// The latch is per-instance: cleaning `a` (twice) does not flip `b`'s latch,
// so each instance still runs its body exactly once → 1 + 1 markers total.
function cleanup_per_instance_main() -> int {
  let a = CleanupResource { log: [] }
  let b = CleanupResource { log: [] }
  a.cleanup()
  a.cleanup()
  b.cleanup()
  a.log.length() + b.log.length()
}

test "cleanup_runs_once_per_distinct_instance" {
    assert.equal(cleanup_per_instance_main(), 2)
}

// `throws never` is the language-blessed "provably cannot fail" spelling and
// is equivalent to no throws clause — it must be ACCEPTED as the magic shape
// (latch injected), not rejected. If it were treated as malformed, no guard
// would be injected and the body would run twice → ["cleaned", "cleaned"].
class CleanupThrowsNeverR {
  log string[]
  function cleanup(self) -> void throws never {
    self.log.push("cleaned")
  }
}

function cleanup_throws_never_main() -> string[] {
  let r = CleanupThrowsNeverR { log: [] }
  r.cleanup()
  r.cleanup()
  r.log
}

test "cleanup_with_explicit_throws_never_is_accepted" {
    assert.equal(cleanup_throws_never_main(), ["cleaned"])
}

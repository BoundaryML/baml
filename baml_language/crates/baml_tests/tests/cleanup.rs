//! BEP-042 `cleanup` magic-method tests (Commit 1: the run-once latch).
//!
//! `cleanup` is recognized by name — a class that defines
//! `function cleanup(self) -> void { ... }` gets a finalizer whose body runs at
//! most once per instance, whether invoked explicitly or via `defer`. Commit 1
//! covers those two trigger paths (the GC finalizer path is Commit 2). The
//! run-once guarantee is the per-instance latch flipped by `root._cleanup_begin`.

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

#[track_caller]
fn expect_int(v: BexExternalValue) -> i64 {
    match v {
        BexExternalValue::Int(i) => i,
        other => panic!("expected Int, got {other:?}"),
    }
}

#[track_caller]
fn expect_strings(v: BexExternalValue) -> Vec<String> {
    match v {
        BexExternalValue::Array { items, .. } => items
            .into_iter()
            .map(|it| match it {
                BexExternalValue::String(s) => s.to_string(),
                other => panic!("expected String element, got {other:?}"),
            })
            .collect(),
        other => panic!("expected Array, got {other:?}"),
    }
}

// A class whose `cleanup` appends a marker to its own `log` field, so the number
// of markers observed after the fact is exactly the number of times the body ran.
const RESOURCE: &str = r#"
class Resource {
  log string[]
  function cleanup(self) -> void {
    self.log.push("cleaned")
  }
}
"#;

#[tokio::test]
async fn cleanup_runs_once_on_explicit_double_call() {
    // Two explicit `cleanup()` calls run the body exactly once: the latch is set
    // on the first call, so the second is a no-op.
    let output = baml_test!(&format!(
        r#"
{RESOURCE}
function main() -> string[] {{
  let r = Resource {{ log: [] }}
  r.cleanup()
  r.cleanup()
  r.log
}}
"#
    ));
    assert_eq!(expect_strings(output.result.unwrap()), vec!["cleaned"]);
}

#[tokio::test]
async fn cleanup_runs_via_defer() {
    // `defer { r.cleanup() }` invokes the finalizer at scope exit — after the
    // block body — so the markers are ["body", "cleaned"].
    let output = baml_test!(&format!(
        r#"
{RESOURCE}
function main() -> string[] {{
  let r = Resource {{ log: [] }}
  {{
    defer {{ r.cleanup() }}
    r.log.push("body")
  }}
  r.log
}}
"#
    ));
    assert_eq!(
        expect_strings(output.result.unwrap()),
        vec!["body", "cleaned"]
    );
}

#[tokio::test]
async fn cleanup_explicit_then_defer_shares_one_latch() {
    // An explicit `cleanup()` runs the body and sets the latch; a later
    // `defer { r.cleanup() }` sees the latch set and is a no-op. The body runs
    // exactly once across both paths.
    let output = baml_test!(&format!(
        r#"
{RESOURCE}
function main() -> string[] {{
  let r = Resource {{ log: [] }}
  r.cleanup()
  {{
    defer {{ r.cleanup() }}
  }}
  r.log
}}
"#
    ));
    assert_eq!(expect_strings(output.result.unwrap()), vec!["cleaned"]);
}

#[tokio::test]
async fn cleanup_runs_once_per_distinct_instance() {
    // The latch is per-instance: cleaning `a` (twice) does not flip `b`'s latch,
    // so each instance still runs its body exactly once → 1 + 1 markers total.
    let output = baml_test!(&format!(
        r#"
{RESOURCE}
function main() -> int {{
  let a = Resource {{ log: [] }}
  let b = Resource {{ log: [] }}
  a.cleanup()
  a.cleanup()
  b.cleanup()
  a.log.length() + b.log.length()
}}
"#
    ));
    assert_eq!(expect_int(output.result.unwrap()), 2);
}

#[tokio::test]
async fn direct_cleanup_satisfies_and_dispatches_through_resource_interface() {
    // `cleanup` must remain a direct class method so the emitter marks the
    // class finalizable. The same method also satisfies the Resource contract;
    // virtual dispatch must not fall back to the interface default.
    let output = baml_test!(
        r#"
interface Resource {
  function cleanup(self) -> void throws never
}
class R {
  log string[]
  function cleanup(self) -> void throws never {
    self.log.push("cleaned")
  }
  implements Resource {}
}
function clean(resource: Resource) -> void throws never {
  resource.cleanup()
}
function main() -> string[] {
  let r = R { log: [] }
  clean(r)
  clean(r)
  r.log
}
"#
    );
    assert_eq!(expect_strings(output.result.unwrap()), vec!["cleaned"]);
}

#[tokio::test]
async fn baml_collect_garbage_runs_unreachable_cleanup_before_returning() {
    // BAML tests sometimes need to observe a finalizer deterministically. The
    // builtin performs a major collection and drains the finalizer queue before
    // returning, so the shared log is already updated at the next statement.
    let output = baml_test!(
        r#"
class Resource {
  log string[]
  function cleanup(self) -> void throws never {
    self.log.push("cleaned")
  }
}
function abandon(log: string[]) -> void throws never {
  let resource = Resource { log: log }
  resource.log.push("created")
}
function main() -> string[] {
  let log: string[] = []
  abandon(log)
  baml.sys.collect_garbage()
  log
}
"#
    );
    assert_eq!(
        expect_strings(output.result.unwrap()),
        vec!["created", "cleaned"]
    );
}

#[tokio::test]
async fn cleanup_with_explicit_throws_never_is_accepted() {
    // `throws never` is the language-blessed "provably cannot fail" spelling and
    // is equivalent to no throws clause — it must be ACCEPTED as the magic shape
    // (latch injected), not rejected. If it were treated as malformed, no guard
    // would be injected and the body would run twice → ["cleaned", "cleaned"].
    let output = baml_test!(
        r#"
class R {
  log string[]
  function cleanup(self) -> void throws never {
    self.log.push("cleaned")
  }
}
function main() -> string[] {
  let r = R { log: [] }
  r.cleanup()
  r.cleanup()
  r.log
}
"#
    );
    assert_eq!(expect_strings(output.result.unwrap()), vec!["cleaned"]);
}

// ── Reserved-name shape enforcement (E0144) ───────────────────────────────

#[tokio::test]
#[should_panic(expected = "[E0144]")]
async fn cleanup_with_extra_param_is_compile_error() {
    // `cleanup` is reserved for the magic finalizer shape `(self) -> void`; an
    // extra parameter is rejected rather than silently treated as an ordinary
    // method.
    let _ = baml_test!(
        r#"
class Bad {
  x int
  function cleanup(self, force: bool) -> void { }
}
function main() -> int { 0 }
"#
    );
}

#[tokio::test]
#[should_panic(expected = "[E0144]")]
async fn cleanup_with_non_void_return_is_compile_error() {
    // A non-`void` return type is not the reserved finalizer shape.
    let _ = baml_test!(
        r#"
class Bad {
  x int
  function cleanup(self) -> string { "nope" }
}
function main() -> int { 0 }
"#
    );
}

#[tokio::test]
#[should_panic(expected = "[E0144]")]
async fn cleanup_with_default_on_self_is_compile_error() {
    // Method parameters accept defaults during lowering, so `cleanup(self = ...)`
    // would otherwise pass the name/arity checks and be mistaken for the magic
    // finalizer. A default on `self` is meaningless (the finalizer is only ever
    // invoked with the receiver) and is not the reserved shape, so it must be
    // rejected rather than silently accepted as a finalizer.
    let _ = baml_test!(
        r#"
class Bad {
  x int
  function cleanup(self = null) -> void { }
}
function main() -> int { 0 }
"#
    );
}

#[tokio::test]
#[should_panic(expected = "[E0144]")]
async fn cleanup_with_throws_is_compile_error() {
    // A finalizer must not declare a `throws` contract — on the GC path the
    // error has no caller and is swallowed, so a propagating `cleanup` is the
    // wrong shape.
    let _ = baml_test!(
        r#"
class Bad {
  x int
  function cleanup(self) -> void throws baml.json.JsonParseError {
    throw baml.json.JsonParseError { message: "boom" }
  }
}
function main() -> int { 0 }
"#
    );
}

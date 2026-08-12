//! BEP-042 `cleanup` magic-method tests (Commit 1: the run-once latch).
//!
//! `cleanup` is recognized by name — a class that defines
//! `function cleanup(self) -> void { ... }` gets a finalizer whose body runs at
//! most once per instance, whether invoked explicitly or via `defer`. The
//! reserved-name shape-enforcement tests assert the [E0144] compile error via
//! `should_panic`, which is not expressible in BAML test blocks.

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

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

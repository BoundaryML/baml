//! BEP-042 Part 3 — `ErrorContext` cause-chain tests.
//!
//! Cause is computed lazily at the throw funnel: a throw whose PC lies in a
//! handler body chains onto that handler's caught error (`vm.rs`
//! `find_cause_context` + the handler-body extent on `ExceptionTableEntry`,
//! built from the catch region's `handler_body` blocks). See
//! `thoughts/antonio/errorcontext-impl-plan.md`.

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

#[track_caller]
fn expect_string(v: BexExternalValue) -> String {
    match v {
        BexExternalValue::String(s) => s.to_string(),
        other => panic!("expected String, got {other:?}"),
    }
}

/// ErrorContext rendering runs inside a native call, so it cannot yield to
/// user `ToString` implementations. It must still preserve the qualified
/// thrown class name and recursively render its fields.
#[tokio::test]
async fn class_error_renders_qualified_name_and_structural_fields() {
    let output = baml_test!(
        r#"
class Detail {
  label string

  implements baml.ToString {
    function to_string(self) -> string throws never { "DETAIL OVERRIDE" }
  }
}

class AppError {
  message string
  detail Detail

  implements baml.ToString {
    function to_string(self) -> string throws never { "ERROR OVERRIDE" }
  }
}

function main() -> string {
  (
    throw AppError {
      message: "boom",
      detail: Detail { label: "visible" },
    }
  ) catch (e, ctx) {
    _ => ctx.to_string()
  }
}
"#
    );
    let rendered = expect_string(output.result.unwrap());
    assert!(
        rendered.contains(
            r#"user.AppError { message: "boom", detail: user.Detail { label: "visible" } }"#
        ),
        "class error lost its identity or fields:\n{rendered}"
    );
    assert!(
        !rendered.contains("OVERRIDE"),
        "ErrorContext unexpectedly dispatched a user ToString override:\n{rendered}"
    );
}

/// Arbitrary thrown values are legal. Their context should retain the value's
/// structural representation rather than replacing it with `<error>`.
#[tokio::test]
async fn non_class_error_renders_structurally() {
    let output = baml_test!(
        r#"
function main() -> string {
  (throw {"first": "alpha", "second": "beta"}) catch (e, ctx) {
    _ => ctx.to_string()
  }
}
"#
    );
    let rendered = expect_string(output.result.unwrap());
    assert!(
        rendered.contains(r#"{"first": "alpha", "second": "beta"}"#),
        "non-class error was not rendered structurally:\n{rendered}"
    );
    assert!(
        !rendered.contains("<error>"),
        "non-class error fell back to the old placeholder:\n{rendered}"
    );
}

#[tokio::test]
async fn structural_error_rendering_truncates_object_cycles() {
    let output = baml_test!(
        r#"
class RecursiveError {
  next RecursiveError?
}

function fail() -> string {
  let error = RecursiveError { next: null }
  error.next = error
  throw error
}

function main() -> string {
  fail() catch (e, ctx) {
    _ => ctx.to_string()
  }
}
"#
    );
    let rendered = expect_string(output.result.unwrap());
    assert!(
        rendered.contains("user.RecursiveError { next: … }"),
        "cyclic class error was not safely truncated:\n{rendered}"
    );
}

#[tokio::test]
async fn structural_error_rendering_enforces_depth_and_node_budgets() {
    let output = baml_test!(
        r#"
class Link {
  next Link?
}

function fail_deep() -> string {
  let root = Link { next: null }
  let current = root
  let i = 0
  while (i < 40) {
    let next = Link { next: null }
    current.next = next
    current = next
    i += 1
  }
  throw root
}

function fail_wide() -> string {
  let values: int[] = []
  let i = 0
  while (i < 300) {
    values.push(i)
    i += 1
  }
  throw values
}

function render_deep() -> string {
  fail_deep() catch (e, ctx) {
    _ => ctx.to_string()
  }
}

function render_wide() -> string {
  fail_wide() catch (e, ctx) {
    _ => ctx.to_string()
  }
}

function main() -> string {
  render_deep() + "|" + render_wide()
}
"#
    );
    let rendered = expect_string(output.result.unwrap());
    let (deep, wide) = rendered
        .split_once('|')
        .expect("expected both bounded renderings");
    assert!(
        deep.contains('…'),
        "deep class error exceeded its budget without truncation:\n{deep}"
    );
    assert!(
        wide.contains('…'),
        "wide array error exceeded its budget without truncation:\n{wide}"
    );
    assert_eq!(
        wide.matches('…').count(),
        1,
        "wide array should use one truncation marker:\n{wide}"
    );
    assert!(
        wide.len() < 2_000,
        "wide array diagnostic should remain bounded, got {} bytes",
        wide.len()
    );
}

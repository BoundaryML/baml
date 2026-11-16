//! The type a generic function's type parameter holds, read at run time in a
//! function that stays a call: one site reads the parameter in frames that
//! bound it to different types, in turn and repeatedly, and each read must
//! give the type of the frame it runs in.
//!
//! Each program runs at `OptLevel::One` and `OptLevel::Two`, which must agree.

use baml_tests::engine::{OptLevel, run_test};
use bex_external_types::BexExternalValue;
use indexmap::IndexMap;

const SOURCE: &str = r#"
function kind(value: unknown) -> string {
  if (value is int[]) { "int[]" }
  else if (value is string[]) { "string[]" }
  else if (value is int[][]) { "int[][]" }
  else { "other" }
}

// Each generic function calls itself, so the optimizer leaves it a call.

function one<T>(v: T, again: bool) -> T[] {
  if (again) { return one(v, false); }
  let out: T[] = [];
  out.push(v);
  out
}
function element_types() -> string {
  let log = "";
  for (let i = 0; i < 3; i += 1) {
    log = `${log}${kind(one(i, false))},${kind(one("a", true))},${kind(one([i], false))},`;
  }
  log
}

function name<T>(v: T, again: bool) -> string {
  if (again) { return name(v, false); }
  reflect.Type.of<T>().to_string()
}
function parameter_names() -> string {
  let log = "";
  for (let i = 0; i < 3; i += 1) {
    log = `${log}${name(i, false)},${name("a", true)},${name(1.5, false)},`;
  }
  log
}

function pushed<T>(xs: T[], v: T, again: bool) -> int {
  if (again) { return pushed(xs, v, false); }
  xs.push(v)
}
function pushes() -> string {
  let ints: int[] = [];
  let texts: string[] = [];
  let log = "";
  for (let i = 0; i < 3; i += 1) {
    log = `${log}${pushed(ints, i, false)}${pushed(texts, "a", true)}`;
  }
  `${log}|${kind(ints)}|${kind(texts)}|${ints.length()}|${texts.length()}`
}
"#;

/// Run `entry` at both levels and return what it gave, which must agree.
async fn text(entry: &str) -> String {
    let at = |opt| async move {
        run_test(SOURCE, entry, IndexMap::new(), opt)
            .await
            .result
            .map_err(|error| error.to_string())
    };
    let out_of_line = at(OptLevel::One).await;
    let optimized = at(OptLevel::Two).await;
    assert_eq!(out_of_line, optimized, "{entry}: O1 against O2");
    match out_of_line {
        Ok(BexExternalValue::String(text)) => text.to_string(),
        other => panic!("{entry} returns a string, got {other:?}"),
    }
}

#[tokio::test]
async fn an_array_built_in_a_generic_frame_has_that_frames_element_type() {
    assert_eq!(
        text("element_types").await,
        "int[],string[],int[][],".repeat(3)
    );
}

#[tokio::test]
async fn a_type_parameter_reflects_the_type_its_frame_bound() {
    assert_eq!(text("parameter_names").await, "int,string,float,".repeat(3));
}

#[tokio::test]
async fn a_native_called_with_a_type_parameter_acts_on_its_receiver() {
    assert_eq!(text("pushes").await, "112233|int[]|string[]|3|3");
}

//! Runtime tests for the BEP-021 `baml.time` date/time family live in
//! `baml_src/ns_time/datetime.baml` as pure-BAML tests. This file holds only
//! tests that currently cannot run under the CLI pipeline: optional
//! class-typed arguments to native functions are dropped there (canary
//! regression from the `Ty::Optional` → `T | null` lowering, also
//! reproducible with `baml.sys.exec`'s `ProcessOptions?`). The engine-API
//! harness used here is unaffected. Move these to the BAML file once that
//! is fixed.

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

/// `PlainDate.to_plain_datetime` with an explicit `PlainTime` argument (the
/// `PlainTime?` parameter must arrive at the native, not default to
/// midnight).
#[tokio::test]
async fn plain_date_to_plain_datetime_with_time() {
    let output = baml_test!(
        r#"
        function main() -> string {
            let d = baml.time.PlainDate.parse("1979-05-27");
            let t = baml.time.PlainTime.parse("07:32:00");
            d.to_plain_datetime(t).to_string()
        }
    "#
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String(
            "1979-05-27T07:32:00".to_string().into()
        ))
    );
}

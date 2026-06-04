//! Runtime tests for the BEP-021 `baml.time` date/time family:
//! `ZonedDateTime`, `PlainDateTime`, `PlainDate`, `PlainTime`, and
//! `TimeZoneOffset`.
//!
//! Three layers are exercised:
//! - Pure natives in `crates/bex_vm/src/package_baml/time.rs`: parsing
//!   (RFC 3339 / RFC 9557 / zoneless ISO 8601), formatting, and calendar
//!   math.
//! - Pure-BAML compositions in `ns_time/*.baml`: `to_zoned` with fixed
//!   offsets, `to_plain`, `with_timezone`, max/min, component accessors.
//! - The timezone-database io path (`_tz_offset_at` / `_tz_to_instant`,
//!   implemented natively via the host's zoneinfo through `jiff`): IANA
//!   offset resolution and TC39 DST disambiguation. These use
//!   `America/Los_Angeles`, whose 2011 transitions (a gap on Mar 13, an
//!   overlap on Nov 6) are the canonical TC39 examples and are stable
//!   tzdb history.

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

/// Assert the most recent BAML execution failed with `UnhandledThrow` whose
/// payload is an `Instance` of `expected_class`.
fn assert_throw_class(
    result: &Result<BexExternalValue, bex_engine::EngineError>,
    expected_class: &str,
) {
    let Err(bex_engine::EngineError::UnhandledThrow { value, .. }) = result else {
        panic!("expected UnhandledThrow({expected_class}), got: {result:?}");
    };
    let BexExternalValue::Instance { class_name, .. } = value.as_ref() else {
        panic!("expected throw Instance({expected_class}), got: {value:?}");
    };
    assert_eq!(class_name, expected_class);
}

fn assert_string_result(
    result: &Result<BexExternalValue, bex_engine::EngineError>,
    expected: &str,
) {
    assert_eq!(
        *result,
        Ok(BexExternalValue::String(expected.to_string().into()))
    );
}

// ─── PlainDate ───────────────────────────────────────────────────────────────

#[tokio::test]
async fn plain_date_round_trip_and_components() {
    let output = baml_test!(
        r#"
        function main() -> string {
            let d = baml.time.PlainDate.parse("1979-05-27");
            let built = baml.time.PlainDate.from_components(1979, 5, 27);
            if (built.to_string() != d.to_string()) {
                baml.sys.panic("from_components and parse disagree");
            }
            baml.json.stringify([d.to_string(), d.year(), d.month(), d.day()])
        }
    "#
    );
    assert_string_result(&output.result, r#"["1979-05-27",1979,5,27]"#);
}

#[tokio::test]
async fn plain_date_rejects_invalid() {
    let output = baml_test!(
        r#"
        function main() -> baml.time.PlainDate {
            baml.time.PlainDate.from_components(2026, 2, 30)
        }
    "#
    );
    assert_throw_class(&output.result, "baml.errors.InvalidArgument");
}

#[tokio::test]
async fn plain_date_to_plain_datetime() {
    let output = baml_test!(
        r#"
        function main() -> string {
            let d = baml.time.PlainDate.parse("1979-05-27");
            let t = baml.time.PlainTime.parse("07:32:00");
            d.to_plain_datetime(t).to_string() + " " + d.to_plain_datetime(null).to_string()
        }
    "#
    );
    assert_string_result(&output.result, "1979-05-27T07:32:00 1979-05-27T00:00:00");
}

// ─── PlainTime ───────────────────────────────────────────────────────────────

#[tokio::test]
async fn plain_time_round_trip_subseconds() {
    let output = baml_test!(
        r#"
        function main() -> string {
            let t = baml.time.PlainTime.parse("07:32:00.5");
            let built = baml.time.PlainTime.from_components(7, 32, 0, 500, null, null);
            if (built.to_string() != t.to_string()) {
                baml.sys.panic("from_components and parse disagree");
            }
            baml.json.stringify([t.to_string(), t.hour(), t.minute(), t.second(), t.millisecond()])
        }
    "#
    );
    assert_string_result(&output.result, r#"["07:32:00.5",7,32,0,500]"#);
}

#[tokio::test]
async fn plain_time_rejects_out_of_range() {
    let output = baml_test!(
        r#"
        function main() -> baml.time.PlainTime {
            baml.time.PlainTime.from_components(24, null, null, null, null, null)
        }
    "#
    );
    assert_throw_class(&output.result, "baml.errors.InvalidArgument");
}

// ─── PlainDateTime ───────────────────────────────────────────────────────────

#[tokio::test]
async fn plain_datetime_round_trip_and_components() {
    let output = baml_test!(
        r#"
        function main() -> string {
            let dt = baml.time.PlainDateTime.parse("1979-05-27T07:32:00");
            baml.json.stringify([
                dt.to_string(),
                dt.year(), dt.month(), dt.day(),
                dt.hour(), dt.minute(), dt.second(),
                dt.to_plain_date().to_string(),
                dt.to_plain_time().to_string(),
            ])
        }
    "#
    );
    assert_string_result(
        &output.result,
        r#"["1979-05-27T07:32:00",1979,5,27,7,32,0,"1979-05-27","07:32:00"]"#,
    );
}

/// Pre-epoch civil values exercise the euclidean day/time split.
#[tokio::test]
async fn plain_datetime_pre_epoch() {
    let output = baml_test!(
        r#"
        function main() -> string {
            let dt = baml.time.PlainDateTime.from_components(1969, 12, 31, 23, 59, 59, null, null, null);
            dt.to_string() + " " + dt.to_plain_date().to_string() + " " + dt.to_plain_time().to_string()
        }
    "#
    );
    assert_string_result(&output.result, "1969-12-31T23:59:59 1969-12-31 23:59:59");
}

/// Offset-carrying strings belong to `ZonedDateTime.parse`.
#[tokio::test]
async fn plain_datetime_rejects_offset_string() {
    let output = baml_test!(
        r#"
        function main() -> baml.time.PlainDateTime {
            baml.time.PlainDateTime.parse("1979-05-27T07:32:00Z")
        }
    "#
    );
    assert_throw_class(&output.result, "baml.errors.ParseError");
}

// ─── ZonedDateTime: fixed offsets (no timezone database involved) ────────────

#[tokio::test]
async fn zoned_parse_fixed_offset_round_trip() {
    let output = baml_test!(
        r#"
        function main() -> string {
            let z = baml.time.ZonedDateTime.parse("2026-03-18T13:04:27-07:00");
            baml.json.stringify([z.to_string(), z.to_instant().to_string(), z.timezone_offset().hours()])
        }
    "#
    );
    assert_string_result(
        &output.result,
        r#"["2026-03-18T13:04:27-07:00","2026-03-18T20:04:27Z",-7]"#,
    );
}

#[tokio::test]
async fn zoned_zero_offset_prints_z() {
    let output = baml_test!(
        r#"
        function main() -> string {
            baml.time.ZonedDateTime.parse("2020-01-01T00:00:00Z").to_string()
        }
    "#
    );
    assert_string_result(&output.result, "2020-01-01T00:00:00Z");
}

/// Zoneless strings are `PlainDateTime`'s job.
#[tokio::test]
async fn zoned_rejects_zoneless_string() {
    let output = baml_test!(
        r#"
        function main() -> baml.time.ZonedDateTime {
            baml.time.ZonedDateTime.parse("1979-05-27T07:32:00")
        }
    "#
    );
    assert_throw_class(&output.result, "baml.errors.ParseError");
}

#[tokio::test]
async fn zoned_to_plain_and_with_timezone() {
    let output = baml_test!(
        r#"
        function main() -> string {
            let z = baml.time.ZonedDateTime.parse("2026-03-18T13:04:27-07:00");
            // Same absolute time relabeled: wall clock moves with the offset.
            let utc = z.with_timezone(baml.time.TimeZoneOffset.utc());
            z.to_plain().to_string() + " " + utc.to_plain().to_string()
        }
    "#
    );
    assert_string_result(&output.result, "2026-03-18T13:04:27 2026-03-18T20:04:27");
}

#[tokio::test]
async fn plain_to_zoned_fixed_offset() {
    let output = baml_test!(
        r#"
        function main() -> string {
            let dt = baml.time.PlainDateTime.parse("1979-05-27T00:32:00");
            // Locating a wall-clock reading at -07:00 lands 7h later in UTC.
            dt.to_zoned(baml.time.TimeZoneOffset.new(-7, 0), null).to_instant().to_string()
        }
    "#
    );
    assert_string_result(&output.result, "1979-05-27T07:32:00Z");
}

#[tokio::test]
async fn zoned_from_components_fixed_offset() {
    let output = baml_test!(
        r#"
        function main() -> string {
            baml.time.ZonedDateTime.from_components(
                baml.time.TimeZoneOffset.new(5, 30),
                2026, 3, 18, 13, 4, 27, null, null, null, null
            ).to_string()
        }
    "#
    );
    assert_string_result(&output.result, "2026-03-18T13:04:27+05:30");
}

// ─── ZonedDateTime: IANA identifiers (host timezone database via jiff) ───────

#[tokio::test]
async fn zoned_parse_iana_annotation_resolves_offset() {
    let output = baml_test!(
        r#"
        function main() -> string {
            // Winter: America/Los_Angeles is PST (-08:00).
            let z = baml.time.ZonedDateTime.parse("2020-01-15T12:00:00-08:00[America/Los_Angeles]");
            baml.json.stringify([z.to_string(), z.timezone_offset().hours()])
        }
    "#
    );
    assert_string_result(
        &output.result,
        r#"["2020-01-15T12:00:00-08:00[America/Los_Angeles]",-8]"#,
    );
}

/// The IANA identifier is DST-aware: the same zone resolves to a different
/// offset in summer.
#[tokio::test]
async fn zoned_iana_offset_is_dst_aware() {
    let output = baml_test!(
        r#"
        function main() -> string {
            let winter = baml.time.ZonedDateTime.parse("2020-01-15T12:00:00-08:00[America/Los_Angeles]");
            let summer = baml.time.ZonedDateTime.from_instant(
                baml.time.Instant.parse("2020-07-15T19:00:00Z"),
                "America/Los_Angeles"
            );
            baml.json.stringify([winter.timezone_offset().hours(), summer.timezone_offset().hours()])
        }
    "#
    );
    assert_string_result(&output.result, "[-8,-7]");
}

#[tokio::test]
async fn unknown_timezone_throws() {
    let output = baml_test!(
        r#"
        function main() -> baml.time.TimeZoneOffset {
            baml.time.ZonedDateTime.parse("2020-01-15T12:00:00-08:00[Not/AZone]").timezone_offset()
        }
    "#
    );
    assert_throw_class(&output.result, "baml.time.UnknownTimezoneError");
}

/// TC39 disambiguation across the 2011-03-13 DST gap in America/Los_Angeles:
/// 02:15 never happened. "compatible" picks the later side for gaps.
#[tokio::test]
async fn to_zoned_dst_gap_disambiguation() {
    let output = baml_test!(
        r#"
        function main() -> string {
            let dt = baml.time.PlainDateTime.parse("2011-03-13T02:15:00");
            let compatible = dt.to_zoned("America/Los_Angeles", null);
            let earlier = dt.to_zoned("America/Los_Angeles", "earlier");
            compatible.to_instant().to_string() + " " + earlier.to_instant().to_string()
        }
    "#
    );
    assert_string_result(&output.result, "2011-03-13T10:15:00Z 2011-03-13T09:15:00Z");
}

/// TC39 disambiguation across the 2011-11-06 DST overlap: 01:15 happened
/// twice. "compatible" picks the earlier side for overlaps; "reject" throws.
#[tokio::test]
async fn to_zoned_dst_overlap_disambiguation() {
    let output = baml_test!(
        r#"
        function main() -> string {
            let dt = baml.time.PlainDateTime.parse("2011-11-06T01:15:00");
            let compatible = dt.to_zoned("America/Los_Angeles", null);
            let later = dt.to_zoned("America/Los_Angeles", "later");
            compatible.to_instant().to_string() + " " + later.to_instant().to_string()
        }
    "#
    );
    assert_string_result(&output.result, "2011-11-06T08:15:00Z 2011-11-06T09:15:00Z");
}

#[tokio::test]
async fn to_zoned_reject_throws_on_ambiguity() {
    let output = baml_test!(
        r#"
        function main() -> baml.time.ZonedDateTime {
            baml.time.PlainDateTime.parse("2011-11-06T01:15:00")
                .to_zoned("America/Los_Angeles", "reject")
        }
    "#
    );
    assert_throw_class(&output.result, "baml.time.AmbiguousTimeError");
}

#[tokio::test]
async fn to_zoned_reject_accepts_unambiguous() {
    let output = baml_test!(
        r#"
        function main() -> string {
            baml.time.PlainDateTime.parse("2011-11-06T12:00:00")
                .to_zoned("America/Los_Angeles", "reject")
                .to_instant()
                .to_string()
        }
    "#
    );
    assert_string_result(&output.result, "2011-11-06T20:00:00Z");
}

// ─── TOML interop: the four datetime kinds in one document ──────────────────

#[tokio::test]
async fn toml_four_datetime_kinds() {
    let output = baml_test!(
        r#"
        function main() -> string {
            let t = baml.toml.Table.parse("odt = 1979-05-27T00:32:00-07:00\nldt = 1979-05-27T07:32:00\nld = 1979-05-27\nlt = 07:32:00.5\n");
            baml.json.stringify(t.to_json())
        }
    "#
    );
    assert_string_result(
        &output.result,
        r#"{"ld":"1979-05-27","ldt":"1979-05-27T07:32:00","lt":"07:32:00.5","odt":"1979-05-27T00:32:00-07:00"}"#,
    );
}

// Tests for the BEP-021 `baml.time` date/time family: `ZonedDateTime`,
// `PlainDateTime`, `PlainDate`, `PlainTime`, and `TimeZoneOffset`.
//
// Converted from crates/baml_tests/tests/datetime.rs. Three layers are
// exercised:
// - Pure natives in `crates/bex_vm/src/package_baml/time.rs`: parsing
//   (RFC 3339 / RFC 9557 / zoneless ISO 8601), formatting, calendar math.
// - Pure-BAML compositions in `ns_time/*.baml`: `to_zoned` with fixed
//   offsets, `to_plain`, `with_timezone`, component accessors.
// - The timezone-database io path (`_tz_offset_at` / `_tz_to_instant`,
//   natively via the host's zoneinfo through `jiff`): IANA offset
//   resolution and TC39 DST disambiguation. These use
//   `America/Los_Angeles`, whose 2011 transitions (a gap on Mar 13, an
//   overlap on Nov 6) are the canonical TC39 examples and stable tzdb
//   history.
//
// Throw tests use the `{ <body>; baml.sys.panic("…") } catch (e) { Class =>
// null }` pattern (cf. ns_time/time.baml): a wrong or missing throw fails
// the test.

// ─── PlainDate ───────────────────────────────────────────────────────────────

test "plain_date_parse_round_trip" {
    assert.equal(baml.time.PlainDate.parse("1979-05-27").to_string(), "1979-05-27")
}

test "plain_date_from_components_matches_parse" {
    assert.equal(
        baml.time.PlainDate.from_components(1979, 5, 27).to_string(),
        baml.time.PlainDate.parse("1979-05-27").to_string(),
    )
}

test "plain_date_components" {
    let d = baml.time.PlainDate.parse("1979-05-27");
    assert.equal(d.year(), 1979);
    assert.equal(d.month(), 5);
    assert.equal(d.day(), 27)
}

test "plain_date_rejects_invalid" {
    {
        baml.time.PlainDate.from_components(2026, 2, 30);
        baml.sys.panic("expected PlainDate.from_components(2026, 2, 30) to throw")
    } catch (e) {
        baml.errors.InvalidArgument => null
    }
}

// Hoisted into a function: locals inside `test` blocks are boxed by the
// closure capture (known VM bug, see the ns_closures/closures.baml header),
// and a boxed local passed to a union-typed parameter (here `PlainTime?`)
// currently arrives as null.
function plain_date_to_plain_datetime_with_time() -> string {
    let d = baml.time.PlainDate.parse("1979-05-27");
    let t = baml.time.PlainTime.parse("07:32:00");
    d.to_plain_datetime(time = t).to_string()
}
test "plain_date_to_plain_datetime" {
    assert.equal(plain_date_to_plain_datetime_with_time(), "1979-05-27T07:32:00");
    // Omitted time defaults to midnight.
    assert.equal(
        baml.time.PlainDate.parse("1979-05-27").to_plain_datetime().to_string(),
        "1979-05-27T00:00:00",
    )
}

// ─── PlainTime ───────────────────────────────────────────────────────────────

test "plain_time_parse_round_trip_subseconds" {
    assert.equal(baml.time.PlainTime.parse("07:32:00.5").to_string(), "07:32:00.5")
}

test "plain_time_from_components_matches_parse" {
    assert.equal(
        baml.time.PlainTime.from_components(7, minute = 32, millisecond = 500).to_string(),
        baml.time.PlainTime.parse("07:32:00.5").to_string(),
    )
}

test "plain_time_components" {
    let t = baml.time.PlainTime.parse("07:32:00.5");
    assert.equal(t.hour(), 7);
    assert.equal(t.minute(), 32);
    assert.equal(t.second(), 0);
    assert.equal(t.millisecond(), 500)
}

test "plain_time_rejects_out_of_range" {
    {
        baml.time.PlainTime.from_components(24);
        baml.sys.panic("expected PlainTime.from_components(24, …) to throw")
    } catch (e) {
        baml.errors.InvalidArgument => null
    }
}

// ─── PlainDateTime ───────────────────────────────────────────────────────────

test "plain_datetime_parse_round_trip" {
    assert.equal(
        baml.time.PlainDateTime.parse("1979-05-27T07:32:00").to_string(),
        "1979-05-27T07:32:00",
    )
}

test "plain_datetime_components_and_projections" {
    let dt = baml.time.PlainDateTime.parse("1979-05-27T07:32:00");
    assert.equal(dt.year(), 1979);
    assert.equal(dt.month(), 5);
    assert.equal(dt.day(), 27);
    assert.equal(dt.hour(), 7);
    assert.equal(dt.minute(), 32);
    assert.equal(dt.second(), 0);
    assert.equal(dt.to_plain_date().to_string(), "1979-05-27");
    assert.equal(dt.to_plain_time().to_string(), "07:32:00")
}

// Pre-epoch civil values exercise the euclidean day/time split.
test "plain_datetime_pre_epoch" {
    let dt = baml.time.PlainDateTime.from_components(1969, 12, 31, hour = 23, minute = 59, second = 59);
    assert.equal(dt.to_string(), "1969-12-31T23:59:59");
    assert.equal(dt.to_plain_date().to_string(), "1969-12-31");
    assert.equal(dt.to_plain_time().to_string(), "23:59:59")
}

// Offset-carrying strings belong to `ZonedDateTime.parse`.
test "plain_datetime_rejects_offset_string" {
    {
        baml.time.PlainDateTime.parse("1979-05-27T07:32:00Z");
        baml.sys.panic("expected PlainDateTime.parse of an offset string to throw")
    } catch (e) {
        baml.errors.ParseError => null
    }
}

// ─── ZonedDateTime: fixed offsets (no timezone database involved) ────────────

test "zoned_parse_fixed_offset_round_trip" {
    let z = baml.time.ZonedDateTime.parse("2026-03-18T13:04:27-07:00");
    assert.equal(z.to_string(), "2026-03-18T13:04:27-07:00");
    assert.equal(z.to_instant().to_string(), "2026-03-18T20:04:27Z");
    assert.equal(z.timezone_offset().hours(), -7)
}

test "zoned_zero_offset_prints_z" {
    assert.equal(
        baml.time.ZonedDateTime.parse("2020-01-01T00:00:00Z").to_string(),
        "2020-01-01T00:00:00Z",
    )
}

// Zoneless strings are `PlainDateTime`'s job.
test "zoned_rejects_zoneless_string" {
    {
        baml.time.ZonedDateTime.parse("1979-05-27T07:32:00");
        baml.sys.panic("expected ZonedDateTime.parse of a zoneless string to throw")
    } catch (e) {
        baml.errors.ParseError => null
    }
}

test "zoned_to_plain_and_with_timezone" {
    let z = baml.time.ZonedDateTime.parse("2026-03-18T13:04:27-07:00");
    assert.equal(z.to_plain().to_string(), "2026-03-18T13:04:27");
    // Same absolute time relabeled: the wall clock moves with the offset.
    let utc = z.with_timezone(baml.time.TimeZoneOffset.utc());
    assert.equal(utc.to_plain().to_string(), "2026-03-18T20:04:27")
}

test "plain_to_zoned_fixed_offset" {
    let dt = baml.time.PlainDateTime.parse("1979-05-27T00:32:00");
    // Locating a wall-clock reading at -07:00 lands 7h later in UTC.
    assert.equal(
        dt.to_zoned(baml.time.TimeZoneOffset.new(-7, 0)).to_instant().to_string(),
        "1979-05-27T07:32:00Z",
    )
}

test "zoned_from_components_fixed_offset" {
    assert.equal(
        baml.time.ZonedDateTime.from_components(
            baml.time.TimeZoneOffset.new(5, 30),
            2026, 3, 18, hour = 13, minute = 4, second = 27,
        ).to_string(),
        "2026-03-18T13:04:27+05:30",
    )
}

// Mixed-sign components are ambiguous (is `new(5, -30)` 4.5h or 5.5h east?)
// and rejected, matching `time::UtcOffset::from_hms` semantics.
test "timezone_offset_rejects_mixed_signs" {
    {
        baml.time.TimeZoneOffset.new(5, -30);
        baml.sys.panic("expected TimeZoneOffset.new(5, -30) to throw")
    } catch (e) {
        baml.errors.InvalidArgument => null
    }
}

// ─── ZonedDateTime: IANA identifiers (host timezone database) ────────────────

test "zoned_parse_iana_annotation_resolves_offset" {
    // Winter: America/Los_Angeles is PST (-08:00).
    let z = baml.time.ZonedDateTime.parse("2020-01-15T12:00:00-08:00[America/Los_Angeles]");
    assert.equal(z.to_string(), "2020-01-15T12:00:00-08:00[America/Los_Angeles]");
    assert.equal(z.timezone_offset().hours(), -8)
}

// The IANA identifier is DST-aware: the same zone resolves to a different
// offset in summer.
test "zoned_iana_offset_is_dst_aware" {
    let winter = baml.time.ZonedDateTime.parse("2020-01-15T12:00:00-08:00[America/Los_Angeles]");
    let summer = baml.time.ZonedDateTime.from_instant(
        baml.time.Instant.parse("2020-07-15T19:00:00Z"),
        "America/Los_Angeles"
    );
    assert.equal(winter.timezone_offset().hours(), -8);
    assert.equal(summer.timezone_offset().hours(), -7)
}

test "unknown_timezone_throws" {
    {
        baml.time.ZonedDateTime.parse("2020-01-15T12:00:00-08:00[Not/AZone]").timezone_offset();
        baml.sys.panic("expected timezone_offset of an unknown zone to throw")
    } catch (e) {
        baml.time.UnknownTimezoneError => null
    }
}

// TC39 disambiguation across the 2011-03-13 DST gap in America/Los_Angeles:
// 02:15 never happened. "compatible" picks the later side for gaps.
test "to_zoned_dst_gap_disambiguation" {
    let dt = baml.time.PlainDateTime.parse("2011-03-13T02:15:00");
    assert.equal(
        dt.to_zoned("America/Los_Angeles").to_instant().to_string(),
        "2011-03-13T10:15:00Z",
    );
    assert.equal(
        dt.to_zoned("America/Los_Angeles", disambiguation = "earlier").to_instant().to_string(),
        "2011-03-13T09:15:00Z",
    )
}

// TC39 disambiguation across the 2011-11-06 DST overlap: 01:15 happened
// twice. "compatible" picks the earlier side for overlaps.
test "to_zoned_dst_overlap_disambiguation" {
    let dt = baml.time.PlainDateTime.parse("2011-11-06T01:15:00");
    assert.equal(
        dt.to_zoned("America/Los_Angeles").to_instant().to_string(),
        "2011-11-06T08:15:00Z",
    );
    assert.equal(
        dt.to_zoned("America/Los_Angeles", disambiguation = "later").to_instant().to_string(),
        "2011-11-06T09:15:00Z",
    )
}

test "to_zoned_reject_throws_on_ambiguity" {
    {
        baml.time.PlainDateTime.parse("2011-11-06T01:15:00")
            .to_zoned("America/Los_Angeles", disambiguation = "reject");
        baml.sys.panic("expected to_zoned(…, \"reject\") of an ambiguous time to throw")
    } catch (e) {
        baml.time.AmbiguousTimeError => null
    }
}

test "to_zoned_reject_accepts_unambiguous" {
    assert.equal(
        baml.time.PlainDateTime.parse("2011-11-06T12:00:00")
            .to_zoned("America/Los_Angeles", disambiguation = "reject")
            .to_instant()
            .to_string(),
        "2011-11-06T20:00:00Z",
    )
}

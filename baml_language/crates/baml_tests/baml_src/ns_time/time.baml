// Tests for the `baml.time` namespace (`Instant` / `Duration`).
//
// Converted from crates/baml_tests/tests/time.rs. Values are nanoseconds since
// the UNIX epoch, carried as `bigint`, so they're compared with
// `assert.equal`; boolean results are asserted directly.
//
// `baml.time.Instant.now()` is a host-bound `$rust_io_function`; its value is
// non-deterministic, so it's only sanity-checked against a lower bound.
//
// The parse-failure throw uses the `{ <body>; baml.sys.panic("…") } catch (e)
// { baml.errors.ParseError => null }` pattern. `Instant.parse` declares
// `throws root.errors.ParseError`, which is the same type caught as
// `baml.errors.ParseError` (cf. `ns_bigints/bigints.baml`).

// ─── Instant.parse (RFC 3339) ───────────────────────────────────────────────

test "instant_parse_epoch" {
    assert.equal(baml.time.Instant.parse("1970-01-01T00:00:00Z").to_timestamp_nanoseconds(), 0n)
}

test "instant_parse_one_second" {
    assert.equal(
        baml.time.Instant.parse("1970-01-01T00:00:01Z").to_timestamp_nanoseconds(),
        1000000000n,
    )
}

test "instant_parse_fractional_seconds" {
    assert.equal(
        baml.time.Instant.parse("1970-01-01T00:00:00.5Z").to_timestamp_nanoseconds(),
        500000000n,
    )
}

test "instant_parse_honors_timezone_offset" {
    // 01:00 at +01:00 is midnight UTC — the epoch.
    assert.equal(
        baml.time.Instant.parse("1970-01-01T01:00:00+01:00").to_timestamp_nanoseconds(),
        0n,
    )
}

test "instant_parse_iso8601_basic_no_separators" {
    // The separator-less basic form is rejected by RFC 3339 and falls through
    // to the ISO 8601 parser.
    assert.equal(baml.time.Instant.parse("19700101T000000Z").to_timestamp_nanoseconds(), 0n)
}

test "instant_parse_rfc2822_epoch" {
    assert.equal(
        baml.time.Instant.parse("Thu, 01 Jan 1970 00:00:00 +0000").to_timestamp_nanoseconds(),
        0n,
    )
}

test "instant_parse_rfc2822_honors_offset" {
    // 10:52:37 at +02:00 is 08:52:37 UTC — the same instant as the RFC 3339 form.
    assert.equal(
        baml.time.Instant.parse("Tue, 01 Jul 2003 10:52:37 +0200").to_timestamp_nanoseconds(),
        baml.time.Instant.parse("2003-07-01T10:52:37+02:00").to_timestamp_nanoseconds(),
    )
}

test "instant_parse_invalid_throws_parse_error" {
    {
        baml.time.Instant.parse("not a timestamp");
        baml.sys.panic("expected Instant.parse(\"not a timestamp\") to throw")
    } catch (e) {
        baml.errors.ParseError => null
    }
}

// ─── Instant.now (host-bound IO) ─────────────────────────────────────────────

test "instant_now_returns_recent_wall_clock" {
    // Can't assert an exact value; sanity-check the clock is past 2023-11-14
    // (1_700_000_000s since epoch).
    assert.is_true(baml.time.Instant.now().to_timestamp_seconds() > 1700000000n)
}

// ─── Duration ────────────────────────────────────────────────────────────────

test "duration_from_seconds_to_milliseconds" {
    assert.equal(baml.time.Duration.from_seconds(2n).to_milliseconds(), 2000n)
}

test "duration_constructors_accept_int_and_bigint" {
    assert.equal(baml.time.Duration.from_nanoseconds(2).to_nanoseconds(), 2n)
    assert.equal(baml.time.Duration.from_nanoseconds(2n).to_nanoseconds(), 2n)
    assert.equal(baml.time.Duration.from_microseconds(2).to_nanoseconds(), 2000n)
    assert.equal(baml.time.Duration.from_microseconds(2n).to_nanoseconds(), 2000n)
    assert.equal(baml.time.Duration.from_milliseconds(2).to_nanoseconds(), 2000000n)
    assert.equal(baml.time.Duration.from_milliseconds(2n).to_nanoseconds(), 2000000n)
    assert.equal(baml.time.Duration.from_seconds(2).to_nanoseconds(), 2000000000n)
    assert.equal(baml.time.Duration.from_seconds(2n).to_nanoseconds(), 2000000000n)
    assert.equal(baml.time.Duration.from_minutes(2).to_nanoseconds(), 120000000000n)
    assert.equal(baml.time.Duration.from_minutes(2n).to_nanoseconds(), 120000000000n)
    assert.equal(baml.time.Duration.from_hours(2).to_nanoseconds(), 7200000000000n)
    assert.equal(baml.time.Duration.from_hours(2n).to_nanoseconds(), 7200000000000n)
}

test "duration_since_is_nonnegative_for_past_instant" {
    assert.is_true(
        baml.time.Instant.parse("1970-01-01T00:00:00Z").elapsed().to_nanoseconds() > 0n
    )
}

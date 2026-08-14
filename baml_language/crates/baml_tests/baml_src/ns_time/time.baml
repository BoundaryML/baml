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

// ─── Operators ───────────────────────────────────────────────────────────────
//
// `baml.time` implements the `baml.ops` arithmetic interfaces, so `+` / `-` /
// `*` / `/` / `%` / unary `-` work on these types. The shapes are:
//
//   Duration      ±  Duration      -> Duration      (also `%`, and unary `-`)
//   Duration      */ int|bigint    -> Duration
//   Instant       ±  Duration      -> Instant
//   Instant       -  Instant       -> Duration
//   PlainDateTime ±  Duration      -> PlainDateTime
//   PlainDateTime -  PlainDateTime -> Duration
//   PlainTime     ±  Duration      -> PlainTime     (wraps at midnight)
//   PlainTime     -  PlainTime     -> Duration      (signed, does not wrap)
//   ZonedDateTime ±  Duration      -> ZonedDateTime (timezone preserved)
//   ZonedDateTime -  ZonedDateTime -> Duration
//
// `*` and `/` are covered for both `int` and `bigint` operands, since
// `Duration` carries a separate impl for each.

// ─── Duration arithmetic ─────────────────────────────────────────────────────

test "duration_add" {
    let sum = baml.time.Duration.from_seconds(10n) + baml.time.Duration.from_seconds(5n);
    assert.equal(sum.to_seconds(), 15n)
}

test "duration_sub" {
    let diff = baml.time.Duration.from_seconds(10n) - baml.time.Duration.from_seconds(4n);
    assert.equal(diff.to_seconds(), 6n)
}

// Subtraction is signed: a larger subtrahend yields a negative duration.
test "duration_sub_goes_negative" {
    let diff = baml.time.Duration.from_seconds(4n) - baml.time.Duration.from_seconds(10n);
    assert.equal(diff.to_seconds(), -6n)
}

test "duration_mul_by_int_and_bigint" {
    assert.equal((baml.time.Duration.from_seconds(3n) * 4).to_seconds(), 12n);
    assert.equal((baml.time.Duration.from_seconds(3n) * 4n).to_seconds(), 12n)
}

test "duration_mul_by_zero_is_zero" {
    assert.equal((baml.time.Duration.from_hours(5n) * 0).to_nanoseconds(), 0n)
}

test "duration_div_by_int_and_bigint" {
    assert.equal((baml.time.Duration.from_seconds(12n) / 4).to_seconds(), 3n);
    assert.equal((baml.time.Duration.from_seconds(12n) / 4n).to_seconds(), 3n)
}

// Division is integer division on the nanosecond count, so it truncates rather
// than rounding: 10ns / 4 is 2ns, not 2.5ns.
test "duration_div_truncates" {
    assert.equal((baml.time.Duration.from_nanoseconds(10n) / 4).to_nanoseconds(), 2n)
}

// `%` takes a `Duration` on the right, so it answers "what is left over after
// whole intervals of `other`" — 10s in steps of 3s leaves 1s.
test "duration_rem" {
    let rem = baml.time.Duration.from_seconds(10n) % baml.time.Duration.from_seconds(3n);
    assert.equal(rem.to_seconds(), 1n)
}

// The operands need not share a unit; both reduce to nanoseconds first.
test "duration_rem_across_units" {
    let rem = baml.time.Duration.from_milliseconds(1500n)
        % baml.time.Duration.from_seconds(1n);
    assert.equal(rem.to_milliseconds(), 500n)
}

test "duration_rem_divides_evenly_is_zero" {
    let rem = baml.time.Duration.from_hours(2n) % baml.time.Duration.from_minutes(30n);
    assert.equal(rem.to_nanoseconds(), 0n)
}

// `%` keeps the sign of the left operand, so a negative duration leaves a
// negative remainder rather than wrapping up to a positive one.
test "duration_rem_keeps_the_sign_of_the_left_operand" {
    let rem = -baml.time.Duration.from_seconds(10n) % baml.time.Duration.from_seconds(3n);
    assert.equal(rem.to_seconds(), -1n)
}

test "duration_negate" {
    assert.equal((-baml.time.Duration.from_seconds(7n)).to_seconds(), -7n)
}

test "duration_negate_is_an_involution" {
    assert.equal((-(-baml.time.Duration.from_seconds(7n))).to_seconds(), 7n)
}

// ─── Instant arithmetic ──────────────────────────────────────────────────────

test "instant_add_duration" {
    let t = baml.time.Instant.from_timestamp_seconds(1000n)
        + baml.time.Duration.from_seconds(30n);
    assert.equal(t.to_timestamp_seconds(), 1030n)
}

test "instant_sub_duration" {
    let t = baml.time.Instant.from_timestamp_seconds(1000n)
        - baml.time.Duration.from_seconds(30n);
    assert.equal(t.to_timestamp_seconds(), 970n)
}

test "instant_sub_instant_is_a_duration" {
    let gap = baml.time.Instant.from_timestamp_seconds(1000n)
        - baml.time.Instant.from_timestamp_seconds(400n);
    assert.equal(gap.to_seconds(), 600n)
}

// The gap is signed, so an earlier minuend gives a negative duration.
test "instant_sub_later_instant_is_negative" {
    let gap = baml.time.Instant.from_timestamp_seconds(400n)
        - baml.time.Instant.from_timestamp_seconds(1000n);
    assert.equal(gap.to_seconds(), -600n)
}

// `(t + d) - t == d` — adding then differencing recovers the duration.
test "instant_add_then_sub_roundtrips" {
    let base = baml.time.Instant.from_timestamp_seconds(1700000000n);
    let step = baml.time.Duration.from_minutes(90n);
    assert.equal(((base + step) - base).to_minutes(), 90n)
}

// ─── PlainDateTime arithmetic ────────────────────────────────────────────────

test "plain_datetime_add_duration" {
    let t = baml.time.PlainDateTime.from_components(2024, 3, 10, hour = 8)
        + baml.time.Duration.from_hours(3n);
    assert.equal(t.hour(), 11)
}

test "plain_datetime_sub_duration" {
    let t = baml.time.PlainDateTime.from_components(2024, 3, 10, hour = 8)
        - baml.time.Duration.from_hours(3n);
    assert.equal(t.hour(), 5)
}

// Unlike `PlainTime`, a `PlainDateTime` carries a date, so crossing midnight
// rolls the day rather than wrapping.
test "plain_datetime_add_crosses_into_the_next_day" {
    let t = baml.time.PlainDateTime.from_components(2024, 3, 10, hour = 23)
        + baml.time.Duration.from_hours(2n);
    assert.equal(t.day(), 11);
    assert.equal(t.hour(), 1)
}

test "plain_datetime_sub_plain_datetime_is_a_duration" {
    let gap = baml.time.PlainDateTime.from_components(2024, 3, 10, hour = 12)
        - baml.time.PlainDateTime.from_components(2024, 3, 10, hour = 9);
    assert.equal(gap.to_hours(), 3n)
}

// ─── PlainTime arithmetic (wraps at midnight) ────────────────────────────────

test "plain_time_add_duration" {
    let t = baml.time.PlainTime.from_components(7, minute = 30)
        + baml.time.Duration.from_minutes(45n);
    assert.equal(string.from(t), "08:15:00")
}

// No date to carry into, so 23:00 + 2h is 01:00.
test "plain_time_add_wraps_past_midnight" {
    let t = baml.time.PlainTime.from_components(23) + baml.time.Duration.from_hours(2n);
    assert.equal(string.from(t), "01:00:00")
}

// Wrapping is modular, not saturating: a multi-day duration reduces mod 24h.
test "plain_time_add_reduces_durations_longer_than_a_day" {
    let t = baml.time.PlainTime.from_components(6)
        + baml.time.Duration.from_hours(50n);
    assert.equal(string.from(t), "08:00:00")
}

test "plain_time_sub_wraps_back_past_midnight" {
    let t = baml.time.PlainTime.from_components(1) - baml.time.Duration.from_hours(2n);
    assert.equal(string.from(t), "23:00:00")
}

// The `((n % day) + day) % day` wrap must land on 00:00:00, not a negative
// value: subtracting exactly the current time-of-day is the boundary case.
test "plain_time_sub_to_exact_midnight" {
    let t = baml.time.PlainTime.from_components(5) - baml.time.Duration.from_hours(5n);
    assert.equal(string.from(t), "00:00:00")
}

test "plain_time_sub_plain_time_is_a_duration" {
    let gap = baml.time.PlainTime.from_components(9, minute = 30)
        - baml.time.PlainTime.from_components(7);
    assert.equal(gap.to_minutes(), 150n)
}

// The difference of two wall-clock times is signed and does NOT wrap — this is
// the one `PlainTime` operator that can produce a negative result.
test "plain_time_sub_plain_time_is_signed" {
    let gap = baml.time.PlainTime.from_components(1)
        - baml.time.PlainTime.from_components(23);
    assert.equal(gap.to_hours(), -22n)
}

// ─── ZonedDateTime arithmetic ────────────────────────────────────────────────

test "zoned_add_duration" {
    let z = baml.time.ZonedDateTime.from_components("UTC", 2024, 3, 10, hour = 8)
        + baml.time.Duration.from_hours(3n);
    assert.equal(z.hour(), 11)
}

test "zoned_sub_duration" {
    let z = baml.time.ZonedDateTime.from_components("UTC", 2024, 3, 10, hour = 8)
        - baml.time.Duration.from_hours(3n);
    assert.equal(z.hour(), 5)
}

// Arithmetic moves the absolute instant and leaves the timezone alone.
test "zoned_arithmetic_preserves_the_timezone" {
    let base = baml.time.ZonedDateTime.from_components("America/New_York", 2024, 3, 10, hour = 8);
    let later = base + baml.time.Duration.from_hours(1n);
    assert.equal(later.timezone(), "America/New_York");
    assert.equal((later - base).to_hours(), 1n)
}

test "zoned_sub_zoned_is_a_duration" {
    let gap = baml.time.ZonedDateTime.from_components("UTC", 2024, 3, 10, hour = 12)
        - baml.time.ZonedDateTime.from_components("UTC", 2024, 3, 10, hour = 9);
    assert.equal(gap.to_hours(), 3n)
}

// The gap is absolute, so two zoned times naming the same instant in different
// timezones differ by zero.
test "zoned_sub_across_timezones_measures_absolute_time" {
    let utc = baml.time.ZonedDateTime.from_components("UTC", 2024, 3, 10, hour = 12);
    let same = utc.with_timezone("Asia/Tokyo");
    assert.equal((utc - same).to_nanoseconds(), 0n)
}

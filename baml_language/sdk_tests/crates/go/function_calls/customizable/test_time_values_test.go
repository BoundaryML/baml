package sdk_test

import (
	"context"
	"math/big"
	"reflect"
	"strings"
	"testing"

	"baml.local/sdk/baml_sdk"
	"baml.local/sdk/baml_sdk/baml"
)

var (
	_ func(context.Context, baml.TimeInstant) (baml.TimeInstant, error)                              = baml_sdk.GoTimeTestsRoundTripInstant
	_ func(context.Context, baml.TimeDuration) (baml.TimeDuration, error)                            = baml_sdk.GoTimeTestsRoundTripDuration
	_ func(context.Context, baml.TimePlainDate) (baml.TimePlainDate, error)                          = baml_sdk.GoTimeTestsRoundTripPlainDate
	_ func(context.Context, baml.TimePlainTime) (baml.TimePlainTime, error)                          = baml_sdk.GoTimeTestsRoundTripPlainTime
	_ func(context.Context, baml.TimePlainDateTime) (baml.TimePlainDateTime, error)                  = baml_sdk.GoTimeTestsRoundTripPlainDateTime
	_ func(context.Context, baml.TimeTimeZoneOffset) (baml.TimeTimeZoneOffset, error)                = baml_sdk.GoTimeTestsRoundTripTimeZoneOffset
	_ func(context.Context, baml.TimeZonedDateTime) (baml.TimeZonedDateTime, error)                  = baml_sdk.GoTimeTestsRoundTripZonedDateTime
	_ func(context.Context, *baml.TimeInstant) (*baml.TimeInstant, error)                            = baml_sdk.GoTimeTestsRoundTripOptionalInstant
	_ func(context.Context, ...baml_sdk.GoTimeTestsDefaultDurationOption) (baml.TimeDuration, error) = baml_sdk.GoTimeTestsDefaultDuration
)

func mustBigInt(t *testing.T, value string) *big.Int {
	t.Helper()
	result, ok := new(big.Int).SetString(value, 10)
	if !ok {
		t.Fatalf("invalid test bigint %q", value)
	}
	return result
}

func bigIntEqual(left, right *big.Int) bool {
	if left == nil || right == nil {
		return left == nil && right == nil
	}
	return left.Cmp(right) == 0
}

func stringPointer(value string) *string                                        { return &value }
func timeInt64Pointer(value int64) *int64                                       { return &value }
func instantPointer(value baml.TimeInstant) *baml.TimeInstant                   { return &value }
func durationPointer(value baml.TimeDuration) *baml.TimeDuration                { return &value }
func plainTimePointer(value baml.TimePlainTime) *baml.TimePlainTime             { return &value }
func zonedDateTimePointer(value baml.TimeZonedDateTime) *baml.TimeZonedDateTime { return &value }

func Test_baml_time_class_and_field_wire_names_are_exact(t *testing.T) {
	classes := []struct {
		value interface{ BAMLClassName() string }
		name  string
	}{
		{baml.TimeInstant{}, "baml.time.Instant"},
		{baml.TimeDuration{}, "baml.time.Duration"},
		{baml.TimePlainDate{}, "baml.time.PlainDate"},
		{baml.TimePlainTime{}, "baml.time.PlainTime"},
		{baml.TimePlainDateTime{}, "baml.time.PlainDateTime"},
		{baml.TimeTimeZoneOffset{}, "baml.time.TimeZoneOffset"},
		{baml.TimeZonedDateTime{}, "baml.time.ZonedDateTime"},
	}
	for _, test := range classes {
		if got := test.value.BAMLClassName(); got != test.name {
			t.Fatalf("class name = %q, want %q", got, test.name)
		}
	}

	assertTimeWireFields(t, reflect.TypeOf(baml.TimeInstant{}), map[string]string{
		"Nanoseconds": "_nanoseconds",
	})
	assertTimeWireFields(t, reflect.TypeOf(baml.TimeDuration{}), map[string]string{
		"Nanoseconds": "_nanoseconds",
	})
	assertTimeWireFields(t, reflect.TypeOf(baml.TimePlainDate{}), map[string]string{
		"Days": "_days",
	})
	assertTimeWireFields(t, reflect.TypeOf(baml.TimePlainTime{}), map[string]string{
		"Nanoseconds": "_nanoseconds",
	})
	assertTimeWireFields(t, reflect.TypeOf(baml.TimePlainDateTime{}), map[string]string{
		"Nanoseconds": "_nanoseconds",
	})
	assertTimeWireFields(t, reflect.TypeOf(baml.TimeTimeZoneOffset{}), map[string]string{
		"Nanoseconds": "_nanoseconds",
	})
	assertTimeWireFields(t, reflect.TypeOf(baml.TimeZonedDateTime{}), map[string]string{
		"Nanoseconds": "_nanoseconds",
		"OffsetNs":    "_offset_ns",
		"Iana":        "_iana",
	})
}

func assertTimeWireFields(t *testing.T, typ reflect.Type, fields map[string]string) {
	t.Helper()
	if typ.NumField() != len(fields) {
		t.Fatalf("%s has %d fields, want %d", typ, typ.NumField(), len(fields))
	}
	for goName, wireName := range fields {
		field, ok := typ.FieldByName(goName)
		if !ok {
			t.Fatalf("%s has no Go field %q", typ, goName)
		}
		if got := field.Tag.Get("baml"); got != wireName {
			t.Fatalf("%s.%s baml tag = %q, want %q", typ, goName, got, wireName)
		}
		if got := field.Tag.Get("json"); got != wireName {
			t.Fatalf("%s.%s json tag = %q, want %q", typ, goName, got, wireName)
		}
	}
}

func Test_baml_time_constructors_and_parsers_return_lossless_internal_values(t *testing.T) {
	ctx := context.Background()
	large := mustBigInt(t, "1234567890123456789012345678901234567890")

	instant, err := baml_sdk.GoTimeTestsInstantFromTimestampNanoseconds(ctx, large)
	if err != nil || !bigIntEqual(instant.Nanoseconds, large) {
		t.Fatalf("instant constructor = %#v, %v", instant, err)
	}
	parsedInstant, err := baml_sdk.GoTimeTestsParseInstant(ctx, "1970-01-01T00:00:00.000000001Z")
	if err != nil || parsedInstant.Nanoseconds.Cmp(big.NewInt(1)) != 0 {
		t.Fatalf("parsed instant = %#v, %v", parsedInstant, err)
	}

	duration, err := baml_sdk.GoTimeTestsDurationFromNanoseconds(ctx, new(big.Int).Neg(large))
	if err != nil || duration.Nanoseconds.Cmp(new(big.Int).Neg(large)) != 0 {
		t.Fatalf("duration constructor = %#v, %v", duration, err)
	}

	date, err := baml_sdk.GoTimeTestsPlainDateFromComponents(ctx, 1970, 1, 1)
	if err != nil || date.Days != 0 {
		t.Fatalf("plain date constructor = %#v, %v", date, err)
	}
	parsedDate, err := baml_sdk.GoTimeTestsParsePlainDate(ctx, "1969-12-31")
	if err != nil || parsedDate.Days != -1 {
		t.Fatalf("parsed plain date = %#v, %v", parsedDate, err)
	}

	plainTime, err := baml_sdk.GoTimeTestsPlainTimeFromComponents(ctx, 23, 59, 59, 999, 999, 999)
	if err != nil || plainTime.Nanoseconds != 86_399_999_999_999 {
		t.Fatalf("plain time constructor = %#v, %v", plainTime, err)
	}
	parsedTime, err := baml_sdk.GoTimeTestsParsePlainTime(ctx, "00:00:00.000000001")
	if err != nil || parsedTime.Nanoseconds != 1 {
		t.Fatalf("parsed plain time = %#v, %v", parsedTime, err)
	}

	plainDateTime, err := baml_sdk.GoTimeTestsPlainDateTimeFromComponents(ctx, 1970, 1, 1, 0, 0, 0, 0, 0, 1)
	if err != nil || plainDateTime.Nanoseconds.Cmp(big.NewInt(1)) != 0 {
		t.Fatalf("plain datetime constructor = %#v, %v", plainDateTime, err)
	}
	parsedDateTime, err := baml_sdk.GoTimeTestsParsePlainDateTime(ctx, "1969-12-31T23:59:59.999999999")
	if err != nil || parsedDateTime.Nanoseconds.Cmp(big.NewInt(-1)) != 0 {
		t.Fatalf("parsed plain datetime = %#v, %v", parsedDateTime, err)
	}

	positiveOffset, err := baml_sdk.GoTimeTestsTimeZoneOffsetFromComponents(ctx, 5, 30)
	if err != nil || positiveOffset.Nanoseconds != 19_800_000_000_000 {
		t.Fatalf("positive offset = %#v, %v", positiveOffset, err)
	}
	negativeOffset, err := baml_sdk.GoTimeTestsTimeZoneOffsetFromComponents(ctx, -9, -30)
	if err != nil || negativeOffset.Nanoseconds != -34_200_000_000_000 {
		t.Fatalf("negative offset = %#v, %v", negativeOffset, err)
	}

	epoch := baml.TimeInstant{Nanoseconds: big.NewInt(0)}
	fixed, err := baml_sdk.GoTimeTestsZonedDateTimeFromFixedOffset(ctx, epoch, positiveOffset)
	if err != nil || fixed.Nanoseconds.Sign() != 0 || fixed.OffsetNs == nil || *fixed.OffsetNs != positiveOffset.Nanoseconds || fixed.Iana != nil {
		t.Fatalf("fixed zoned datetime = %#v, %v", fixed, err)
	}
	iana, err := baml_sdk.GoTimeTestsZonedDateTimeFromIana(ctx, epoch, "America/Vancouver")
	if err != nil || iana.Nanoseconds.Sign() != 0 || iana.OffsetNs != nil || iana.Iana == nil || *iana.Iana != "America/Vancouver" {
		t.Fatalf("IANA zoned datetime = %#v, %v", iana, err)
	}
	parsedFixed, err := baml_sdk.GoTimeTestsParseZonedDateTime(ctx, "1970-01-01T01:00:00+01:00")
	if err != nil || parsedFixed.Nanoseconds.Sign() != 0 || parsedFixed.OffsetNs == nil || *parsedFixed.OffsetNs != 3_600_000_000_000 || parsedFixed.Iana != nil {
		t.Fatalf("parsed fixed zoned datetime = %#v, %v", parsedFixed, err)
	}
	parsedIANA, err := baml_sdk.GoTimeTestsParseZonedDateTime(ctx, "1970-01-01T00:00:00Z[UTC]")
	if err != nil || parsedIANA.Nanoseconds.Sign() != 0 || parsedIANA.OffsetNs != nil || parsedIANA.Iana == nil || *parsedIANA.Iana != "UTC" {
		t.Fatalf("parsed IANA zoned datetime = %#v, %v", parsedIANA, err)
	}
}

func Test_baml_time_go_constructed_values_round_trip_at_numeric_boundaries(t *testing.T) {
	ctx := context.Background()
	positive := mustBigInt(t, "99999999999999999999999999999999999999999999999999")
	negative := new(big.Int).Neg(positive)

	instant := baml.TimeInstant{Nanoseconds: positive}
	if got, err := baml_sdk.GoTimeTestsRoundTripInstant(ctx, instant); err != nil || !bigIntEqual(got.Nanoseconds, positive) {
		t.Fatalf("instant round trip = %#v, %v", got, err)
	}
	duration := baml.TimeDuration{Nanoseconds: negative}
	if got, err := baml_sdk.GoTimeTestsRoundTripDuration(ctx, duration); err != nil || !bigIntEqual(got.Nanoseconds, negative) {
		t.Fatalf("duration round trip = %#v, %v", got, err)
	}
	// BAML ints deliberately use the signed 62-bit immediate range even though
	// the host projection is int64.
	for _, days := range []int64{-4_611_686_018_427_387_904, -1, 0, 4_611_686_018_427_387_903} {
		got, err := baml_sdk.GoTimeTestsRoundTripPlainDate(ctx, baml.TimePlainDate{Days: days})
		if err != nil || got.Days != days {
			t.Fatalf("plain date %d round trip = %#v, %v", days, got, err)
		}
	}
	for _, nanoseconds := range []int64{0, 86_399_999_999_999} {
		got, err := baml_sdk.GoTimeTestsRoundTripPlainTime(ctx, baml.TimePlainTime{Nanoseconds: nanoseconds})
		if err != nil || got.Nanoseconds != nanoseconds {
			t.Fatalf("plain time %d round trip = %#v, %v", nanoseconds, got, err)
		}
	}
	plainDateTime := baml.TimePlainDateTime{Nanoseconds: negative}
	if got, err := baml_sdk.GoTimeTestsRoundTripPlainDateTime(ctx, plainDateTime); err != nil || !bigIntEqual(got.Nanoseconds, negative) {
		t.Fatalf("plain datetime round trip = %#v, %v", got, err)
	}
	for _, nanoseconds := range []int64{-86_400_000_000_000, 0, 86_400_000_000_000} {
		got, err := baml_sdk.GoTimeTestsRoundTripTimeZoneOffset(ctx, baml.TimeTimeZoneOffset{Nanoseconds: nanoseconds})
		if err != nil || got.Nanoseconds != nanoseconds {
			t.Fatalf("timezone offset %d round trip = %#v, %v", nanoseconds, got, err)
		}
	}

	fixedOffset := int64(-86_400_000_000_000)
	fixed := baml.TimeZonedDateTime{Nanoseconds: positive, OffsetNs: &fixedOffset}
	gotFixed, err := baml_sdk.GoTimeTestsRoundTripZonedDateTime(ctx, fixed)
	if err != nil || !bigIntEqual(gotFixed.Nanoseconds, positive) || gotFixed.OffsetNs == nil || *gotFixed.OffsetNs != fixedOffset || gotFixed.Iana != nil {
		t.Fatalf("fixed zoned datetime round trip = %#v, %v", gotFixed, err)
	}
	ianaName := "Etc/UTC"
	iana := baml.TimeZonedDateTime{Nanoseconds: negative, Iana: &ianaName}
	gotIANA, err := baml_sdk.GoTimeTestsRoundTripZonedDateTime(ctx, iana)
	if err != nil || !bigIntEqual(gotIANA.Nanoseconds, negative) || gotIANA.OffsetNs != nil || gotIANA.Iana == nil || *gotIANA.Iana != ianaName {
		t.Fatalf("IANA zoned datetime round trip = %#v, %v", gotIANA, err)
	}
}

func Test_baml_time_nested_containers_and_baml_field_inspection(t *testing.T) {
	ctx := context.Background()
	instant := baml.TimeInstant{Nanoseconds: mustBigInt(t, "123456789012345678901")}
	duration := baml.TimeDuration{Nanoseconds: mustBigInt(t, "-987654321098765432109")}
	date := baml.TimePlainDate{Days: -1}
	plainTime := baml.TimePlainTime{Nanoseconds: 86_399_999_999_999}
	plainDateTime := baml.TimePlainDateTime{Nanoseconds: big.NewInt(-1)}
	offset := baml.TimeTimeZoneOffset{Nanoseconds: -28_800_000_000_000}
	zoned := baml.TimeZonedDateTime{Nanoseconds: big.NewInt(42), OffsetNs: timeInt64Pointer(offset.Nanoseconds)}
	iana := baml.TimeZonedDateTime{Nanoseconds: big.NewInt(-42), Iana: stringPointer("America/Vancouver")}

	want := baml_sdk.GoTimeTestsTimeEnvelope{
		Instant:                   instant,
		Duration:                  duration,
		PlainDate:                 date,
		PlainTime:                 plainTime,
		PlainDateTime:             plainDateTime,
		TimeZoneOffset:            offset,
		ZonedDateTime:             zoned,
		OptionalInstant:           instantPointer(instant),
		InstantList:               []baml.TimeInstant{{Nanoseconds: big.NewInt(0)}, instant},
		OptionalDurationList:      []*baml.TimeDuration{nil, durationPointer(duration)},
		PlainDateMap:              map[string]baml.TimePlainDate{"before_epoch": date, "epoch": {Days: 0}},
		OptionalPlainTimeMap:      map[string]*baml.TimePlainTime{"missing": nil, "last_ns": plainTimePointer(plainTime)},
		PlainDateTimeList:         []baml.TimePlainDateTime{{Nanoseconds: big.NewInt(0)}, plainDateTime},
		TimeZoneOffsetMap:         map[string]baml.TimeTimeZoneOffset{"utc": {Nanoseconds: 0}, "west": offset},
		OptionalZonedDateTimeList: []*baml.TimeZonedDateTime{zonedDateTimePointer(zoned), nil, zonedDateTimePointer(iana)},
	}

	got, err := baml_sdk.GoTimeTestsRoundTripTimeEnvelope(ctx, want)
	if err != nil || !timeEnvelopeEqual(got, want) {
		t.Fatalf("time envelope round trip = %#v, %v; want %#v", got, err, want)
	}
	fields, err := baml_sdk.GoTimeTestsInspectTimeEnvelope(ctx, want)
	if err != nil || !bigIntEqual(fields.InstantNs, instant.Nanoseconds) ||
		!bigIntEqual(fields.DurationNs, duration.Nanoseconds) || fields.PlainDateDays != date.Days ||
		fields.PlainTimeNs != plainTime.Nanoseconds || !bigIntEqual(fields.PlainDateTimeNs, plainDateTime.Nanoseconds) ||
		fields.TimeZoneOffsetNs != offset.Nanoseconds || !bigIntEqual(fields.ZonedDateTimeNs, zoned.Nanoseconds) ||
		fields.ZonedDateTimeOffsetNs == nil || *fields.ZonedDateTimeOffsetNs != offset.Nanoseconds || fields.ZonedDateTimeIana != nil {
		t.Fatalf("BAML field inspection = %#v, %v", fields, err)
	}

	want.ZonedDateTime = iana
	fields, err = baml_sdk.GoTimeTestsInspectTimeEnvelope(ctx, want)
	if err != nil || fields.ZonedDateTimeOffsetNs != nil || fields.ZonedDateTimeIana == nil || *fields.ZonedDateTimeIana != "America/Vancouver" {
		t.Fatalf("BAML IANA field inspection = %#v, %v", fields, err)
	}
}

func timeEnvelopeEqual(left, right baml_sdk.GoTimeTestsTimeEnvelope) bool {
	if !timeInstantEqual(left.Instant, right.Instant) || !timeDurationEqual(left.Duration, right.Duration) ||
		left.PlainDate != right.PlainDate || left.PlainTime != right.PlainTime ||
		!timePlainDateTimeEqual(left.PlainDateTime, right.PlainDateTime) || left.TimeZoneOffset != right.TimeZoneOffset ||
		!timeZonedDateTimeEqual(left.ZonedDateTime, right.ZonedDateTime) ||
		!optionalTimeInstantEqual(left.OptionalInstant, right.OptionalInstant) ||
		len(left.InstantList) != len(right.InstantList) || len(left.OptionalDurationList) != len(right.OptionalDurationList) ||
		len(left.PlainDateTimeList) != len(right.PlainDateTimeList) ||
		len(left.OptionalZonedDateTimeList) != len(right.OptionalZonedDateTimeList) ||
		!reflect.DeepEqual(left.PlainDateMap, right.PlainDateMap) ||
		!reflect.DeepEqual(left.OptionalPlainTimeMap, right.OptionalPlainTimeMap) ||
		!reflect.DeepEqual(left.TimeZoneOffsetMap, right.TimeZoneOffsetMap) {
		return false
	}
	for index := range left.InstantList {
		if !timeInstantEqual(left.InstantList[index], right.InstantList[index]) {
			return false
		}
	}
	for index := range left.OptionalDurationList {
		if !optionalTimeDurationEqual(left.OptionalDurationList[index], right.OptionalDurationList[index]) {
			return false
		}
	}
	for index := range left.PlainDateTimeList {
		if !timePlainDateTimeEqual(left.PlainDateTimeList[index], right.PlainDateTimeList[index]) {
			return false
		}
	}
	for index := range left.OptionalZonedDateTimeList {
		if !optionalTimeZonedDateTimeEqual(left.OptionalZonedDateTimeList[index], right.OptionalZonedDateTimeList[index]) {
			return false
		}
	}
	return true
}

func timeInstantEqual(left, right baml.TimeInstant) bool {
	return bigIntEqual(left.Nanoseconds, right.Nanoseconds)
}

func optionalTimeInstantEqual(left, right *baml.TimeInstant) bool {
	return (left == nil && right == nil) || (left != nil && right != nil && timeInstantEqual(*left, *right))
}

func timeDurationEqual(left, right baml.TimeDuration) bool {
	return bigIntEqual(left.Nanoseconds, right.Nanoseconds)
}

func optionalTimeDurationEqual(left, right *baml.TimeDuration) bool {
	return (left == nil && right == nil) || (left != nil && right != nil && timeDurationEqual(*left, *right))
}

func timePlainDateTimeEqual(left, right baml.TimePlainDateTime) bool {
	return bigIntEqual(left.Nanoseconds, right.Nanoseconds)
}

func timeZonedDateTimeEqual(left, right baml.TimeZonedDateTime) bool {
	return bigIntEqual(left.Nanoseconds, right.Nanoseconds) && reflect.DeepEqual(left.OffsetNs, right.OffsetNs) && reflect.DeepEqual(left.Iana, right.Iana)
}

func optionalTimeZonedDateTimeEqual(left, right *baml.TimeZonedDateTime) bool {
	return (left == nil && right == nil) || (left != nil && right != nil && timeZonedDateTimeEqual(*left, *right))
}

func Test_baml_time_nullable_and_defaulted_positions(t *testing.T) {
	ctx := context.Background()
	if got, err := baml_sdk.GoTimeTestsRoundTripOptionalInstant(ctx, nil); err != nil || got != nil {
		t.Fatalf("nil optional instant = %#v, %v", got, err)
	}
	instant := baml.TimeInstant{Nanoseconds: big.NewInt(7)}
	gotInstant, err := baml_sdk.GoTimeTestsRoundTripOptionalInstant(ctx, &instant)
	if err != nil || gotInstant == nil || gotInstant.Nanoseconds.Cmp(instant.Nanoseconds) != 0 {
		t.Fatalf("present optional instant = %#v, %v", gotInstant, err)
	}

	wantDefault := mustBigInt(t, "123456789012345678901234567890")
	gotDefault, err := baml_sdk.GoTimeTestsDefaultDuration(ctx)
	if err != nil || !bigIntEqual(gotDefault.Nanoseconds, wantDefault) {
		t.Fatalf("default duration = %#v, %v", gotDefault, err)
	}
	override := baml.TimeDuration{Nanoseconds: big.NewInt(-9)}
	gotOverride, err := baml_sdk.GoTimeTestsDefaultDuration(ctx, baml_sdk.WithGoTimeTestsDefaultDurationValue(override))
	if err != nil || !bigIntEqual(gotOverride.Nanoseconds, override.Nanoseconds) {
		t.Fatalf("overridden duration = %#v, %v", gotOverride, err)
	}

	if got, err := baml_sdk.GoTimeTestsDefaultZonedDateTime(ctx); err != nil || got != nil {
		t.Fatalf("default zoned datetime = %#v, %v", got, err)
	}
	zoned := baml.TimeZonedDateTime{Nanoseconds: big.NewInt(5), OffsetNs: timeInt64Pointer(0)}
	gotZoned, err := baml_sdk.GoTimeTestsDefaultZonedDateTime(ctx, baml_sdk.WithGoTimeTestsDefaultZonedDateTimeValue(&zoned))
	if err != nil || gotZoned == nil || !reflect.DeepEqual(*gotZoned, zoned) {
		t.Fatalf("overridden zoned datetime = %#v, %v", gotZoned, err)
	}
}

func Test_baml_time_raw_class_transport_does_not_enforce_semantic_invariants(t *testing.T) {
	ctx := context.Background()

	invalidPlainTime := baml.TimePlainTime{Nanoseconds: -1}
	if got, err := baml_sdk.GoTimeTestsRoundTripPlainTime(ctx, invalidPlainTime); err != nil || got != invalidPlainTime {
		t.Fatalf("semantically invalid plain time transport = %#v, %v", got, err)
	}

	invalidOffset := baml.TimeTimeZoneOffset{Nanoseconds: 86_400_000_000_001}
	if got, err := baml_sdk.GoTimeTestsRoundTripTimeZoneOffset(ctx, invalidOffset); err != nil || got != invalidOffset {
		t.Fatalf("semantically invalid timezone offset transport = %#v, %v", got, err)
	}

	both := baml.TimeZonedDateTime{
		Nanoseconds: big.NewInt(1),
		OffsetNs:    timeInt64Pointer(0),
		Iana:        stringPointer("UTC"),
	}
	if got, err := baml_sdk.GoTimeTestsRoundTripZonedDateTime(ctx, both); err != nil || !timeZonedDateTimeEqual(got, both) {
		t.Fatalf("zoned datetime with both zone fields transport = %#v, %v", got, err)
	}

	neither := baml.TimeZonedDateTime{Nanoseconds: big.NewInt(2)}
	if got, err := baml_sdk.GoTimeTestsRoundTripZonedDateTime(ctx, neither); err != nil || !timeZonedDateTimeEqual(got, neither) {
		t.Fatalf("zoned datetime with neither zone field transport = %#v, %v", got, err)
	}
}

func Test_baml_time_malformed_values_fail_at_the_earliest_typed_boundary(t *testing.T) {
	ctx := context.Background()
	for name, call := range map[string]func() error{
		"instant nil bigint": func() error {
			_, err := baml_sdk.GoTimeTestsRoundTripInstant(ctx, baml.TimeInstant{})
			return err
		},
		"duration nil bigint": func() error {
			_, err := baml_sdk.GoTimeTestsRoundTripDuration(ctx, baml.TimeDuration{})
			return err
		},
		"plain datetime nil bigint": func() error {
			_, err := baml_sdk.GoTimeTestsRoundTripPlainDateTime(ctx, baml.TimePlainDateTime{})
			return err
		},
		"zoned datetime nil bigint": func() error {
			_, err := baml_sdk.GoTimeTestsRoundTripZonedDateTime(ctx, baml.TimeZonedDateTime{OffsetNs: timeInt64Pointer(0)})
			return err
		},
		"int outside BAML range": func() error {
			_, err := baml_sdk.GoTimeTestsRoundTripPlainDate(ctx, baml.TimePlainDate{Days: 4_611_686_018_427_387_904})
			return err
		},
	} {
		t.Run(name, func(t *testing.T) {
			if err := call(); err == nil || (!strings.Contains(err.Error(), "uninitialized") && !strings.Contains(err.Error(), "outside the BAML integer range")) {
				t.Fatalf("malformed value error = %v", err)
			}
		})
	}

	for name, call := range map[string]func() error{
		"instant":        func() error { _, err := baml_sdk.GoTimeTestsParseInstant(ctx, "not-an-instant"); return err },
		"date":           func() error { _, err := baml_sdk.GoTimeTestsParsePlainDate(ctx, "2026-02-30"); return err },
		"time":           func() error { _, err := baml_sdk.GoTimeTestsParsePlainTime(ctx, "24:00:00"); return err },
		"plain datetime": func() error { _, err := baml_sdk.GoTimeTestsParsePlainDateTime(ctx, "missing-time"); return err },
		"zoned datetime": func() error { _, err := baml_sdk.GoTimeTestsParseZonedDateTime(ctx, "2026-01-01T00:00:00"); return err },
		"date components": func() error {
			_, err := baml_sdk.GoTimeTestsPlainDateFromComponents(ctx, 2026, 2, 30)
			return err
		},
		"time components": func() error {
			_, err := baml_sdk.GoTimeTestsPlainTimeFromComponents(ctx, 24, 0, 0, 0, 0, 0)
			return err
		},
		"timezone components": func() error {
			_, err := baml_sdk.GoTimeTestsTimeZoneOffsetFromComponents(ctx, 24, 1)
			return err
		},
	} {
		t.Run("parse "+name, func(t *testing.T) {
			if err := call(); err == nil {
				t.Fatal("malformed parser input unexpectedly succeeded")
			}
		})
	}
}

// Internal (white-box) tests for the Go host-callable bridge. These live in
// `package pkg` so they can exercise unexported helpers directly:
//
//   - coerceToType range/precision checks (F9)
//   - encode-error rollback of registered host-value keys (F5)
//
// The end-to-end behavior is covered by `pkg/tests/host_callable_test.go`;
// these tests pin the lower-level contracts without needing a BAML runtime.

package pkg

import (
	"math"
	"reflect"
	"testing"
)

// --- F9: coerceToType range/precision checks ---------------------------------

func TestCoerceToTypeIntNarrowing(t *testing.T) {
	cases := []struct {
		name   string
		src    int64
		target any // a value whose reflect.Type is the narrowing target
		ok     bool
	}{
		{"int8 in range", 44, int8(0), true},
		{"int8 max", 127, int8(0), true},
		{"int8 min", -128, int8(0), true},
		{"int8 overflow 300", 300, int8(0), false},
		{"int8 underflow -129", -129, int8(0), false},
		{"int16 in range", 30000, int16(0), true},
		{"int16 overflow", 40000, int16(0), false},
		{"int32 in range", 2_000_000_000, int32(0), true},
		{"int32 overflow", 3_000_000_000, int32(0), false},
		{"uint8 in range", 255, uint8(0), true},
		{"uint8 overflow", 256, uint8(0), false},
		{"uint8 negative", -1, uint8(0), false},
		{"uint16 in range", 65535, uint16(0), true},
		{"uint16 overflow", 65536, uint16(0), false},
		{"uint32 in range", 4_294_967_295, uint32(0), true},
		{"uint32 overflow", 4_294_967_296, uint32(0), false},
		{"uint64 negative", -1, uint64(0), false},
		{"uint64 large ok", math.MaxInt64, uint64(0), true},
		{"int64 always ok", math.MaxInt64, int64(0), true},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			target := reflect.TypeOf(tc.target)
			out, ok := coerceToType(tc.src, target)
			if ok != tc.ok {
				t.Fatalf("coerceToType(%d -> %s): ok=%v, want %v", tc.src, target, ok, tc.ok)
			}
			if ok {
				// On success the value must equal the source exactly.
				got := out.Convert(reflect.TypeOf(int64(0))).Int()
				if got != tc.src {
					t.Fatalf("coerceToType(%d -> %s): round-tripped to %d", tc.src, target, got)
				}
			}
		})
	}
}

func TestCoerceToTypeIntToFloat32(t *testing.T) {
	// Exactly representable integers round-trip; values beyond 2^24 do not.
	if _, ok := coerceToType(int64(16_777_216), reflect.TypeOf(float32(0))); !ok {
		t.Fatal("2^24 should be exactly representable as float32")
	}
	if _, ok := coerceToType(int64(16_777_217), reflect.TypeOf(float32(0))); ok {
		t.Fatal("2^24+1 is not exactly representable as float32; expected rejection")
	}
	// int64 -> float64 always accepted.
	if _, ok := coerceToType(int64(math.MaxInt64), reflect.TypeOf(float64(0))); !ok {
		t.Fatal("int64 -> float64 should always succeed")
	}
}

func TestCoerceToTypeFloat64ToFloat32(t *testing.T) {
	// In-range floats narrow (mantissa rounding is allowed).
	if _, ok := coerceToType(0.5, reflect.TypeOf(float32(0))); !ok {
		t.Fatal("0.5 should narrow to float32")
	}
	if _, ok := coerceToType(0.1, reflect.TypeOf(float32(0))); !ok {
		t.Fatal("0.1 should narrow to float32 (rounding allowed)")
	}
	// Overflow of float32 range is rejected.
	if _, ok := coerceToType(1e40, reflect.TypeOf(float32(0))); ok {
		t.Fatal("1e40 overflows float32 range; expected rejection")
	}
	// Non-finite values pass through.
	if _, ok := coerceToType(math.Inf(1), reflect.TypeOf(float32(0))); !ok {
		t.Fatal("+Inf should narrow to float32")
	}
	if _, ok := coerceToType(math.NaN(), reflect.TypeOf(float32(0))); !ok {
		t.Fatal("NaN should narrow to float32")
	}
}

// --- F5: encode-error rollback of registered host-value keys -----------------

// countRegistry returns the current number of entries in the host-value
// registry. White-box helper for the rollback test.
func countRegistry() int {
	hostValues.mu.Lock()
	defer hostValues.mu.Unlock()
	return len(hostValues.table)
}

// unencodable is a type goToInboundValue cannot encode, used to force an
// encode error after an earlier callable kwarg has been registered.
type unencodable struct{ _ int }

func TestEncodeErrorRollsBackRegisteredCallables(t *testing.T) {
	before := countRegistry()

	cb := func(x int64) string { return "" }
	// Map iteration order is randomized, so a single map with one good +
	// one bad kwarg could encode the bad one first and never register the
	// callable. To deterministically exercise the rollback path, drive the
	// internal encoder directly: register a callable, then fail.
	var registered []uint64
	entry, err := goToInboundValueTracking(cb, &registered)
	if err != nil {
		t.Fatalf("encoding a callable should succeed: %v", err)
	}
	if entry.GetHandle() == nil {
		t.Fatal("callable should encode to a handle")
	}
	if len(registered) != 1 {
		t.Fatalf("expected 1 registered key, got %d", len(registered))
	}
	if countRegistry() != before+1 {
		t.Fatalf("registry should have grown by 1, before=%d now=%d", before, countRegistry())
	}

	// Now a sibling kwarg fails to encode. Roll back.
	if _, err := goToInboundValueTracking(unencodable{}, &registered); err == nil {
		t.Fatal("expected unencodable type to fail")
	}
	rollbackRegisteredHostValues(registered)

	if countRegistry() != before {
		t.Fatalf("rollback should have removed the registered callable: before=%d now=%d", before, countRegistry())
	}
}

// encodeCallArgs must roll back on a later-kwarg failure end-to-end.
func TestEncodeCallArgsRollsBackOnLaterFailure(t *testing.T) {
	before := countRegistry()
	cb := func(x int64) string { return "" }
	_, err := encodeCallArgs(map[string]any{
		"callback": cb,
		"bad":      unencodable{},
	})
	if err == nil {
		t.Fatal("expected encodeCallArgs to fail on the unencodable kwarg")
	}
	if countRegistry() != before {
		t.Fatalf("encodeCallArgs should have rolled back the callable: before=%d now=%d", before, countRegistry())
	}
}

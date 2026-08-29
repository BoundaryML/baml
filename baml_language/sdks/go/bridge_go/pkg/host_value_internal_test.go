// Internal (white-box) tests for the Go host-callable bridge. These live in
// `package pkg` so they can exercise unexported helpers directly:
//
//   - coerceToType range/precision checks
//   - encode-error rollback of registered host-value keys
//
// The end-to-end behavior is covered by `pkg/tests/host_callable_test.go`;
// these tests pin the lower-level contracts without needing a BAML runtime.

package pkg

import (
	"math"
	"reflect"
	"testing"

	pb "bridge_go/cffi/proto/baml_bridge/cffi/v1"

	"google.golang.org/protobuf/proto"
)

// --- coerceToType range/precision checks -------------------------------------

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

// --- encode-error rollback of registered host-value keys ---------------------

// countRegistry returns the current number of entries in the host-value
// registry. White-box helper for the rollback test.
func countRegistry() int {
	hostValues.mu.Lock()
	defer hostValues.mu.Unlock()
	return len(hostValues.table)
}

// unencodable is a type goToInboundValueTracking cannot encode, used to force an
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

// --- coerceToType must reject lossy conversions, not silently corrupt --------

func TestCoerceToTypeRejectsFloatToInt(t *testing.T) {
	// A float argument into an integer parameter must be rejected, not
	// truncated (300.0 -> int8 44, 3.9 -> 3, NaN -> 0, +Inf -> MaxInt64).
	for _, target := range []any{int(0), int8(0), int32(0), int64(0), uint32(0)} {
		tt := reflect.TypeOf(target)
		for _, src := range []float64{3.5, 3.0, 300.0, math.NaN(), math.Inf(1)} {
			if _, ok := coerceToType(src, tt); ok {
				t.Fatalf("coerceToType(%v -> %s) must be rejected, not truncated", src, tt)
			}
		}
	}
	// float -> float still works.
	if _, ok := coerceToType(3.5, reflect.TypeOf(float64(0))); !ok {
		t.Fatal("float64 -> float64 should succeed")
	}
}

func TestCoerceToTypeRejectsIntToString(t *testing.T) {
	// int64 -> string must be rejected, not converted to a Unicode rune ("A").
	if _, ok := coerceToType(int64(65), reflect.TypeOf("")); ok {
		t.Fatal("coerceToType(int64 -> string) must be rejected (no rune conversion)")
	}
	if _, ok := coerceToType("hi", reflect.TypeOf("")); !ok {
		t.Fatal("string -> string should succeed")
	}
}

// --- goToInboundValueTracking must encode the natural narrow numeric return types ----

func TestGoToInboundValueEncodesNarrowNumerics(t *testing.T) {
	for _, v := range []any{int8(5), int16(5), int32(5), uint8(5), uint16(5), uint32(5), uint(5)} {
		iv, err := goToInboundValueTracking(v, nil)
		if err != nil {
			t.Fatalf("encoding %T should succeed: %v", v, err)
		}
		if iv.GetIntValue() != 5 {
			t.Fatalf("encoding %T: got int_value %d, want 5", v, iv.GetIntValue())
		}
	}
	iv, err := goToInboundValueTracking(float32(1.5), nil)
	if err != nil {
		t.Fatalf("encoding float32 should succeed: %v", err)
	}
	if iv.GetFloatValue() != 1.5 {
		t.Fatalf("encoding float32: got float_value %v, want 1.5", iv.GetFloatValue())
	}
	// A uint beyond int64 range cannot fit the proto int64 → error, not wrap.
	if _, err := goToInboundValueTracking(uint64(math.MaxUint64), nil); err == nil {
		t.Fatal("encoding uint64 > MaxInt64 should error")
	}
}

// --- isNilValue detects the typed-nil-error trap -----------------------------

type tnErr struct{}

func (*tnErr) Error() string { return "boom" }

func TestIsNilValueDetectsTypedNil(t *testing.T) {
	// A genuine nil `error` interface.
	var nilErr error
	if !isNilValue(reflect.ValueOf(&nilErr).Elem()) {
		t.Fatal("a nil error interface must be detected as nil")
	}
	// A direct nil pointer.
	var nilPtr *tnErr
	if !isNilValue(reflect.ValueOf(nilPtr)) {
		t.Fatal("a nil pointer must be detected as nil")
	}
	// A typed-nil pointer boxed in an `error` interface (the trap): the
	// interface is non-nil but the boxed pointer is nil, so calling `.Error()`
	// would panic. `isNilValue` must see through to the nil pointer.
	errType := reflect.TypeOf((*error)(nil)).Elem()
	boxed := reflect.New(errType).Elem()
	boxed.Set(reflect.ValueOf(nilPtr))
	if boxed.IsNil() {
		t.Fatal("sanity: a typed-nil boxed in an interface is a non-nil interface")
	}
	if !isNilValue(boxed) {
		t.Fatal("a typed-nil pointer boxed in an interface must be detected as nil")
	}
	// A real, non-nil error must NOT be detected as nil.
	boxed.Set(reflect.ValueOf(&tnErr{}))
	if isNilValue(boxed) {
		t.Fatal("a non-nil error must not be detected as nil")
	}
}

// encodeCallArgs must roll back on a later-kwarg failure end-to-end.
func TestEncodeCallArgsRollsBackOnLaterFailure(t *testing.T) {
	before := countRegistry()
	cb := func(x int64) string { return "" }
	_, err := encodeCallArgs(map[string]any{
		"callback": cb,
		"bad":      unencodable{},
	}, "TestFunction", 1)
	if err == nil {
		t.Fatal("expected encodeCallArgs to fail on the unencodable kwarg")
	}
	if countRegistry() != before {
		t.Fatalf("encodeCallArgs should have rolled back the callable: before=%d now=%d", before, countRegistry())
	}
}

func TestEncodeCallArgsAlwaysSetsFunctionTarget(t *testing.T) {
	encoded, err := encodeCallArgs(map[string]any{}, "TestFunction", 7)
	if err != nil {
		t.Fatalf("encodeCallArgs failed: %v", err)
	}
	var call pb.CallFunctionArgs
	if err := proto.Unmarshal(encoded, &call); err != nil {
		t.Fatalf("decoding CallFunctionArgs failed: %v", err)
	}
	if got := call.GetFunctionName(); got != "TestFunction" {
		t.Fatalf("function target = %q, want TestFunction", got)
	}
	if got := call.GetCallId(); got != 7 {
		t.Fatalf("call ID = %d, want 7", got)
	}
}

func TestPortableMediaAndPromptRoundTripWithoutHandles(t *testing.T) {
	media := &pb.BamlValueMedia{
		Media: pb.MediaTypeEnum_IMAGE,
		Value: &pb.BamlValueMedia_Url{Url: "https://example.com/cat.png"},
	}
	prompt := &pb.BamlValuePromptAst{
		Value: &pb.BamlValuePromptAst_Simple{
			Simple: &pb.BamlValuePromptAstSimple{
				Value: &pb.BamlValuePromptAstSimple_String_{String_: "hello"},
			},
		},
	}

	for name, outbound := range map[string]*pb.BamlOutboundValue{
		"media":  {Value: &pb.BamlOutboundValue_MediaValue{MediaValue: media}},
		"prompt": {Value: &pb.BamlOutboundValue_PromptAstValue{PromptAstValue: prompt}},
	} {
		t.Run(name, func(t *testing.T) {
			decoded, err := outboundToGo(outbound)
			if err != nil {
				t.Fatal(err)
			}
			inbound, err := goToInboundValueTracking(decoded, nil)
			if err != nil {
				t.Fatal(err)
			}
			if _, ok := inbound.Value.(*pb.InboundValue_Handle); ok {
				t.Fatal("portable payload was converted to a handle")
			}
			switch name {
			case "media":
				if !proto.Equal(inbound.GetMediaValue(), media) {
					t.Fatalf("media payload changed: got %v", inbound.GetMediaValue())
				}
			case "prompt":
				if !proto.Equal(inbound.GetPromptAstValue(), prompt) {
					t.Fatalf("prompt payload changed: got %v", inbound.GetPromptAstValue())
				}
			}
		})
	}
}

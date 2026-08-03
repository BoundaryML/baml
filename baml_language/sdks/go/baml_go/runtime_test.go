package baml_go

import (
	"context"
	"errors"
	"math"
	"strings"
	"testing"

	"github.com/boundaryml/baml-go/internal/cffi"
	"google.golang.org/protobuf/proto"
)

func outboundResultBytes(t *testing.T, result *cffi.BamlOutboundResult) []byte {
	t.Helper()
	payload, err := proto.Marshal(result)
	if err != nil {
		t.Fatal(err)
	}
	return payload
}

func outboundClass(name string, fields ...*cffi.BamlOutboundMapEntry) *cffi.BamlOutboundValue {
	return &cffi.BamlOutboundValue{Value: &cffi.BamlOutboundValue_ClassValue{ClassValue: &cffi.BamlValueClass{
		Name: name, Fields: fields,
	}}}
}

func outboundString(value string) *cffi.BamlOutboundValue {
	return &cffi.BamlOutboundValue{Value: &cffi.BamlOutboundValue_StringValue{StringValue: value}}
}

func TestDecodeResultRejectsMalformedAndEmptyEnvelopes(t *testing.T) {
	for name, payload := range map[string][]byte{
		"malformed protobuf": {0xff},
		"empty envelope":     outboundResultBytes(t, &cffi.BamlOutboundResult{}),
	} {
		t.Run(name, func(t *testing.T) {
			if _, err := decodeResult(payload); err == nil || err.Error() == "" {
				t.Fatalf("decodeResult returned error %v", err)
			}
		})
	}
	if _, err := decodeResultEnvelope(nil); err == nil || !strings.Contains(err.Error(), "nil result envelope") {
		t.Fatalf("nil envelope error = %v", err)
	}
}

func TestDecodeResultRejectsMissingArmPayloads(t *testing.T) {
	for name, result := range map[string]*cffi.BamlOutboundResult{
		"success": {Result: &cffi.BamlOutboundResult_Ok{}},
		"error":   {Result: &cffi.BamlOutboundResult_Error{}},
		"panic":   {Result: &cffi.BamlOutboundResult_Panic{}},
	} {
		t.Run(name, func(t *testing.T) {
			if _, err := decodeResultEnvelope(result); err == nil || err.Error() == "" {
				t.Fatalf("decodeResultEnvelope returned error %v", err)
			}
		})
	}
}

func TestDecodeResultFormatsErrorIdentityMessageAndTrace(t *testing.T) {
	trace := []string{
		`File "outer.baml", line 3, in user.errors.Outer`,
		`File "middle.baml", line 7, in user.errors.Middle`,
		`File "types.baml", line 12, in user.errors.Throw`,
	}
	value := outboundClass("user.errors.MyError", &cffi.BamlOutboundMapEntry{
		Key: "message", Value: outboundString("broken input"),
	})
	result := &cffi.BamlOutboundResult{Result: &cffi.BamlOutboundResult_Error{Error: &cffi.BamlOutboundError{
		Value: value,
		Trace: trace,
	}}}
	_, err := decodeResult(outboundResultBytes(t, result))
	if err == nil {
		t.Fatal("error result decoded successfully")
	}
	want := "BAML error: user.errors.MyError: broken input\n    " + strings.Join(trace, "\n    ")
	if err.Error() != want {
		t.Fatalf("formatted error = %q, want %q", err, want)
	}
	for _, frame := range trace {
		if count := strings.Count(err.Error(), frame); count != 1 {
			t.Fatalf("trace frame %q appears %d times in %q", frame, count, err)
		}
	}
}

func TestDecodeResultUnwrapsUnionThrownClassIdentity(t *testing.T) {
	value := &cffi.BamlOutboundValue{Value: &cffi.BamlOutboundValue_UnionVariantValue{UnionVariantValue: &cffi.BamlValueUnionVariant{
		Value: outboundClass("user.errors.ParseError", &cffi.BamlOutboundMapEntry{
			Key: "message", Value: outboundString("bad parse"),
		}),
	}}}
	result := &cffi.BamlOutboundResult{Result: &cffi.BamlOutboundResult_Error{Error: &cffi.BamlOutboundError{Value: value}}}
	_, err := decodeResult(outboundResultBytes(t, result))
	if err == nil || !strings.Contains(err.Error(), "user.errors.ParseError: bad parse") {
		t.Fatalf("union thrown error = %v", err)
	}
}

func TestDecodeResultTraceLessFailuresRemainDescriptive(t *testing.T) {
	for name, result := range map[string]*cffi.BamlOutboundResult{
		"error": {Result: &cffi.BamlOutboundResult_Error{Error: &cffi.BamlOutboundError{}}},
		"panic": {Result: &cffi.BamlOutboundResult_Panic{Panic: &cffi.BamlOutboundPanic{}}},
	} {
		t.Run(name, func(t *testing.T) {
			_, err := decodeResult(outboundResultBytes(t, result))
			if err == nil || !strings.Contains(err.Error(), "BAML "+name) || strings.Contains(err.Error(), "\n") {
				t.Fatalf("trace-less %s error = %q", name, err)
			}
		})
	}
}

func TestDecodeResultNonExitPanicReturnsError(t *testing.T) {
	trace := []string{
		`File "outer.baml", line 2, in user.panics.Outer`,
		`File "middle.baml", line 5, in user.panics.Middle`,
		`File "types.baml", line 9, in user.panics.Boom`,
	}
	result := &cffi.BamlOutboundResult{Result: &cffi.BamlOutboundResult_Panic{Panic: &cffi.BamlOutboundPanic{
		Value: outboundClass("baml.panics.UserPanic", &cffi.BamlOutboundMapEntry{
			Key: "message", Value: outboundString("user-initiated boom"),
		}),
		Trace: trace,
	}}}
	_, err := decodeResult(outboundResultBytes(t, result))
	if err == nil {
		t.Fatal("panic result decoded successfully")
	}
	want := "BAML panic: baml.panics.UserPanic: user-initiated boom\n    " + strings.Join(trace, "\n    ")
	if err.Error() != want {
		t.Fatalf("formatted panic = %q, want %q", err, want)
	}
	for _, frame := range trace {
		if count := strings.Count(err.Error(), frame); count != 1 {
			t.Fatalf("panic trace frame %q appears %d times in %q", frame, count, err)
		}
	}
}

func TestDecodeResultExitPanicUsesHostExitContract(t *testing.T) {
	previous := processExit
	t.Cleanup(func() { processExit = previous })

	called := false
	processExit = func(code int) {
		called = true
		if code != 7 {
			t.Fatalf("exit code = %d, want 7", code)
		}
	}
	result := &cffi.BamlOutboundResult{Result: &cffi.BamlOutboundResult_Panic{Panic: &cffi.BamlOutboundPanic{
		IsExitPanic: true, ExitCode: 7,
	}}}
	_, err := decodeResult(outboundResultBytes(t, result))
	if !called || err == nil || !strings.Contains(err.Error(), "returned unexpectedly") {
		t.Fatalf("called = %v, error = %v", called, err)
	}

	for _, code := range []int64{math.MinInt64, int64(math.MinInt32) - 1, int64(math.MaxInt32) + 1, math.MaxInt64} {
		if got := processExitCode(code); got != 1 {
			t.Fatalf("processExitCode(%d) = %d, want 1", code, got)
		}
	}
}

func TestDecodeResultSuccessStillDefersTypedDecoderErrors(t *testing.T) {
	result := &cffi.BamlOutboundResult{Result: &cffi.BamlOutboundResult_Ok{Ok: &cffi.BamlOutboundValue{
		Value: &cffi.BamlOutboundValue_IntValue{IntValue: 42},
	}}}
	value, err := decodeResult(outboundResultBytes(t, result))
	if err != nil {
		t.Fatal(err)
	}
	if _, err := value.String(); err == nil || !strings.Contains(err.Error(), "expected BAML string") {
		t.Fatalf("typed decoder error = %v", err)
	}
}

func TestUnexpectedNeverReturnIsDescriptive(t *testing.T) {
	err := UnexpectedNeverReturn("user.errors.AlwaysPanics")
	if !strings.Contains(err.Error(), "never-returning") || !strings.Contains(err.Error(), "user.errors.AlwaysPanics") {
		t.Fatalf("never-return error = %v", err)
	}
}

func TestCallRejectsNULInFunctionNameBeforeRuntimeInitialization(t *testing.T) {
	_, err := Call(context.Background(), "user.foo\x00bar", nil)
	if err == nil || err.Error() != "baml_go.Call: function name contains a NUL byte" {
		t.Fatalf("got error %v, want embedded-NUL diagnostic", err)
	}
}

func TestUnhandledSpawnError(t *testing.T) {
	t.Run("unhandled_spawn_error_uses_host_default", func(t *testing.T) {
		payload, err := proto.Marshal(&cffi.BamlOutboundResult{
			Result: &cffi.BamlOutboundResult_Error{
				Error: &cffi.BamlOutboundError{
					Value: &cffi.BamlOutboundValue{Value: &cffi.BamlOutboundValue_StringValue{StringValue: "boom"}},
				},
			},
		})
		if err != nil {
			t.Fatal(err)
		}
		defer func() {
			if recovered := recover(); recovered == nil {
				t.Fatal("default unhandled-spawn handler did not panic")
			}
		}()
		reportUnhandledSpawnError(payload, false)
	})
}

func TestRuntimeInitializationWaitHonorsCancellation(t *testing.T) {
	state := newNativeRuntimeState()
	if err := state.acquire(context.Background()); err != nil {
		t.Fatal(err)
	}
	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	if err := state.acquire(ctx); !errors.Is(err, context.Canceled) {
		t.Fatalf("got error %v, want context cancellation", err)
	}
	state.release()
}

func TestWaitForCallResultPreservesExactContextErrorWhenResultAndDoneAreReady(t *testing.T) {
	ctx, cancel := context.WithCancel(context.Background())
	result := make(chan []byte, 1)
	result <- []byte("already completed")
	cancel()

	for range 1000 {
		payload, err := waitForCallResult(ctx, result)
		if err != ctx.Err() {
			t.Fatalf("wait error identity = %v, want exact ctx.Err() %v", err, ctx.Err())
		}
		if payload != nil {
			t.Fatalf("cancelled wait returned payload %q", payload)
		}
		// Keep both select arms ready on every iteration. The assertion must
		// hold regardless of which ready arm the scheduler chooses.
		select {
		case result <- []byte("already completed"):
		default:
		}
	}
}

func TestReservePendingCallSkipsOccupiedIDAfterWraparound(t *testing.T) {
	previous := nextCallbackID.Load()
	nextCallbackID.Store(^uint32(0))
	occupied := &pendingCall{result: make(chan []byte, 1)}
	pendingCalls.Store(uint32(1), occupied)
	t.Cleanup(func() {
		pendingCalls.Delete(uint32(1))
		pendingCalls.Delete(uint32(2))
		nextCallbackID.Store(previous)
	})

	call := &pendingCall{result: make(chan []byte, 1)}
	if id := reservePendingCall(call); id != 2 {
		t.Fatalf("reserved callback ID %d, want 2", id)
	}
	if got, _ := pendingCalls.Load(uint32(1)); got != occupied {
		t.Fatal("occupied callback ID was overwritten")
	}
}

func TestEncodeCallUsesNamedKwargs(t *testing.T) {
	payload, err := encodeCall(42, map[string]Input{
		"text":  String("hello"),
		"count": Int64(3),
	})
	if err != nil {
		t.Fatal(err)
	}
	if len(payload) == 0 {
		t.Fatal("encoded payload was empty")
	}
}

func TestScalarValueAccessors(t *testing.T) {
	value := Value{value: &cffi.BamlOutboundValue{
		Value: &cffi.BamlOutboundValue_StringValue{StringValue: "hello"},
	}}
	got, err := value.String()
	if err != nil {
		t.Fatal(err)
	}
	if got != "hello" {
		t.Fatalf("got %q, want hello", got)
	}
}

func TestScalarValueAccessorsRejectNilLiteralPayloads(t *testing.T) {
	value := Value{value: &cffi.BamlOutboundValue{
		Value: &cffi.BamlOutboundValue_LiteralValue{},
	}}
	for name, decode := range map[string]func() error{
		"string": func() error { _, err := value.String(); return err },
		"int":    func() error { _, err := value.Int64(); return err },
		"bigint": func() error { _, err := value.BigInt(); return err },
		"float":  func() error { _, err := value.Float64(); return err },
		"bool":   func() error { _, err := value.Bool(); return err },
	} {
		t.Run(name, func(t *testing.T) {
			if err := decode(); err == nil {
				t.Fatal("nil literal payload unexpectedly decoded")
			}
		})
	}
}

func TestAbsentOutboundOneofIsNull(t *testing.T) {
	value := Value{value: &cffi.BamlOutboundValue{}}
	if isNull, err := value.isNull(); err != nil || !isNull {
		t.Fatalf("isNull returned (%v, %v), want (true, nil)", isNull, err)
	}
	if _, err := value.Null(); err != nil {
		t.Fatalf("absent outbound oneof did not decode as null: %v", err)
	}
}

func TestEncodeCallUsesExactClassAndFieldWireNames(t *testing.T) {
	payload, err := encodeCall(42, map[string]Input{
		"person_arg": Class("user.people.Person", map[string]Input{
			"age_years": Int64(37),
			"full_name": String("Ada"),
		}),
	})
	if err != nil {
		t.Fatal(err)
	}

	var call cffi.CallFunctionArgs
	if err := proto.Unmarshal(payload, &call); err != nil {
		t.Fatal(err)
	}
	if len(call.Kwargs) != 1 || call.Kwargs[0].GetStringKey() != "person_arg" {
		t.Fatalf("unexpected kwargs: %#v", call.Kwargs)
	}
	class := call.Kwargs[0].Value.GetClassValue()
	if class == nil ||
		call.Kwargs[0].Value.GetValueType().GetClassTy().GetName() != "user.people.Person" {
		t.Fatalf("unexpected class: %#v", class)
	}
	if len(class.Fields) != 2 {
		t.Fatalf("got %d fields, want 2", len(class.Fields))
	}
	if class.Fields[0].GetStringKey() != "age_years" || class.Fields[1].GetStringKey() != "full_name" {
		t.Fatalf("fields are not sorted by exact wire name: %#v", class.Fields)
	}
}

func TestClassValueValidatesNameAndDecodesFields(t *testing.T) {
	value := Value{value: &cffi.BamlOutboundValue{
		Value: &cffi.BamlOutboundValue_ClassValue{ClassValue: &cffi.BamlValueClass{
			Name: "user.Person",
			Fields: []*cffi.BamlOutboundMapEntry{
				{Key: "name", Value: &cffi.BamlOutboundValue{Value: &cffi.BamlOutboundValue_StringValue{StringValue: "Ada"}}},
				{Key: "age", Value: &cffi.BamlOutboundValue{Value: &cffi.BamlOutboundValue_IntValue{IntValue: 37}}},
			},
		}},
	}}

	class, err := value.Class("user.Person")
	if err != nil {
		t.Fatal(err)
	}
	name, err := class.String("name")
	if err != nil {
		t.Fatal(err)
	}
	age, err := class.Int64("age")
	if err != nil {
		t.Fatal(err)
	}
	if name != "Ada" || age != 37 {
		t.Fatalf("got (%q, %d), want (%q, %d)", name, age, "Ada", 37)
	}
	if _, err := value.Class("user.Other"); err == nil {
		t.Fatal("wrong class name unexpectedly succeeded")
	}
	if _, err := class.Bool("missing"); err == nil {
		t.Fatal("missing field unexpectedly succeeded")
	}
}

func TestClassValueDecodesNestedClass(t *testing.T) {
	value := Value{value: &cffi.BamlOutboundValue{
		Value: &cffi.BamlOutboundValue_ClassValue{ClassValue: &cffi.BamlValueClass{
			Name: "user.Outer",
			Fields: []*cffi.BamlOutboundMapEntry{{
				Key: "inner",
				Value: &cffi.BamlOutboundValue{Value: &cffi.BamlOutboundValue_ClassValue{
					ClassValue: &cffi.BamlValueClass{
						Name: "user.Inner",
						Fields: []*cffi.BamlOutboundMapEntry{{
							Key: "value",
							Value: &cffi.BamlOutboundValue{Value: &cffi.BamlOutboundValue_IntValue{
								IntValue: 42,
							}},
						}},
					},
				}},
			}},
		}},
	}}

	outer, err := value.Class("user.Outer")
	if err != nil {
		t.Fatal(err)
	}
	inner, err := outer.Class("inner", "user.Inner")
	if err != nil {
		t.Fatal(err)
	}
	got, err := inner.Int64("value")
	if err != nil {
		t.Fatal(err)
	}
	if got != 42 {
		t.Fatalf("got %d, want 42", got)
	}
	if _, err := outer.Class("inner", "user.Other"); err == nil {
		t.Fatal("wrong nested class name unexpectedly succeeded")
	}
}

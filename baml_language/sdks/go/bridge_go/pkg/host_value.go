// Per-process Go host-value registry.
//
// When Go code passes a callable as an argument to a BAML function, the
// inbound encoder in `proto.go` calls [registerHostValue] to obtain a `u64`
// key and emits `InboundValue::Handle{key, HOST_VALUE_CALLABLE}`. Rust's
// `inbound_to_external` decoder constructs a
// `BexExternalValue::HostValue(Arc<HostValueArc>)`; the engine binds it to an
// `Object::HostClosure`; later when BAML invokes it, the `call_host_value`
// sysop fires a `HostDispatchFn` via the bridge_cffi global, which lands here
// in [bamlHostDispatch].
//
// The dispatch callback:
//
//  1. Looks up the Go callable by `host_value_key`.
//  2. Decodes the `BamlOutboundValue` args (a list shape — see
//     `sys_native::host_impls::call_host_value`) into Go values.
//  3. Invokes the callable via reflection. Panics are recovered.
//  4. Encodes the result into `InboundValue` bytes and calls
//     `complete_host_call(call_id, 0, ptr, len)` via cgo.
//  5. On any error (panic, decode failure, arity mismatch, result encode
//     failure), encodes a `baml.errors.HostCallable` inbound value and calls
//     `complete_host_call(call_id, 1, ptr, len)`.
//
// When the engine drops the last Rust clone of the `HostValueArc`,
// [bamlHostRelease] fires and removes the registry entry. Unlike Python/Node,
// Go has no finalizer model — the registry release alone is enough since the
// user's closure is held by the registry and will become GC-eligible the
// moment the registry drops its only reference.

package pkg

/*
#include <stdint.h>
#include <stdlib.h>
*/
import "C"

import (
	"fmt"
	"math"
	"reflect"
	"runtime/debug"
	"sync"
	"sync/atomic"
	"unsafe"

	"bridge_go/cffi"
	pb "bridge_go/cffi/proto/baml_bridge/cffi/v1"

	"google.golang.org/protobuf/proto"
)

// hostValueRegistry holds Go callables that have been handed to BAML.
//
// The key is a freshly-allocated `uint64` (never 0). Removal happens in
// [bamlHostRelease] when Rust drops its last clone of the corresponding
// `HostValueArc`.
type hostValueRegistry struct {
	mu    sync.Mutex
	table map[uint64]reflect.Value
	next  atomic.Uint64
}

var hostValues = &hostValueRegistry{
	table: make(map[uint64]reflect.Value),
}

func init() {
	// Match Python/Node: 0 is reserved as "invalid". Start at 1 so the
	// first key handed out is non-zero even before the first add. Package
	// initializers may already have registered the static fallback error.
	hostValues.next.CompareAndSwap(0, 1)
}

func nextHostValueKey() uint64 {
	for {
		k := hostValues.next.Add(1) - 1
		if k != 0 {
			return k
		}
	}
}

// registerHostValue inserts a Go callable into the registry and returns its
// key. `fn` must be a `reflect.Value` whose `Kind() == reflect.Func` —
// callers should validate this before invoking, but we do not panic here on
// non-func values: the dispatch path catches it with a HostCallableError.
//
// Release tradeoff: the registry entry is normally dropped only when the
// engine garbage-collects the `HostClosure` it allocated for this callable
// and fires `bamlHostRelease` (a GC-timed release, drained by the engine
// after collection). The entry therefore outlives the call that introduced
// it, by an amount the host cannot control.
// The one place we release eagerly is the encode-error rollback in
// `encodeCallArgs` (via `unregisterHostValue`), because there the engine
// never sees the key at all and so would never release it.
func registerHostValue(fn reflect.Value) uint64 {
	key := nextHostValueKey()
	hostValues.mu.Lock()
	hostValues.table[key] = fn
	hostValues.mu.Unlock()
	return key
}

// bamlHostRelease drops the Go callable associated with host_value_key. Fires
// when the last Rust clone of the corresponding `HostValueArc` is dropped —
// see `bex_external_types::host_value::host_release_dispatch`.
//
//export bamlHostRelease
func bamlHostRelease(hostValueKey C.uint64_t) {
	hostValues.mu.Lock()
	delete(hostValues.table, uint64(hostValueKey))
	hostValues.mu.Unlock()
}

// unregisterHostValue drops the registry entry for `key`. Used by the encoder
// to roll back callables it registered for early kwargs when encoding a later
// kwarg fails (the call never reaches the engine, so the engine would never
// decode — and therefore never release — the orphaned key, leaking it for the
// life of the process). This is the bridge's own release path, equivalent to
// what `bamlHostRelease` does once the engine GCs the `HostClosure`.
func unregisterHostValue(key uint64) {
	hostValues.mu.Lock()
	delete(hostValues.table, key)
	hostValues.mu.Unlock()
}

// bamlHostDispatch is called by Rust when BAML invokes a host callable.
//
// `args` is a protobuf-encoded `BamlOutboundValue` whose variant is a
// `BamlValueList` (see `sys_native::host_impls::call_host_value` — args are
// wrapped as `BexExternalValue::Array` before encoding).
//
// We copy the wire bytes (the caller's pointer is only valid for the duration
// of the call) and dispatch the user callable on a worker goroutine. Rust's
// dispatch site is fire-and-return — it does not block on the result; the
// engine awaits a oneshot that `complete_host_call` later resolves — so
// returning promptly here keeps the dispatch path responsive when many host
// calls arrive in close succession.
//
//export bamlHostDispatch
func bamlHostDispatch(hostValueKey C.uint64_t, callID C.uint32_t, args *C.uint8_t, length C.size_t) {
	id := uint32(callID)

	// The byte-copy, registry lookup, and goroutine launch all run on the
	// cgo-callback stack. A panic that escaped here would (a) leave the
	// engine awaiting `call_id` forever and (b) cross the cgo boundary,
	// aborting the whole process. Recover so we always complete `call_id`
	// with an error and never propagate a panic across the FFI boundary.
	// (The goroutine that `runHostCallable` runs on has its own recover; a
	// panic *inside* `go runHostCallable(...)` after launch is handled there,
	// not here, since `go` returns before the goroutine runs.)
	defer func() {
		if r := recover(); r != nil {
			msg := fmt.Sprintf("host dispatch panicked before invoking callable: %v", r)
			completeCallSafely(id, "panic", msg, msg)
		}
	}()

	// Copy the wire bytes out of the caller-owned buffer.
	var argsBytes []byte
	if length > 0 && args != nil {
		// `C.GoBytes` takes a C `int` (32-bit); a `size_t` that exceeds
		// `MaxInt32` would truncate to a wrong (possibly negative) length and
		// read the wrong region. A multi-gigabyte args payload is never
		// legitimate, so reject it rather than risk a bad read.
		if length > C.size_t(math.MaxInt32) {
			sendHostCallableError(id, "ProtocolError",
				fmt.Sprintf("host-call args payload too large: %d bytes", uint64(length)))
			return
		}
		argsBytes = C.GoBytes(unsafe.Pointer(args), C.int(length))
	}

	key := uint64(hostValueKey)

	hostValues.mu.Lock()
	fn, ok := hostValues.table[key]
	hostValues.mu.Unlock()
	if !ok {
		sendHostCallableError(id, "KeyError",
			fmt.Sprintf("no host callable registered for key %d", key))
		return
	}

	go runHostCallable(fn, id, argsBytes)
}

// runHostCallable decodes args, invokes `fn`, and forwards the result or
// error back to Rust via `complete_host_call`.
//
// Panics, arity mismatches, and conversion failures are caught and surfaced as
// `baml.errors.HostCallable` values carrying Go error metadata.
func runHostCallable(fn reflect.Value, callID uint32, argsBytes []byte) {
	defer func() {
		if r := recover(); r != nil {
			// Best-effort: capture the goroutine's stack trace so the host
			// can see where the panic happened. `sendHostCallableError`
			// marshals a proto and crosses the cgo boundary — if *that*
			// panics (a second panic in the recovery path), an unrecovered
			// goroutine panic would abort the whole process and the engine
			// would still hang on `call_id`. `completeCallSafely` wraps the
			// normal error path in its own recover and falls back to a
			// pre-marshaled static payload so the call is always completed
			// exactly once and no panic escapes this goroutine.
			tb := string(debug.Stack())
			// The panic summary goes in `message`; the full value + stack goes
			// in `traceback`. Both surface on the BAML `HostCallable` class.
			completeCallSafely(callID, "panic",
				fmt.Sprintf("host callable panicked: %v", r),
				fmt.Sprintf("host callable panicked: %v\n%s", r, tb))
		}
	}()

	if fn.Kind() != reflect.Func {
		sendHostCallableError(callID, "TypeError",
			fmt.Sprintf("registered host value is not callable (kind=%s)", fn.Kind()))
		return
	}

	// Decode the wire payload: a `BamlToHostCall` the engine already resolved
	// against the callable's declared params (omitted optionals dropped), with
	// `Args` in declared order. Go invokes host callables positionally and has no
	// keyword-argument facility, so we take every supplied arg's value in order —
	// `ArgName`/`IsOptionalArg` are not used here. An optional left out in the
	// middle of the run cannot be expressed positionally and is unsupported (see
	// the bridge-go divergences notes).
	var toHostCall pb.BamlToHostCall
	if err := proto.Unmarshal(argsBytes, &toHostCall); err != nil {
		sendHostCallableError(callID, "DecodeError",
			fmt.Sprintf("failed to decode host-call args: %v", err))
		return
	}
	rawArgs := make([]*pb.BamlOutboundValue, 0, len(toHostCall.GetArgs()))
	for _, arg := range toHostCall.GetArgs() {
		rawArgs = append(rawArgs, arg.GetValue())
	}

	// Convert the decoded args to reflect.Values matching the function's
	// declared parameter types. `outboundToGo` returns idiomatic Go values
	// (string, int64, float64, bool, []any, map[string]any, DynamicEnum,
	// DynamicClass, DynamicUnion, BamlHandle); we then coerce each to the
	// declared parameter type.
	fnType := fn.Type()
	expectedNumArgs := fnType.NumIn()
	if fnType.IsVariadic() {
		// Variadic: at least NumIn-1 fixed args; trailing variadic slice
		// may be absent or any length.
		if len(rawArgs) < expectedNumArgs-1 {
			sendHostCallableError(callID, "TypeError",
				fmt.Sprintf("host callable expected at least %d args, got %d",
					expectedNumArgs-1, len(rawArgs)))
			return
		}
	} else if len(rawArgs) != expectedNumArgs {
		sendHostCallableError(callID, "TypeError",
			fmt.Sprintf("host callable expected %d args, got %d",
				expectedNumArgs, len(rawArgs)))
		return
	}

	in := make([]reflect.Value, len(rawArgs))
	for i, raw := range rawArgs {
		decoded, err := outboundToGo(raw)
		if err != nil {
			sendHostCallableError(callID, "DecodeError",
				fmt.Sprintf("failed to decode host-call arg %d: %v", i, err))
			return
		}
		// Figure out the declared param type for this slot.
		var paramType reflect.Type
		if fnType.IsVariadic() && i >= fnType.NumIn()-1 {
			// Inside the variadic slice — declared element type is
			// `fnType.In(NumIn-1).Elem()`.
			paramType = fnType.In(fnType.NumIn() - 1).Elem()
		} else {
			paramType = fnType.In(i)
		}
		conv, ok := coerceToType(decoded, paramType)
		if !ok {
			sendHostCallableError(callID, "TypeError",
				fmt.Sprintf("host callable arg %d: cannot convert %T to %s", i, decoded, paramType))
			return
		}
		in[i] = conv
	}

	// Invoke. The Call panics on arity / type mismatch, which we already
	// guard against above, but the deferred recover above also catches
	// anything that slips through (or a panic from inside the user's
	// callable).
	out := fn.Call(in)

	// Interpret the function's return shape. We accept three styles, matching
	// idiomatic Go callable signatures:
	//
	//  - func(...)             -> no return; encode null
	//  - func(...) T           -> single return; encode T
	//  - func(...) (T, error)  -> the trailing error becomes a HostCallable error
	var resultValue any
	switch len(out) {
	case 0:
		resultValue = nil
	case 1:
		// A lone `error` return is the callable's status, not a value: a
		// non-nil error surfaces as a `HostCallable` error, nil as a null/void
		// result. Without this a returned error would reach
		// `goToInboundValueTracking` and fail to encode, losing the error text.
		if fnType.Out(0).Implements(errorInterface) {
			if !isNilValue(out[0]) {
				if err, ok := out[0].Interface().(error); ok {
					surfaceCallableError(callID, err)
					return
				}
			}
			resultValue = nil
		} else {
			resultValue = out[0].Interface()
		}
	case 2:
		// The only 2-value host-callable shape we accept is `(value, error)`.
		// If the second return type does not implement `error` (e.g.
		// `func() (int, int)`), reject the signature rather than silently
		// dropping the second value.
		if !fnType.Out(1).Implements(errorInterface) {
			sendHostCallableError(callID, "TypeError",
				fmt.Sprintf("host callable's second return type %s does not implement error; expected (value, error)", fnType.Out(1)))
			return
		}
		// Surface a non-nil error. A typed-nil pointer error (e.g. a
		// `(T, *MyErr)` returning `(v, nil)`) is a non-nil interface wrapping a
		// nil pointer; `isNilValue` detects it so we neither call `.Error()` on
		// a nil receiver (a panic) nor turn a successful return into an error.
		if !isNilValue(out[1]) {
			if err, ok := out[1].Interface().(error); ok {
				surfaceCallableError(callID, err)
				return
			}
		}
		resultValue = out[0].Interface()
	default:
		sendHostCallableError(callID, "TypeError",
			fmt.Sprintf("host callable returned %d values; expected 0, 1, or (value, error)", len(out)))
		return
	}

	// Encode the result as an `InboundValue` (host->engine direction; no
	// type metadata — engine re-validates). Track any host callables the
	// encoder registers for values nested in the result: if encoding or
	// marshaling then fails, the bytes never reach the engine, so it never
	// decodes — and never releases — them. Roll those registrations back so
	// the user's callables don't leak (mirrors the argument-path rollback in
	// `encodeCallArgs`).
	var registered []uint64
	iv, err := goToInboundValueTracking(resultValue, &registered)
	if err != nil {
		rollbackRegisteredHostValues(registered)
		sendHostCallableError(callID, "EncodeError",
			fmt.Sprintf("failed to encode host-call result: %v", err))
		return
	}
	resultBytes, err := proto.Marshal(iv)
	if err != nil {
		rollbackRegisteredHostValues(registered)
		sendHostCallableError(callID, "EncodeError",
			fmt.Sprintf("failed to marshal host-call result: %v", err))
		return
	}
	completeHostCallSuccess(callID, resultBytes)
}

// coerceToType narrows an `outboundToGo` value to the function's declared
// param type. Returns the converted `reflect.Value` and `true` on success;
// `false` if no idiomatic conversion exists.
//
// Coverage matches the inverse of `goToInboundValueTracking` plus the proto decoder's
// natural Go types: string / int64 / float64 / bool / []byte / []any /
// map[string]any. We also widen `int64 -> int*` and `float64 -> float32` so
// users can write the natural Go signature `func(x int) string` even though
// the proto carries `int64`.
func coerceToType(v any, target reflect.Type) (reflect.Value, bool) {
	if v == nil {
		// nil is assignable to interface{} or pointer kinds.
		switch target.Kind() {
		case reflect.Interface, reflect.Ptr, reflect.Slice, reflect.Map, reflect.Chan, reflect.Func:
			return reflect.Zero(target), true
		default:
			return reflect.Value{}, false
		}
	}
	rv := reflect.ValueOf(v)
	// Fast path: already assignable.
	if rv.Type().AssignableTo(target) {
		return rv, true
	}
	// Numeric widening / narrowing. The proto wire carries integers as
	// `int64` and floats as `float64`; users may declare narrower Go types
	// (`int8`, `uint32`, `float32`, …). A bare `rv.Convert(target)` would
	// silently truncate or lose precision (e.g. `300` into `int8` becomes
	// `44`), corrupting the host call. We range/precision-check first and
	// reject out-of-range values so the dispatch path surfaces a clean
	// `HOST_CALLABLE_INVALID_ARGUMENT` instead of feeding garbage to the
	// user callable.
	if rv.Kind() == reflect.Int64 {
		src := rv.Int()
		switch target.Kind() {
		case reflect.Int, reflect.Int8, reflect.Int16, reflect.Int32, reflect.Int64:
			if !intFitsSigned(src, target.Bits()) {
				return reflect.Value{}, false
			}
			return rv.Convert(target), true
		case reflect.Uint, reflect.Uint8, reflect.Uint16, reflect.Uint32, reflect.Uint64, reflect.Uintptr:
			if !intFitsUnsigned(src, target.Bits()) {
				return reflect.Value{}, false
			}
			return rv.Convert(target), true
		case reflect.Float32:
			if !int64FitsFloat32(src) {
				return reflect.Value{}, false
			}
			return rv.Convert(target), true
		case reflect.Float64:
			// Every int64 fits a float64 magnitude-wise; mantissa
			// precision loss above 2^53 is accepted (matches Go's own
			// implicit int64→float64 in arithmetic).
			return rv.Convert(target), true
		case reflect.String:
			// Go converts int64 → string as a Unicode rune (65 → "A"), not a
			// decimal string — almost never the intent. Reject rather than
			// fall through to the convertible path and produce a surprising
			// one-rune string.
			return reflect.Value{}, false
		}
	}
	if rv.Kind() == reflect.Float64 {
		src := rv.Float()
		switch target.Kind() {
		case reflect.Float32:
			if !float64FitsFloat32(src) {
				return reflect.Value{}, false
			}
			return rv.Convert(target), true
		case reflect.Float64:
			return rv.Convert(target), true
		case reflect.Int, reflect.Int8, reflect.Int16, reflect.Int32, reflect.Int64,
			reflect.Uint, reflect.Uint8, reflect.Uint16, reflect.Uint32, reflect.Uint64,
			reflect.Uintptr:
			// A float argument cannot become an integer parameter without
			// truncation / precision loss (300.0 → int8 44, 3.9 → 3, NaN → 0,
			// +Inf → MaxInt64). Reject rather than silently corrupt.
			return reflect.Value{}, false
		}
	}
	// Convertible (e.g. string aliases). Numeric kinds are handled above
	// with range checks; everything reaching here is a non-narrowing
	// conversion (string→named-string, []byte aliases, etc.).
	if rv.Type().ConvertibleTo(target) {
		return rv.Convert(target), true
	}
	return reflect.Value{}, false
}

// intFitsSigned reports whether `v` fits exactly in a signed integer of the
// given bit width (`bits` is `target.Bits()`, e.g. 8/16/32/64; `int`/`uint`
// report their platform width). 64-bit always fits since the source is int64.
func intFitsSigned(v int64, bits int) bool {
	if bits >= 64 {
		return true
	}
	max := int64(1)<<(bits-1) - 1
	min := -(int64(1) << (bits - 1))
	return v >= min && v <= max
}

// intFitsUnsigned reports whether `v` fits exactly in an unsigned integer of
// the given bit width. Negative values never fit. `uint`/`uintptr` report
// the platform width (32 or 64); a 64-bit unsigned target accepts any
// non-negative int64.
func intFitsUnsigned(v int64, bits int) bool {
	if v < 0 {
		return false
	}
	if bits >= 64 {
		return true
	}
	max := int64(1)<<bits - 1
	return v <= max
}

// int64FitsFloat32 reports whether the integer `v` round-trips through
// float32 without changing value. Unlike a float-to-float narrowing (where
// mantissa rounding is expected), an *integer* arriving on the wire carries
// exact semantics, so silently rounding `16_777_217` down to `16_777_216`
// would be data corruption. We therefore require an exact round trip:
// float32's exact-integer range is ±2^24. (int64 can never overflow
// float32's magnitude range, so this only screens precision loss.)
func int64FitsFloat32(v int64) bool {
	f := float32(v)
	return int64(f) == v
}

// float64FitsFloat32 reports whether narrowing `v` to float32 is safe. We
// reject only *overflow* — a finite input whose magnitude exceeds float32's
// representable range becomes ±Inf, which is silent corruption. We accept
// ordinary mantissa rounding (e.g. `0.1` is not exactly representable in
// either float type) because rejecting it would break the common, legitimate
// case of declaring a `float32` parameter; that rounding is the documented
// semantics of a narrower float and matches Go's own implicit narrowing.
// Non-finite inputs (NaN/±Inf) represent themselves in float32 and pass.
func float64FitsFloat32(v float64) bool {
	if math.IsNaN(v) || math.IsInf(v, 0) {
		return true
	}
	// A finite input that overflows float32's range converts to ±Inf.
	return !math.IsInf(float64(float32(v)), 0)
}

// errorInterface is the reflect type of the built-in `error` interface, used
// to detect error-typed return values.
var errorInterface = reflect.TypeOf((*error)(nil)).Elem()

// isNilValue reports whether `v` is nil, including a typed-nil pointer boxed in
// an interface — where the interface itself is non-nil but the boxed pointer is
// nil (the classic Go typed-nil-error trap, where calling a method on the nil
// pointer panics).
func isNilValue(v reflect.Value) bool {
	switch v.Kind() {
	case reflect.Ptr, reflect.Slice, reflect.Map, reflect.Func, reflect.Chan:
		return v.IsNil()
	case reflect.Interface:
		return v.IsNil() || isNilValue(v.Elem())
	default:
		return false
	}
}

// surfaceCallableError completes the call with a `HostCallable` error built
// from `err`. The error text becomes the BAML `HostCallable` `message`; it is
// also mirrored into `traceback`, since a plain returned error carries no stack.
func surfaceCallableError(callID uint32, err error) {
	sendHostCallableError(callID, errorClassName(err), err.Error(),
		withTraceback(err.Error()), withOpaqueValue(err))
}

// errorClassName returns a best-effort short type name for `err`, matching
// the Python/Node convention of "ValueError" / "TypeError" / etc.
func errorClassName(err error) string {
	if err == nil {
		return "Error"
	}
	t := reflect.TypeOf(err)
	for t.Kind() == reflect.Ptr {
		t = t.Elem()
	}
	name := t.Name()
	// Unexported types (lowercase initial) are stdlib internals — e.g.
	// `errorString` from `errors.New`, `wrapError` from `fmt.Errorf` — whose
	// names leak Go internals rather than a meaningful class. Fall back to a
	// generic "Error" for those, matching the Python/Node convention for plain
	// errors; user-defined *exported* error types keep their name.
	if name == "" || name[0] < 'A' || name[0] > 'Z' {
		return "Error"
	}
	return name
}

type hostCallableErrorDetails struct {
	traceback   *string
	opaqueValue any
}

type errorOpt func(*hostCallableErrorDetails)

func withTraceback(tb string) errorOpt {
	return func(details *hostCallableErrorDetails) {
		details.traceback = &tb
	}
}

func withOpaqueValue(value any) errorOpt {
	return func(details *hostCallableErrorDetails) {
		details.opaqueValue = value
	}
}

func inboundStringField(key, value string) *pb.InboundMapEntry {
	return &pb.InboundMapEntry{
		Key: &pb.InboundMapEntry_StringKey{StringKey: key},
		Value: &pb.InboundValue{
			Value: &pb.InboundValue_StringValue{StringValue: value},
		},
	}
}

func marshalHostCallableError(
	className, message string,
	details hostCallableErrorDetails,
) ([]byte, error) {
	if details.opaqueValue == nil {
		details.opaqueValue = &BamlError{ClassName: className, Message: message}
	}
	handleKey := registerHostValue(reflect.ValueOf(details.opaqueValue))
	fields := []*pb.InboundMapEntry{
		inboundStringField("message", message),
		inboundStringField("class_name", className),
		inboundStringField("language", "go"),
	}
	if details.traceback != nil {
		fields = append(fields, inboundStringField("traceback", *details.traceback))
	}
	fields = append(fields, &pb.InboundMapEntry{
		Key: &pb.InboundMapEntry_StringKey{StringKey: "_handle"},
		Value: &pb.InboundValue{
			Value: &pb.InboundValue_Handle{Handle: &pb.BamlHandle{
				Key:        handleKey,
				HandleType: pb.BamlHandleType_HOST_VALUE_OPAQUE,
			}},
		},
	})
	inbound := &pb.InboundValue{
		ValueType: &pb.BamlTy{Ty: &pb.BamlTy_ClassTy{ClassTy: &pb.BamlTyClass{
			Name: "baml.errors.HostCallable",
		}}},
		Value: &pb.InboundValue_ClassValue{ClassValue: &pb.InboundClassValue{
			Fields: fields,
		}},
	}
	bytes, err := proto.Marshal(inbound)
	if err != nil {
		unregisterHostValue(handleKey)
		return nil, err
	}
	return bytes, nil
}

// sendHostCallableError encodes a `baml.errors.HostCallable` inbound value and
// forwards it to Rust via `complete_host_call(callID, 1, ptr, len)`.
func sendHostCallableError(callID uint32, className, message string, opts ...errorOpt) {
	details := hostCallableErrorDetails{}
	for _, o := range opts {
		o(&details)
	}
	bytes, marshalErr := marshalHostCallableError(className, message, details)
	if marshalErr != nil {
		bytes = lastResortErrorPayload
	}
	completeHostCallError(callID, bytes)
}

// completeHostCallSuccess forwards a successful host-call result to Rust.
func completeHostCallSuccess(callID uint32, bytes []byte) {
	cffi.CompleteHostCall(callID, 0, bytes)
}

// completeHostCallError forwards an error host-call result to Rust.
func completeHostCallError(callID uint32, bytes []byte) {
	cffi.CompleteHostCall(callID, 1, bytes)
}

// lastResortErrorPayload is a pre-marshaled `baml.errors.HostCallable` used
// when the normal error-encoding path itself panics.
var lastResortErrorPayload = mustMarshalHostCallableError()

func mustMarshalHostCallableError() []byte {
	b, err := marshalHostCallableError(
		"InternalError",
		"host callable failed and the error could not be encoded",
		hostCallableErrorDetails{},
	)
	if err != nil {
		panic(fmt.Sprintf("failed to marshal static HostCallable payload: %v", err))
	}
	return b
}

// completeCallSafely completes `callID` with an error, guaranteeing exactly
// one completion and that no panic escapes. It first attempts the normal
// `sendHostCallableError` path (rich class name / message / traceback); if
// that panics — e.g. a panic while marshaling the proto or crossing the cgo
// boundary — it falls back to a pre-marshaled static payload that cannot
// panic. This defends the recovery path itself against a second panic, which
// would otherwise abort the process (an unrecovered goroutine panic) and
// leave the engine hanging.
//
// Both `message` and `traceback` surface on the BAML `HostCallable` class:
// the summary belongs in `message`, the full panic value / stack in
// `traceback`.
func completeCallSafely(callID uint32, className, message, traceback string) {
	defer func() {
		if recover() != nil {
			// The rich error path panicked; complete with the static
			// payload. `cffi.CompleteHostCall` on already-marshaled bytes
			// is the only remaining operation and does not allocate or
			// marshal, so it cannot panic for these reasons.
			completeHostCallError(callID, lastResortErrorPayload)
		}
	}()
	sendHostCallableError(callID, className, message, withTraceback(traceback))
}

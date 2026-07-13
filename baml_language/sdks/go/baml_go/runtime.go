package baml_go

/*
#cgo LDFLAGS: -L${SRCDIR}/../../../target/debug -lbridge_cffi
#cgo darwin LDFLAGS: -Wl,-rpath,${SRCDIR}/../../../target/debug

#include <stdint.h>
#include <stddef.h>
#include <stdlib.h>

typedef struct {
	const int8_t *ptr;
	size_t len;
} BamlBuffer;

typedef void (*BamlResultCallback)(uint32_t call_id, const int8_t *content, size_t length);

extern BamlBuffer initialize_runtime_from_bytecode_ffi(const uint8_t *bytecode, size_t length);
extern void free_buffer(BamlBuffer buffer);
extern void register_callback(BamlResultCallback callback);
extern void call_function(const char *function_name, const uint8_t *encoded_args, size_t length, uint32_t callback_id);
extern uint64_t new_function_call(void);
extern int32_t cancel_function_call(uint64_t call_id);

extern void bamlGoResultCallback(uint32_t call_id, const int8_t *content, size_t length);

static void baml_register_go_callback(void) {
	register_callback(bamlGoResultCallback);
}
*/
import "C"

import (
	"context"
	"errors"
	"fmt"
	"math/big"
	"sort"
	"strings"
	"sync"
	"sync/atomic"
	"unsafe"

	"github.com/boundaryml/baml/sdks/go/baml_go/internal/cffi"
	"google.golang.org/protobuf/proto"
)

var (
	registerCallbackOnce sync.Once
	nextCallbackID       atomic.Uint32
	pendingCalls         sync.Map
)

type pendingCall struct {
	result chan []byte
}

// Initialize replaces the process-wide BAML runtime with the supplied
// serialized program. Generated projects normally call this through their
// internal bootstrap package exactly once.
func Initialize(bytecode []byte) error {
	var pointer *C.uint8_t
	if len(bytecode) != 0 {
		pointer = (*C.uint8_t)(unsafe.Pointer(&bytecode[0]))
	}

	buffer := C.initialize_runtime_from_bytecode_ffi(pointer, C.size_t(len(bytecode)))
	defer C.free_buffer(buffer)
	if buffer.len == 0 {
		return nil
	}
	message := C.GoBytes(unsafe.Pointer(buffer.ptr), C.int(buffer.len))
	return fmt.Errorf("initialize BAML runtime: %s", message)
}

// Input is a value supplied to a BAML callable.
type Input struct {
	value *cffi.InboundValue
}

// Null is the sole Go value corresponding to BAML's standalone null type.
// Optional and union types use their own generated representations.
type Null struct{}

func String(value string) Input {
	return Input{value: &cffi.InboundValue{Value: &cffi.InboundValue_StringValue{StringValue: value}}}
}

func Int64(value int64) Input {
	return Input{value: &cffi.InboundValue{Value: &cffi.InboundValue_IntValue{IntValue: value}}}
}

func BigInt(value *big.Int) Input {
	if value == nil {
		return Input{}
	}
	return Input{value: &cffi.InboundValue{Value: &cffi.InboundValue_BigintValue{BigintValue: value.Text(16)}}}
}

func Float64(value float64) Input {
	return Input{value: &cffi.InboundValue{Value: &cffi.InboundValue_FloatValue{FloatValue: value}}}
}

func Bool(value bool) Input {
	return Input{value: &cffi.InboundValue{Value: &cffi.InboundValue_BoolValue{BoolValue: value}}}
}

func NullInput(_ Null) Input {
	return Input{value: &cffi.InboundValue{}}
}

func Uint8Array(value []byte) Input {
	return Input{value: &cffi.InboundValue{Value: &cffi.InboundValue_Uint8ArrayValue{
		Uint8ArrayValue: append([]byte(nil), value...),
	}}}
}

// Value is a value returned by a BAML callable. Its typed accessors validate
// the wire value before exposing it to generated code.
type Value struct {
	value *cffi.BamlOutboundValue
}

// Call invokes one fully-qualified BAML callable and blocks until it returns
// or the context is cancelled.
func Call(ctx context.Context, function string, args map[string]Input) (Value, error) {
	if ctx == nil {
		return Value{}, errors.New("baml_go.Call: nil context")
	}
	if err := ctx.Err(); err != nil {
		return Value{}, err
	}

	registerCallbackOnce.Do(func() { C.baml_register_go_callback() })

	engineCallID := uint64(C.new_function_call())
	if engineCallID == 0 {
		return Value{}, errors.New("BAML returned an invalid zero call ID")
	}
	callbackID := nextNonzeroCallbackID()
	call := &pendingCall{result: make(chan []byte, 1)}
	pendingCalls.Store(callbackID, call)

	encoded, err := encodeCall(engineCallID, args)
	if err != nil {
		pendingCalls.Delete(callbackID)
		return Value{}, err
	}
	functionName := C.CString(function)
	defer C.free(unsafe.Pointer(functionName))
	var encodedPointer *C.uint8_t
	if len(encoded) != 0 {
		encodedPointer = (*C.uint8_t)(unsafe.Pointer(&encoded[0]))
	}
	C.call_function(functionName, encodedPointer, C.size_t(len(encoded)), C.uint32_t(callbackID))

	select {
	case payload := <-call.result:
		return decodeResult(payload)
	case <-ctx.Done():
		pendingCalls.Delete(callbackID)
		C.cancel_function_call(C.uint64_t(engineCallID))
		return Value{}, ctx.Err()
	}
}

func nextNonzeroCallbackID() uint32 {
	for {
		id := nextCallbackID.Add(1)
		if id != 0 {
			return id
		}
	}
}

func encodeCall(callID uint64, args map[string]Input) ([]byte, error) {
	keys := make([]string, 0, len(args))
	for key := range args {
		keys = append(keys, key)
	}
	sort.Strings(keys)

	kwargs := make([]*cffi.InboundMapEntry, 0, len(keys))
	for _, key := range keys {
		input := args[key]
		if input.value == nil {
			return nil, fmt.Errorf("argument %q has an uninitialized baml_go.Input", key)
		}
		kwargs = append(kwargs, &cffi.InboundMapEntry{
			Key:   &cffi.InboundMapEntry_StringKey{StringKey: key},
			Value: input.value,
		})
	}

	payload, err := proto.Marshal(&cffi.CallFunctionArgs{CallId: callID, Kwargs: kwargs})
	if err != nil {
		return nil, fmt.Errorf("encode BAML call: %w", err)
	}
	return payload, nil
}

func decodeResult(payload []byte) (Value, error) {
	result := &cffi.BamlOutboundResult{}
	if err := proto.Unmarshal(payload, result); err != nil {
		return Value{}, fmt.Errorf("decode BAML result: %w", err)
	}

	switch item := result.Result.(type) {
	case *cffi.BamlOutboundResult_Ok:
		if item.Ok == nil {
			return Value{}, errors.New("BAML returned an empty success value")
		}
		return Value{value: item.Ok}, nil
	case *cffi.BamlOutboundResult_Error:
		return Value{}, outboundFailure("BAML error", item.Error.GetTrace())
	case *cffi.BamlOutboundResult_Panic:
		return Value{}, outboundFailure("BAML panic", item.Panic.GetTrace())
	default:
		return Value{}, errors.New("BAML returned an empty result envelope")
	}
}

func outboundFailure(kind string, trace []string) error {
	if len(trace) == 0 {
		return errors.New(kind)
	}
	return fmt.Errorf("%s:\n%s", kind, strings.Join(trace, "\n"))
}

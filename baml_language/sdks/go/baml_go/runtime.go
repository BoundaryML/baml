package baml_go

import (
	"context"
	"errors"
	"fmt"
	"math/big"
	"sort"
	"strings"
	"sync"
	"sync/atomic"

	"github.com/boundaryml/baml-go/internal/cffi"
	"google.golang.org/protobuf/proto"
)

var (
	registerCallbackOnce sync.Once
	nextCallbackID       atomic.Uint32
	pendingCalls         sync.Map
)

var nativeRuntime = newNativeRuntimeState()

type nativeRuntimeState struct {
	initialization chan struct{}
	loaded         bool
	path           string
	version        string
}

func newNativeRuntimeState() *nativeRuntimeState {
	state := &nativeRuntimeState{initialization: make(chan struct{}, 1)}
	state.initialization <- struct{}{}
	return state
}

func (state *nativeRuntimeState) acquire(ctx context.Context) error {
	select {
	case <-ctx.Done():
		return ctx.Err()
	case <-state.initialization:
		return nil
	}
}

func (state *nativeRuntimeState) release() {
	state.initialization <- struct{}{}
}

type pendingCall struct {
	result chan []byte
}

// Initialize replaces the process-wide BAML runtime with the supplied
// serialized program. Generated projects normally call this through their
// internal bootstrap package exactly once.
func Initialize(bytecode []byte) error {
	if err := ensureNativeRuntime(context.Background()); err != nil {
		return err
	}
	return nativeInitialize(bytecode)
}

func ensureNativeRuntime(ctx context.Context) error {
	if err := nativeRuntime.acquire(ctx); err != nil {
		return err
	}
	defer nativeRuntime.release()
	if nativeRuntime.loaded {
		return nil
	}
	config, err := currentRuntimeConfig()
	if err != nil {
		return err
	}
	path, expectedVersion, err := resolveRuntime(ctx, config)
	if err != nil {
		return err
	}
	actualVersion, err := nativeOpen(path)
	if err != nil {
		return err
	}
	if err := nativeRegisterBridge(requiredRuntimeVersion()); err != nil {
		nativeCloseAfterLoadFailure()
		return err
	}
	if expectedVersion != "" && actualVersion != expectedVersion {
		nativeCloseAfterLoadFailure()
		return fmt.Errorf("BAML runtime version mismatch: artifact is %s but library reports %s", expectedVersion, actualVersion)
	}
	nativeRuntime.loaded = true
	nativeRuntime.path = path
	nativeRuntime.version = actualVersion
	return nil
}

// Input is a value supplied to a BAML callable.
type Input struct {
	value *cffi.InboundValue
	err   error
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
	if strings.IndexByte(function, 0) >= 0 {
		return Value{}, errors.New("baml_go.Call: function name contains a NUL byte")
	}

	if err := ensureNativeRuntime(ctx); err != nil {
		return Value{}, err
	}
	registerCallbackOnce.Do(nativeRegisterCallback)

	engineCallID := nativeNewFunctionCall()
	if engineCallID == 0 {
		return Value{}, errors.New("BAML returned an invalid zero call ID")
	}
	call := &pendingCall{result: make(chan []byte, 1)}
	callbackID := reservePendingCall(call)

	encoded, err := encodeCall(engineCallID, args)
	if err != nil {
		pendingCalls.Delete(callbackID)
		return Value{}, err
	}
	nativeCall(function, encoded, callbackID)

	select {
	case payload := <-call.result:
		return decodeResult(payload)
	case <-ctx.Done():
		pendingCalls.Delete(callbackID)
		nativeCancel(engineCallID)
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

func reservePendingCall(call *pendingCall) uint32 {
	for {
		id := nextNonzeroCallbackID()
		if _, loaded := pendingCalls.LoadOrStore(id, call); !loaded {
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
		if input.err != nil {
			return nil, fmt.Errorf("argument %q: %w", key, input.err)
		}
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

package baml_go

import (
	"errors"
	"fmt"
	"reflect"
	"runtime/debug"
	"sync"
	"sync/atomic"

	"github.com/boundaryml/baml-go/internal/cffi"
	"google.golang.org/protobuf/proto"
)

// HostCallableFunc is the wire-level adapter implemented by generated callback
// codecs. Values are already decoded from the canonical BAML callback argument
// list; the returned Input is validated against the callback's declared return
// type by the BAML runtime.
type HostCallableFunc func([]Value) (Input, error)

// HostCallableArityError reports malformed runtime dispatch without requiring
// generated packages to import formatting helpers solely for adapter guards.
func HostCallableArityError(expected, actual int) error {
	return fmt.Errorf("BAML host callable expected %d arguments, got %d", expected, actual)
}

// HostCallableArgumentError identifies the callback argument whose canonical
// generated decoder rejected the runtime payload.
func HostCallableArgumentError(index int, err error) error {
	return fmt.Errorf("decode BAML host-call argument %d: %w", index, err)
}

type hostValue struct {
	callable HostCallableFunc
	opaque   any
}

type hostCallablePanicError struct {
	message   string
	recovered any
}

func (err *hostCallablePanicError) Error() string { return err.message }

var hostValues = struct {
	sync.Mutex
	next  atomic.Uint64
	table map[uint64]hostValue
}{table: make(map[uint64]hostValue)}

var completeNativeHostCall = nativeCompleteHostCall

func nextHostValueKey() uint64 {
	for {
		key := hostValues.next.Add(1)
		if key != 0 {
			return key
		}
	}
}

func registerHostValue(value hostValue) uint64 {
	key := nextHostValueKey()
	hostValues.Lock()
	hostValues.table[key] = value
	hostValues.Unlock()
	return key
}

func unregisterHostValue(key uint64) {
	if key == 0 {
		return
	}
	hostValues.Lock()
	delete(hostValues.table, key)
	hostValues.Unlock()
}

func lookupHostCallable(key uint64) (HostCallableFunc, bool) {
	hostValues.Lock()
	value, ok := hostValues.table[key]
	hostValues.Unlock()
	return value.callable, ok && value.callable != nil
}

// HostCallable encodes a generated callback adapter as a host-owned callable.
// Registration is deferred until the containing call is dispatched so failed
// argument encoding cannot leak the Go closure. After dispatch, ownership
// transfers to the native HostValueArc: removal is driven by the registered
// host-release callback and is idempotent. Prompt removal after an otherwise
// normal call is not guaranteed because the engine may retain its HostClosure
// until a later garbage-collection cycle.
func HostCallable(callable HostCallableFunc) Input {
	if callable == nil {
		return InvalidInput("BAML host callable is nil")
	}
	return Input{deferred: &inputEncoder{encode: func(transaction *inputTransaction) (*cffi.InboundValue, error) {
		if transaction == nil {
			return nil, errors.New("BAML host callable requires an input transaction")
		}
		key := registerHostValue(hostValue{callable: callable})
		transaction.ownHostValue(key)
		return hostValueInput(key, cffi.BamlHandleType_HOST_VALUE_CALLABLE), nil
	}}}
}

func hostValueInput(key uint64, kind cffi.BamlHandleType) *cffi.InboundValue {
	return &cffi.InboundValue{Value: &cffi.InboundValue_Handle{Handle: &cffi.BamlHandle{
		Key: key, HandleType: kind,
	}}}
}

func dispatchHostCall(key uint64, callID uint32, payload []byte) {
	var call cffi.BamlToHostCall
	if err := proto.Unmarshal(payload, &call); err != nil {
		releaseResultOwner(ownHostCallOutboundHandles(&call))
		completeHostCallFailure(callID, fmt.Errorf("decode BAML host-call arguments: %w", err), "")
		return
	}
	// Take ownership of every decoded native handle before looking up the
	// callable or validating argument metadata. Rejected unknown/malformed
	// dispatches must release their handles just as successful adapters do.
	owner := ownHostCallOutboundHandles(&call)
	handedToAdapter := false
	defer func() {
		if !handedToAdapter {
			releaseResultOwner(owner)
		}
	}()

	callable, ok := lookupHostCallable(key)
	if !ok {
		completeHostCallFailure(callID, fmt.Errorf("no Go host callable registered for key %d", key), "")
		return
	}

	values := make([]Value, len(call.Args))
	for index, argument := range call.Args {
		if argument == nil || argument.Value == nil {
			completeHostCallFailure(callID, fmt.Errorf("BAML host-call argument %d is empty", index), "")
			return
		}
		if argument.IsOptionalArg || argument.ArgName != "" {
			completeHostCallFailure(callID, fmt.Errorf("BAML host-call argument %d is not required positional", index), "")
			return
		}
		values[index] = Value{value: argument.Value, owner: owner}
	}

	handedToAdapter = true
	go runHostCall(callID, callable, values, owner)
}

func ownHostCallOutboundHandles(call *cffi.BamlToHostCall) *resultOwner {
	allHandles := make(map[uint64]struct{})
	if call != nil {
		for _, argument := range call.Args {
			if argument != nil {
				collectOutboundHandles(argument.Value, allHandles)
			}
		}
	}
	return ownOutboundHandleKeys(allHandles)
}

func runHostCall(callID uint32, callable HostCallableFunc, values []Value, owner *resultOwner) {
	finished := false
	defer releaseResultOwner(owner)
	defer func() {
		if finished {
			return
		}
		releaseResultOwner(owner)
		owner = nil
		if recovered := recover(); recovered != nil {
			message := fmt.Sprintf("Go host callable panicked: %v", recovered)
			completeHostCallFailure(callID, &hostCallablePanicError{message: message, recovered: recovered}, string(debug.Stack()))
			return
		}
		// runtime.Goexit is not a panic: recover returns nil while deferred
		// functions still run. Complete explicitly so the native caller cannot
		// remain suspended forever.
		completeHostCallFailure(callID, errors.New("Go host callable exited without returning"), string(debug.Stack()))
	}()
	result, err := callable(values)
	// Generated adapters have finished decoding every argument, cloning any
	// media handle that escapes into a typed Go value. Release the original
	// engine-owned argument handles deterministically before completing.
	releaseResultOwner(owner)
	owner = nil
	if err != nil {
		completeHostCallFailure(callID, err, "")
		finished = true
		return
	}
	completeHostCallInput(callID, false, result)
	finished = true
}

func completeHostCallFailure(callID uint32, err error, traceback string) {
	if err == nil {
		err = errors.New("Go host callable failed")
	}
	key := registerHostValue(hostValue{opaque: err})
	// Class is static except for the pre-registered opaque handle. Associate
	// that key with the completion transaction so failed encoding rolls it back.
	input := Input{deferred: &inputEncoder{encode: func(transaction *inputTransaction) (*cffi.InboundValue, error) {
		if transaction == nil {
			unregisterHostValue(key)
			return nil, errors.New("BAML host error requires an input transaction")
		}
		transaction.ownHostValue(key)
		return Class("baml.errors.HostCallable", map[string]Input{
			"message":    String(err.Error()),
			"class_name": String(hostErrorClassName(err)),
			"language":   String("go"),
			"traceback":  Optional(tracebackOrNil(traceback), String),
			"_handle":    {value: hostValueInput(key, cffi.BamlHandleType_HOST_VALUE_OPAQUE)},
		}).encodeValue(transaction)
	}}}
	completeHostCallInput(callID, true, input)
}

func tracebackOrNil(traceback string) *string {
	if traceback == "" {
		return nil
	}
	return &traceback
}

func hostErrorClassName(err error) string {
	if err == nil {
		return "Error"
	}
	typeOf := reflect.TypeOf(err)
	for typeOf.Kind() == reflect.Pointer {
		typeOf = typeOf.Elem()
	}
	name := typeOf.Name()
	if name == "" || name[0] < 'A' || name[0] > 'Z' {
		return "Error"
	}
	return name
}

func completeHostCallInput(callID uint32, isError bool, input Input) {
	transaction := &inputTransaction{}
	encoded, err := input.encodeValue(transaction)
	if err != nil {
		transaction.rollback()
		if !isError {
			completeHostCallFailure(callID, fmt.Errorf("encode Go host-call result: %w", err), "")
			return
		}
		// The failure encoder itself failed. Complete with an empty error payload;
		// the runtime deterministically promotes this bridge fault to SdkPanic.
		completeNativeHostCall(callID, true, nil)
		return
	}
	payload, err := proto.Marshal(encoded)
	if err != nil {
		transaction.rollback()
		if !isError {
			completeHostCallFailure(callID, fmt.Errorf("marshal Go host-call result: %w", err), "")
			return
		}
		completeNativeHostCall(callID, true, nil)
		return
	}
	completeNativeHostCall(callID, isError, payload)
	transaction.commitHostValues()
	transaction.rollback()
}

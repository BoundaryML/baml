package baml_go

import (
	"context"
	"errors"
	"fmt"
	"reflect"
	"runtime/debug"
	"sync"
	"sync/atomic"

	"github.com/boundaryml/baml-go/internal/cffi"
	"google.golang.org/protobuf/proto"
)

// HostCallArguments is the validated engine-to-Go callback argument split.
// Required arguments retain declared positional order. Supplied optional
// arguments are keyed by their canonical BAML parameter name; omitted optional
// arguments are absent.
type HostCallArguments struct {
	required []Value
	optional map[string]Value
	frame    *hostInvocationFrame
}

// Context carries the callback's inherited invocation state and cancellation.
// Pass it to BAML re-entry or derive a child context to preserve those controls.
func (arguments HostCallArguments) Context() context.Context {
	if arguments.frame != nil {
		return arguments.frame.ctx
	}
	return context.Background()
}

// RequiredCount returns the number of required positional arguments.
func (arguments HostCallArguments) RequiredCount() int { return len(arguments.required) }

// Required returns a required positional argument. Generated adapters validate
// the count before indexing.
func (arguments HostCallArguments) Required(index int) Value { return arguments.required[index] }

// Optional returns a supplied optional argument by its canonical BAML name.
func (arguments HostCallArguments) Optional(name string) (Value, bool) {
	value, ok := arguments.optional[name]
	return value, ok
}

// OptionalCount returns the number of supplied optional arguments.
func (arguments HostCallArguments) OptionalCount() int { return len(arguments.optional) }

// HostCallableFunc is the wire-level adapter implemented by generated callback
// codecs. Arguments are already validated and partitioned according to the
// canonical BAML callback calling convention; the returned Input is validated
// against the callback's declared return type by the BAML runtime.
type HostCallableFunc func(HostCallArguments) (Input, error)

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

// HostCallableOptionalArgumentError identifies a supplied named callback
// argument whose generated decoder rejected the runtime payload.
func HostCallableOptionalArgumentError(name string, err error) error {
	return fmt.Errorf("decode BAML host-call optional argument %q: %w", name, err)
}

// HostCallableOptionalCountError reports optional names outside the generated
// callback contract without exposing the runtime's internal argument map.
func HostCallableOptionalCountError(expected, actual int) error {
	return fmt.Errorf("BAML host callable recognized %d supplied optional arguments, got %d", expected, actual)
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
// HostCallable registers callable for the engine to invoke and sends its
// handle, stating functionType beside it: a host callable has no type of its
// own, so the one its slot declares is what the engine binds it to.
func HostCallable(callable HostCallableFunc, functionType BAMLType) Input {
	if callable == nil {
		return InvalidInput("BAML host callable is nil")
	}
	if functionType.value == nil || functionType.value.GetFunction() == nil {
		return InvalidInput("BAML host callable requires its function type")
	}
	return Input{deferred: &inputEncoder{encode: func(transaction *inputTransaction) (*cffi.InboundValue, error) {
		if transaction == nil {
			return nil, errors.New("BAML host callable requires an input transaction")
		}
		stated, err := functionType.stated(transaction)
		if err != nil {
			return nil, fmt.Errorf("BAML host callable function type: %w", err)
		}
		key := registerHostValue(hostValue{callable: callable})
		transaction.ownHostValue(key)
		value := hostValueInput(key, cffi.BamlHandleType_HOST_VALUE_CALLABLE)
		value.ValueType = stated
		return value, nil
	}}}
}

func hostValueInput(key uint64, kind cffi.BamlHandleType) *cffi.InboundValue {
	return &cffi.InboundValue{Value: &cffi.InboundValue_Handle{Handle: &cffi.BamlHandle{
		Key: key, HandleType: kind,
	}}}
}

func dispatchHostCall(key uint64, callID uint32, payload []byte, frames ...*hostInvocationFrame) {
	var frame *hostInvocationFrame
	if len(frames) != 0 {
		frame = frames[0]
	}
	handedToAdapter := false
	defer func() {
		if !handedToAdapter && frame != nil {
			frame.finish()
		}
	}()
	var call cffi.BamlToHostCall
	if err := proto.Unmarshal(payload, &call); err != nil {
		releaseResultOwner(ownHostCallOutboundHandles(&call))
		completeHostCallBridgeFailure(callID, fmt.Sprintf("decode BAML host-call arguments: %v", err))
		return
	}
	// Take ownership of every decoded native handle before looking up the
	// callable or validating argument metadata. Rejected unknown/malformed
	// dispatches must release their handles just as successful adapters do.
	owner := ownHostCallOutboundHandles(&call)
	defer func() {
		if !handedToAdapter {
			releaseResultOwner(owner)
		}
	}()

	callable, ok := lookupHostCallable(key)
	if !ok {
		completeHostCallBridgeFailure(callID, fmt.Sprintf("no Go host callable registered for key %d", key))
		return
	}

	required := make([]Value, 0, len(call.Args))
	optional := make(map[string]Value)
	seenOptional := false
	for index, argument := range call.Args {
		if argument == nil || argument.Value == nil {
			completeHostCallBridgeFailure(callID, fmt.Sprintf("BAML host-call argument %d is empty", index))
			return
		}
		value := Value{value: argument.Value, owner: owner}
		if argument.IsOptionalArg {
			seenOptional = true
			if argument.ArgName == "" {
				completeHostCallBridgeFailure(callID, fmt.Sprintf("BAML host-call optional argument %d has no name", index))
				return
			}
			if _, duplicate := optional[argument.ArgName]; duplicate {
				completeHostCallBridgeFailure(callID, fmt.Sprintf("BAML host-call optional argument %q is duplicated", argument.ArgName))
				return
			}
			optional[argument.ArgName] = value
			continue
		}
		if argument.ArgName != "" {
			completeHostCallBridgeFailure(callID, fmt.Sprintf("BAML host-call required argument %d unexpectedly has name %q", index, argument.ArgName))
			return
		}
		if seenOptional {
			completeHostCallBridgeFailure(callID, fmt.Sprintf("BAML host-call required argument %d follows optional arguments", index))
			return
		}
		required = append(required, value)
	}

	handedToAdapter = true
	go runHostCall(callID, callable, HostCallArguments{required: required, optional: optional, frame: frame}, owner)
}

func ownHostCallOutboundHandles(call *cffi.BamlToHostCall) *resultOwner {
	var keys []uint64
	if call != nil {
		for _, argument := range call.Args {
			if argument != nil {
				keys = appendOutboundHandles(keys, argument.Value)
			}
		}
	}
	return ownOutboundHandleKeys(keys)
}

func runHostCall(callID uint32, callable HostCallableFunc, arguments HostCallArguments, owner *resultOwner) {
	finished := false
	if arguments.frame != nil {
		defer arguments.frame.finish()
	}
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
	if err := arguments.Context().Err(); err != nil {
		completeHostCallFailure(callID, err, "")
		finished = true
		return
	}
	result, err := callable(arguments)
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
	completeHostCallReturned(callID, result)
	finished = true
}

// completeHostCallFailure completes the call as an exception of the host's
// own: the engine builds the baml.errors.HostCallable whose _handle is the
// registry key of err, so the same process recovers err by identity.
func completeHostCallFailure(callID uint32, err error, traceback string) {
	if err == nil {
		err = errors.New("Go host callable failed")
	}
	key := registerHostValue(hostValue{opaque: err})
	raised := &cffi.HostException{
		Message:   err.Error(),
		ClassName: hostErrorClassName(err),
		Language:  "go",
		Traceback: tracebackOrNil(traceback),
		Handle:    key,
	}
	if !completeHostCall(callID, &cffi.HostCallCompletion{Outcome: &cffi.HostCallCompletion_Raised{Raised: raised}}) {
		unregisterHostValue(key)
	}
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

// completeHostCallReturned completes the call with the callable's result. A
// result the adapter cannot encode is a fault of the bridge's own: the
// generated adapter already typed it.
func completeHostCallReturned(callID uint32, input Input) {
	transaction := &inputTransaction{}
	encoded, err := input.encodeValue(transaction)
	if err != nil {
		transaction.rollback()
		completeHostCallBridgeFailure(callID, fmt.Sprintf("encode Go host-call result: %v", err))
		return
	}
	if !completeHostCall(callID, &cffi.HostCallCompletion{Outcome: &cffi.HostCallCompletion_Returned{Returned: encoded}}) {
		transaction.rollback()
		return
	}
	transaction.commit()
}

// completeHostCallBridgeFailure completes the call as a failure of the
// bridge's own, which the engine surfaces as baml.panics.SdkPanic.
func completeHostCallBridgeFailure(callID uint32, message string) {
	completeHostCall(callID, &cffi.HostCallCompletion{Outcome: &cffi.HostCallCompletion_BridgeFailure{BridgeFailure: message}})
}

// completeHostCall delivers a completion. It reports false when the payload
// could not be marshalled; the call is then completed as a bridge failure
// (an empty payload carries no outcome) and the caller rolls back whatever
// the completion transferred.
func completeHostCall(callID uint32, completion *cffi.HostCallCompletion) bool {
	payload, err := proto.Marshal(completion)
	if err != nil {
		completeNativeHostCall(callID, nil)
		return false
	}
	completeNativeHostCall(callID, payload)
	return true
}

type invocationStateContextKey struct{}
type invocationStateCarrier struct {
	state       uint64
	owner       *resultOwner
	cancel      *cffi.BamlOutboundValue
	cancelOwner *resultOwner
}
type hostInvocationFrame struct {
	ctx    context.Context
	finish func()
}

var hostInvocationCancellations sync.Map

func dispatchHostInvocation(payload []byte) {
	var envelope cffi.HostInvocation
	if err := proto.Unmarshal(payload, &envelope); err != nil {
		releaseResultOwner(ownOutboundHandles(envelope.Cancel))
		if envelope.EffectiveState != 0 {
			releaseOutboundHandle(envelope.EffectiveState)
		}
		if envelope.CallbackId != 0 {
			completeHostCallBridgeFailure(envelope.CallbackId, fmt.Sprintf("decode host invocation: %v", err))
		}
		return
	}
	carrier := &invocationStateCarrier{state: envelope.EffectiveState,
		owner: ownOutboundHandleKeys([]uint64{envelope.EffectiveState})}
	carrier.cancel = envelope.Cancel
	carrier.cancelOwner = ownOutboundHandles(envelope.Cancel)
	ctx, cancel := context.WithCancel(context.WithValue(context.Background(), invocationStateContextKey{}, carrier))
	hostInvocationCancellations.Store(envelope.CallbackId, cancel)
	var once sync.Once
	frame := &hostInvocationFrame{ctx: ctx, finish: func() {
		once.Do(func() {
			hostInvocationCancellations.Delete(envelope.CallbackId)
		})
	}}
	dispatchHostCall(envelope.HostValueKey, envelope.CallbackId, envelope.ApplicationArgs, frame)
}

func cancelHostInvocation(id uint32) {
	if cancel, ok := hostInvocationCancellations.Load(id); ok {
		cancel.(context.CancelFunc)()
	}
}

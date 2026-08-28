package baml_go

import (
	"context"
	"errors"
	"fmt"
	"log"
	"math"
	"math/big"
	"os"
	"sort"
	"strconv"
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
	processExit          = os.Exit
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
	return InitializeWithMetadata(bytecode, "")
}

func InitializeWithMetadata(bytecode []byte, embeddedBamlToml string) error {
	if err := ensureNativeRuntime(context.Background()); err != nil {
		return err
	}
	return nativeInitialize(bytecode, embeddedBamlToml)
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
	if err := nativeRegisterBridge(BridgeRuntimeName, GetToolchainVersion(), GetBridgeRuntimeVersion()); err != nil {
		nativeCloseAfterLoadFailure()
		return err
	}
	nativeRegisterUnhandledSpawnErrorCallback()
	if expectedVersion != "" && actualVersion != expectedVersion {
		_ = nativeShutdown()
		nativeCloseAfterLoadFailure()
		return fmt.Errorf("BAML runtime version mismatch: artifact is %s but library reports %s", expectedVersion, actualVersion)
	}
	nativeRuntime.loaded = true
	nativeRuntime.path = path
	nativeRuntime.version = actualVersion
	return nil
}

// Shutdown waits for spawned work, reports unreachable errors through Go's
// panic handler, and releases the process-wide runtime.
func Shutdown() error {
	if err := nativeRuntime.acquire(context.Background()); err != nil {
		return err
	}
	defer nativeRuntime.release()
	if !nativeRuntime.loaded {
		return nil
	}
	return nativeShutdown()
}

func reportUnhandledSpawnError(payload []byte, cancelled bool) {
	_, err := decodeResult(payload)
	if err == nil {
		err = errors.New("BAML spawned work failed without an error result")
	}
	if cancelled {
		log.Printf("BAML spawned work was cancelled: %v", err)
		return
	}
	panic(err)
}

// Input is a value supplied to a BAML callable.
type Input struct {
	value    *cffi.InboundValue
	err      error
	deferred *inputEncoder
}

// inputEncoder is indirect so exported Input retains its original comparable
// Go representation even when it carries deferred transactional work.
type inputEncoder struct {
	encode func(*inputTransaction) (*cffi.InboundValue, error)
}

// inputTransaction owns native handles cloned while one call is encoded.
// cleanup always attempts to release every key. The native call entry point
// synchronously decodes arguments before returning: successfully decoded
// handles have already been drained (so release harmlessly reports invalid),
// while any handle after a decode failure remains in the table and is freed.
type inputTransaction struct {
	keys          []uint64
	hostValueKeys []uint64
}

func (transaction *inputTransaction) own(key uint64) {
	transaction.keys = append(transaction.keys, key)
}

func (transaction *inputTransaction) ownHostValue(key uint64) {
	transaction.hostValueKeys = append(transaction.hostValueKeys, key)
}

// commitHostValues transfers host-value lifetime to the native runtime after
// it has synchronously decoded an inbound payload. Native handle clones remain
// transaction-owned and are released by rollback as before; host values are
// instead released exactly once by the registered host-release callback.
func (transaction *inputTransaction) commitHostValues() {
	if transaction != nil {
		transaction.hostValueKeys = nil
	}
}

func (transaction *inputTransaction) rollback() {
	if transaction == nil {
		return
	}
	for _, key := range transaction.keys {
		if key != 0 {
			releaseInboundHandle(key)
		}
	}
	transaction.keys = nil
	for _, key := range transaction.hostValueKeys {
		unregisterHostValue(key)
	}
	transaction.hostValueKeys = nil
}

func (input Input) encodeValue(transaction *inputTransaction) (*cffi.InboundValue, error) {
	if input.err != nil {
		return nil, input.err
	}
	if input.deferred != nil {
		return input.deferred.encode(transaction)
	}
	if input.value == nil {
		return nil, errors.New("uninitialized baml_go.Input")
	}
	return input.value, nil
}

// Null is the sole Go value corresponding to BAML's standalone null type.
// Optional and union types use their own generated representations.
type Null struct{}

// BAMLInput and BAMLType let a standalone null participate in generic
// parameters and reflectively encoded containers without losing its exact
// wire descriptor.
func (value Null) BAMLInput() Input { return NullInput(value) }
func (Null) BAMLType() BAMLType     { return PrimitiveBAMLType(NullType) }

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

// MustBigIntLiteral parses a generator-owned decimal bigint literal. Generated
// source only calls this with compiler-validated constants.
func MustBigIntLiteral(value string) *big.Int {
	integer, ok := new(big.Int).SetString(value, 10)
	if !ok {
		panic("invalid generated BAML bigint literal: " + value)
	}
	return integer
}

// MustFloatLiteral parses compiler-owned BAML float source text at runtime.
// Generated source must not embed float literals as Go constants: Go's
// constant conversion loses the sign bit of -0.0 before it becomes float64.
// strconv.ParseFloat preserves IEEE-754 details such as signed zero and uses
// the same decimal/exponent syntax accepted by BAML's compiler. BAML has no
// NaN or infinity literal syntax, so those non-finite spellings are rejected
// even though strconv accepts them. Finite decimal underflow follows normal
// IEEE rounding to signed zero; overflow is rejected.
func MustFloatLiteral(value string) float64 {
	parsed, err := strconv.ParseFloat(value, 64)
	if err != nil {
		var numberError *strconv.NumError
		underflow := errors.As(err, &numberError) && numberError.Err == strconv.ErrRange && !math.IsInf(parsed, 0)
		if !underflow {
			panic("invalid generated BAML float literal: " + value)
		}
	}
	if math.IsNaN(parsed) || math.IsInf(parsed, 0) {
		panic("invalid generated BAML float literal: " + value)
	}
	return parsed
}

func Float64(value float64) Input {
	return Input{value: &cffi.InboundValue{Value: &cffi.InboundValue_FloatValue{FloatValue: value}}}
}

func Bool(value bool) Input {
	return Input{value: &cffi.InboundValue{Value: &cffi.InboundValue_BoolValue{BoolValue: value}}}
}

// Type encodes a reflected BAML type value. The descriptor remains opaque to
// callers and is validated before it crosses the ABI.
func Type(value BAMLType) Input {
	if err := validateBAMLTypeValue(value); err != nil {
		return InvalidInput("invalid BAML type value: " + err.Error())
	}
	if value.definition != nil {
		return Input{value: &cffi.InboundValue{Value: &cffi.InboundValue_TyDefValue{
			TyDefValue: proto.Clone(value.definition).(*cffi.BamlTyDef),
		}}}
	}
	return Input{value: &cffi.InboundValue{Value: &cffi.InboundValue_TyValue{TyValue: value.value}}}
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
	owner *resultOwner
}

// FunctionOperation selects a semantic projection of one authored function.
// Direct is the zero value for protobuf/backwards compatibility.
type FunctionOperation int32

const (
	FunctionOperationDirect FunctionOperation = iota
	FunctionOperationSpec
	FunctionOperationStream
)

func (operation FunctionOperation) valid() bool {
	return operation >= FunctionOperationDirect && operation <= FunctionOperationStream
}

// Call invokes one fully-qualified BAML callable and blocks until it returns
// or the context is cancelled.
func Call(ctx context.Context, function string, args map[string]Input) (Value, error) {
	return callWithTypeArgsOperation(ctx, function, args, nil, FunctionOperationDirect)
}

// CallOperation invokes a semantic projection on the original authored FQN.
// Operation dispatch never translates the FQN into a synthetic `$...` name.
func CallOperation(ctx context.Context, function string, operation FunctionOperation, args map[string]Input) (Value, error) {
	return callWithTypeArgsOperation(ctx, function, args, nil, operation)
}

// TypeArgument binds one BAML callable type parameter to a concrete runtime
// type. Generated generic functions construct these from Go type arguments;
// callers normally use the generated surface instead of this wire-level type.
type TypeArgument struct {
	Name string
	Type BAMLType
}

type callOptions struct {
	arguments map[string]Input
	typeArgs  []TypeArgument
}

// CallOption is shared by every generated callable option type. WithTypeArg
// therefore composes with ordinary defaulted arguments and nil options remain
// valid Go values.
type CallOption func(*callOptions)

// WithTypeArg explicitly binds one named BAML type parameter. A later binding
// of the same name replaces the generated Go-instantiation default in place.
func WithTypeArg(name string, value BAMLType) CallOption {
	return func(options *callOptions) {
		for index := range options.typeArgs {
			if options.typeArgs[index].Name == name {
				options.typeArgs[index].Type = value
				return
			}
		}
		options.typeArgs = append(options.typeArgs, TypeArgument{Name: name, Type: value})
	}
}

// WithArgument is the generator hook used by ordinary optional BAML
// parameters. Public generated setters retain their type-safe signatures.
func WithArgument(name string, value Input) CallOption {
	return func(options *callOptions) { options.arguments[name] = value }
}

// ApplyCallOptions mutates arguments and returns an independent ordered type
// binding slice. The engine remains authoritative for unknown parameter names.
func ApplyCallOptions(arguments map[string]Input, defaults []TypeArgument, values ...CallOption) []TypeArgument {
	options := callOptions{
		arguments: arguments,
		typeArgs:  append([]TypeArgument(nil), defaults...),
	}
	for _, value := range values {
		if value != nil {
			value(&options)
		}
	}
	return options.typeArgs
}

// CallWithTypeArgs invokes a generic BAML callable with explicit, named type
// bindings. Bindings use structured BAML descriptors so nested classes,
// containers, and unions retain their complete runtime identity.
func CallWithTypeArgs(ctx context.Context, function string, args map[string]Input, typeArgs []TypeArgument) (Value, error) {
	return callWithTypeArgsOperation(ctx, function, args, typeArgs, FunctionOperationDirect)
}

// CallWithTypeArgsOperation is the generic-call form of CallOperation.
func CallWithTypeArgsOperation(ctx context.Context, function string, operation FunctionOperation, args map[string]Input, typeArgs []TypeArgument) (Value, error) {
	return callWithTypeArgsOperation(ctx, function, args, typeArgs, operation)
}

func callWithTypeArgsOperation(ctx context.Context, function string, args map[string]Input, typeArgs []TypeArgument, operation FunctionOperation) (Value, error) {
	if ctx == nil {
		return Value{}, errors.New("baml_go.Call: nil context")
	}
	if err := ctx.Err(); err != nil {
		return Value{}, err
	}
	if strings.IndexByte(function, 0) >= 0 {
		return Value{}, errors.New("baml_go.Call: function name contains a NUL byte")
	}
	if !operation.valid() {
		return Value{}, fmt.Errorf("baml_go.Call: unknown function operation %d", operation)
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

	encoded, transaction, err := encodeCallForDispatchWithTargetAndTypeArgs(
		engineCallID,
		args,
		typeArgs,
		namedCallTarget(function, operation),
	)
	if err != nil {
		pendingCalls.Delete(callbackID)
		return Value{}, err
	}
	defer transaction.rollback()
	nativeCall(encoded, callbackID)
	transaction.commitHostValues()

	payload, err := waitForCallResult(ctx, call.result)
	if err != nil {
		pendingCalls.Delete(callbackID)
		nativeCancel(engineCallID)
		return Value{}, err
	}
	return decodeResult(payload)
}

func callHandle(ctx context.Context, handleKey uint64, args map[string]Input) (Value, error) {
	return callHandleOperation(ctx, handleKey, FunctionOperationDirect, args)
}

func callHandleOperation(ctx context.Context, handleKey uint64, operation FunctionOperation, args map[string]Input) (Value, error) {
	if ctx == nil {
		return Value{}, errors.New("baml_go.Function.Call: nil context")
	}
	if err := ctx.Err(); err != nil {
		return Value{}, err
	}
	if handleKey == 0 {
		return Value{}, errors.New("baml_go.Function.Call: zero handle")
	}
	if !operation.valid() {
		return Value{}, fmt.Errorf("baml_go.Function.Call: invalid function operation %d", operation)
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
	encoded, transaction, err := encodeCallForDispatchWithTargetAndTypeArgs(
		engineCallID,
		args,
		nil,
		handleCallTarget(handleKey, operation),
	)
	if err != nil {
		pendingCalls.Delete(callbackID)
		return Value{}, err
	}
	defer transaction.rollback()
	nativeCall(encoded, callbackID)
	transaction.commitHostValues()

	payload, err := waitForCallResult(ctx, call.result)
	if err != nil {
		pendingCalls.Delete(callbackID)
		nativeCancel(engineCallID)
		return Value{}, err
	}
	return decodeResult(payload)
}

func waitForCallResult(ctx context.Context, result <-chan []byte) ([]byte, error) {
	select {
	case payload := <-result:
		// Cancellation is part of the public Go API contract. If the native
		// result and ctx.Done become ready together, preserve the exact context
		// error instead of nondeterministically exposing the runtime's
		// cancellation panic envelope.
		if err := ctx.Err(); err != nil {
			return nil, err
		}
		return payload, nil
	case <-ctx.Done():
		return nil, ctx.Err()
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
	payload, transaction, err := encodeCallForDispatch(callID, args)
	if transaction != nil {
		transaction.rollback()
	}
	return payload, err
}

func encodeCallForDispatch(callID uint64, args map[string]Input) ([]byte, *inputTransaction, error) {
	return encodeCallForDispatchWithTypeArgs(callID, args, nil)
}

func encodeCallForDispatchWithTypeArgs(callID uint64, args map[string]Input, typeArgs []TypeArgument) ([]byte, *inputTransaction, error) {
	return encodeCallForDispatchWithTargetAndTypeArgs(callID, args, typeArgs, nil)
}

type callTarget struct {
	functionName   *string
	functionHandle *uint64
	operation      FunctionOperation
}

func namedCallTarget(name string, operation FunctionOperation) *callTarget {
	return &callTarget{functionName: &name, operation: operation}
}

func handleCallTarget(handle uint64, operation FunctionOperation) *callTarget {
	return &callTarget{functionHandle: &handle, operation: operation}
}

func encodeCallForDispatchWithTargetAndTypeArgs(
	callID uint64,
	args map[string]Input,
	typeArgs []TypeArgument,
	target *callTarget,
) ([]byte, *inputTransaction, error) {
	transaction := &inputTransaction{}
	failed := true
	defer func() {
		if failed {
			transaction.rollback()
		}
	}()

	keys := make([]string, 0, len(args))
	for key := range args {
		keys = append(keys, key)
	}
	sort.Strings(keys)

	kwargs := make([]*cffi.InboundMapEntry, 0, len(keys))
	for _, key := range keys {
		input := args[key]
		value, err := input.encodeValue(transaction)
		if err != nil {
			return nil, nil, fmt.Errorf("argument %q: %w", key, err)
		}
		kwargs = append(kwargs, &cffi.InboundMapEntry{
			Key:   &cffi.InboundMapEntry_StringKey{StringKey: key},
			Value: value,
		})
	}

	encodedTypeArgs := make([]*cffi.BamlTyArg, 0, len(typeArgs))
	seenTypeArgs := make(map[string]struct{}, len(typeArgs))
	for index, binding := range typeArgs {
		if binding.Name == "" {
			return nil, nil, fmt.Errorf("type argument %d has an empty name", index)
		}
		if _, duplicate := seenTypeArgs[binding.Name]; duplicate {
			return nil, nil, fmt.Errorf("duplicate type argument %q", binding.Name)
		}
		if err := validateBAMLTypeValue(binding.Type); err != nil {
			return nil, nil, fmt.Errorf("type argument %q: %w", binding.Name, err)
		}
		seenTypeArgs[binding.Name] = struct{}{}
		encoded := &cffi.BamlTyArg{TypeVar: binding.Name}
		if binding.Type.definition != nil {
			encoded.TypeDefinition = proto.Clone(binding.Type.definition).(*cffi.BamlTyDef)
		} else {
			encoded.TypeValue = proto.Clone(binding.Type.value).(*cffi.BamlTy)
		}
		encodedTypeArgs = append(encodedTypeArgs, encoded)
	}

	callArgs := &cffi.CallFunctionArgs{
		CallId:   callID,
		Kwargs:   kwargs,
		TypeArgs: encodedTypeArgs,
	}
	if target != nil && target.functionName != nil {
		callArgs.CallTarget = &cffi.CallFunctionArgs_FunctionName{FunctionName: *target.functionName}
	} else if target != nil && target.functionHandle != nil {
		callArgs.CallTarget = &cffi.CallFunctionArgs_FunctionHandle{FunctionHandle: *target.functionHandle}
	}
	if target != nil {
		callArgs.Operation = cffi.FunctionOperation(target.operation)
	}
	payload, err := proto.Marshal(callArgs)
	if err != nil {
		return nil, nil, fmt.Errorf("encode BAML call: %w", err)
	}
	failed = false
	return payload, transaction, nil
}

func decodeResult(payload []byte) (Value, error) {
	result := &cffi.BamlOutboundResult{}
	if err := proto.Unmarshal(payload, result); err != nil {
		return Value{}, fmt.Errorf("decode BAML result: %w", err)
	}
	return decodeResultEnvelope(result)
}

func decodeResultEnvelope(result *cffi.BamlOutboundResult) (Value, error) {
	if result == nil {
		return Value{}, errors.New("BAML returned a nil result envelope")
	}
	switch item := result.Result.(type) {
	case *cffi.BamlOutboundResult_Ok:
		if item.Ok == nil {
			return Value{}, errors.New("BAML returned an empty success value")
		}
		owner := ownOutboundHandles(item.Ok)
		return Value{value: item.Ok, owner: owner}, nil
	case *cffi.BamlOutboundResult_Error:
		if item.Error == nil {
			return Value{}, errors.New("BAML returned an empty error payload")
		}
		failure := outboundFailure("error", item.Error.GetValue(), item.Error.GetTrace())
		releaseOutboundHandles(item.Error.GetValue())
		releaseOutboundOpaqueHostValues(item.Error.GetValue())
		return Value{}, failure
	case *cffi.BamlOutboundResult_Panic:
		if item.Panic == nil {
			return Value{}, errors.New("BAML returned an empty panic payload")
		}
		if item.Panic.GetIsExitPanic() {
			releaseOutboundHandles(item.Panic.GetValue())
			releaseOutboundOpaqueHostValues(item.Panic.GetValue())
			processExit(processExitCode(item.Panic.GetExitCode()))
			return Value{}, errors.New("BAML process-exit handler returned unexpectedly")
		}
		failure := outboundFailure("panic", item.Panic.GetValue(), item.Panic.GetTrace())
		releaseOutboundHandles(item.Panic.GetValue())
		releaseOutboundOpaqueHostValues(item.Panic.GetValue())
		return Value{}, failure
	default:
		return Value{}, errors.New("BAML returned an empty result envelope")
	}
}

func processExitCode(code int64) int {
	// Match the Rust bridge's host contract: process APIs consume an i32 exit
	// code, and an out-of-range BAML value falls back to a generic failure.
	if code < math.MinInt32 || code > math.MaxInt32 {
		return 1
	}
	return int(code)
}

// BamlError is the structured base payload for an ordinary BAML throw. The
// three BEP-066 reflection channels below are distinct concrete Go errors;
// other BAML classes remain BamlError values with their exact class identity.
type BamlError struct {
	Kind      string
	ClassName string
	Message   string
	Trace     []string
	Value     any
}

func (failure *BamlError) Error() string { return renderBamlError(failure) }

type Diagnostic struct {
	Code    string
	Message string
	Span    any
}

type CompilationError struct {
	BamlError
	Diagnostics []Diagnostic
}

func (failure *CompilationError) Error() string { return renderBamlError(&failure.BamlError) }

type EvaluationError struct{ BamlError }

func (failure *EvaluationError) Error() string { return renderBamlError(&failure.BamlError) }

type SessionBusy struct{ BamlError }

func (failure *SessionBusy) Error() string { return renderBamlError(&failure.BamlError) }

func outboundFailure(kind string, value *cffi.BamlOutboundValue, trace []string) error {
	className, message := outboundFailureIdentity(value)
	if className == "" {
		className = "baml." + kind
	}
	decoded, _ := decodeDynamicValue(Value{value: value}, "error", 0)
	base := BamlError{
		Kind: kind, ClassName: className, Message: message,
		Trace: append([]string(nil), trace...), Value: decoded,
	}
	switch className {
	case "reflect.errors.CompilationError":
		return &CompilationError{BamlError: base, Diagnostics: decodeDiagnostics(decoded)}
	case "reflect.errors.EvaluationError":
		return &EvaluationError{BamlError: base}
	case "reflect.errors.SessionBusy":
		return &SessionBusy{BamlError: base}
	default:
		return &base
	}
}

func renderBamlError(failure *BamlError) string {
	var rendered strings.Builder
	fmt.Fprintf(&rendered, "BAML %s: %s", failure.Kind, failure.ClassName)
	if failure.Message != "" {
		fmt.Fprintf(&rendered, ": %s", failure.Message)
	}
	for _, line := range failure.Trace {
		rendered.WriteString("\n    ")
		rendered.WriteString(line)
	}
	return rendered.String()
}

func decodeDiagnostics(value any) []Diagnostic {
	object, ok := value.(map[string]any)
	if !ok {
		return nil
	}
	rows, ok := object["diagnostics"].([]any)
	if !ok {
		return nil
	}
	diagnostics := make([]Diagnostic, 0, len(rows))
	for _, row := range rows {
		fields, ok := row.(map[string]any)
		if !ok {
			continue
		}
		code, _ := fields["code"].(string)
		message, _ := fields["message"].(string)
		diagnostics = append(diagnostics, Diagnostic{Code: code, Message: message, Span: fields["span"]})
	}
	return diagnostics
}

func outboundFailureIdentity(value *cffi.BamlOutboundValue) (string, string) {
	// A thrown member of a union is wrapped in the same selected-arm envelope
	// as an ordinary union value. Aliases and throws unions must not hide the
	// concrete class identity from the host error string.
	for depth := 0; value != nil && depth < 64; depth++ {
		switch item := value.Value.(type) {
		case *cffi.BamlOutboundValue_UnionVariantValue:
			if item.UnionVariantValue == nil {
				return "", ""
			}
			value = item.UnionVariantValue.GetValue()
			continue
		case *cffi.BamlOutboundValue_ClassValue:
			if item.ClassValue == nil {
				return "", ""
			}
			message := ""
			for _, field := range item.ClassValue.GetFields() {
				if field != nil && field.GetKey() == "message" && field.GetValue() != nil {
					if decoded, err := (Value{value: field.GetValue()}).String(); err == nil {
						message = decoded
					}
					break
				}
			}
			return item.ClassValue.GetName(), message
		default:
			return "", ""
		}
	}
	return "", ""
}

// UnexpectedNeverReturn reports native/runtime drift for a BAML function
// whose canonical return type is never. Generated functions call this only
// when the bridge unexpectedly receives an ok result arm.
func UnexpectedNeverReturn(function string) error {
	return fmt.Errorf("BAML never-returning function %q returned successfully", function)
}

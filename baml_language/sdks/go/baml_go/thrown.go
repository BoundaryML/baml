package baml_go

import (
	"errors"
	"fmt"
	"reflect"
	"runtime"
	"strconv"

	"github.com/boundaryml/baml-go/internal/cffi"
)

// ThrownValue is a BAML value a callback throws. A callback whose declared
// throws names BAML values returns Throw(value) as its error, with value of
// the Go type its generated signature documents; the value crosses as the
// declared throws type, so BAML's typed catch arms match it.
type ThrownValue[T any] struct {
	Value T
}

func (thrown *ThrownValue[T]) Error() string {
	return fmt.Sprintf("BAML value thrown by a host callable: %v", thrown.Value)
}

func (thrown *ThrownValue[T]) thrownGoType() reflect.Type {
	return reflect.TypeFor[T]()
}

// Throw makes value the BAML value a callback throws. Return it as the
// callback's error.
func Throw[T any](value T) error {
	return &ThrownValue[T]{Value: value}
}

// thrownByCallback is implemented by every ThrownValue, so a value thrown
// with a Go type other than the callback's declared one is recognized.
type thrownByCallback interface {
	error
	thrownGoType() reflect.Type
}

// HostCallableError is how a generated callback adapter returns the error its
// callback returned when the callback's declared throws names BAML values:
// a value thrown with Throw as the declared Go type T is encoded with encode,
// as the declared throws type; any other error is classified as every
// callback's error is.
func HostCallableError[T any](err error, encode func(T) Input) (Input, error) {
	if thrown, ok := err.(*ThrownValue[T]); ok {
		return Input{}, &hostCallThrow{value: encode(thrown.Value)}
	}
	return Input{}, err
}

// hostCallThrow is a value a generated adapter already encoded as the
// callback's declared throws type.
type hostCallThrow struct {
	value Input
}

func (*hostCallThrow) Error() string { return "BAML value thrown by a host callable" }

// errorPayload is the value a BAML throw or panic carried, as the engine sent
// it, together with the engine references it transferred to Go. A callback
// that returns the error throws this same value on.
type errorPayload struct {
	value *cffi.BamlOutboundValue
	owner *resultOwner
}

// bamlErrorCarrier is implemented by BamlError and every error type that
// embeds it.
type bamlErrorCarrier interface {
	error
	bamlError() *BamlError
}

func (failure *BamlError) bamlError() *BamlError { return failure }

// bridgeFaultError marks a refusal of the bridge's own: the value is one the
// callable produced in good faith that the bridge cannot carry (deeper than
// the wire, a cycle past an encoder's cap, a handle that could not be
// cloned), as opposed to a value that does not fit its declared type.
type bridgeFaultError interface {
	error
	bridgeFault()
}

func (*wireDepthError) bridgeFault()    {}
func (*classNestingError) bridgeFault() {}
func (*nestingCapError) bridgeFault()   {}
func (*handleCloneError) bridgeFault()  {}
func (*forwardFault) bridgeFault()      {}

func isBridgeFault(err error) bool {
	var fault bridgeFaultError
	return errors.As(err, &fault)
}

// nestingCapError is a value nested past an encoder's own recursion cap. The
// caps bound the encoders' recursion, not what crosses: the exact measure is
// checkInboundDepth's. They sit far past the wire's 100 messages (every level
// a value nests costs at least one), so only a value the wire refuses anyway,
// a cycle, or a chain of hundreds of Go pointers reaches one.
type nestingCapError struct {
	what  string
	path  string
	limit int
}

func (err *nestingCapError) Error() string {
	return fmt.Sprintf("encode %s at %s: nesting exceeds %d levels (deeper than the wire carries, or a cycle)", err.what, err.path, err.limit)
}

// handleCloneError is a handle Go holds that could not be cloned for a
// request.
type handleCloneError struct {
	what string
	err  error
}

func (err *handleCloneError) Error() string {
	return fmt.Sprintf("clone BAML %s handle for input: %v", err.what, err.err)
}

func (err *handleCloneError) Unwrap() error { return err.err }

// cloneForInput clones key for one inbound occurrence and journals the clone
// in transaction, which releases it if the request never reaches the engine.
func cloneForInput(transaction *inputTransaction, key uint64, what string) (uint64, error) {
	if transaction == nil {
		return 0, fmt.Errorf("a BAML %s handle is sent only within a call's input transaction", what)
	}
	cloned, err := cloneInboundHandle(key)
	if err != nil {
		return 0, &handleCloneError{what: what, err: err}
	}
	if cloned == 0 {
		return 0, &handleCloneError{what: what, err: errors.New("runtime returned a zero handle")}
	}
	transaction.own(cloned)
	return cloned, nil
}

// forwardFault is a value the engine sent Go that Go cannot send back. The
// engine built the value, so failing to carry it is the bridge's own fault.
type forwardFault struct {
	err error
}

func (fault *forwardFault) Error() string {
	return fmt.Sprintf("the value the engine sent cannot be sent back: %v", fault.err)
}

func (fault *forwardFault) Unwrap() error { return fault.err }

// forwardInput is the inbound encoding of a value the engine sent Go, so a
// callback can throw a BAML error it received on: the same graph, each node
// stated by the type the engine annotated it with, every engine handle a
// fresh clone and every host key as it arrived.
func (payload *errorPayload) forwardInput() Input {
	return Input{deferred: &inputEncoder{encode: func(transaction *inputTransaction) (*cffi.InboundValue, error) {
		encoded, err := outboundToInbound(payload.value, transaction)
		runtime.KeepAlive(payload.owner)
		if err != nil {
			return nil, &forwardFault{err: err}
		}
		return encoded, nil
	}}}
}

func outboundToInbound(value *cffi.BamlOutboundValue, transaction *inputTransaction) (*cffi.InboundValue, error) {
	if value == nil {
		return nil, errors.New("thrown value is empty")
	}
	switch item := value.Value.(type) {
	case nil, *cffi.BamlOutboundValue_NullValue:
		return &cffi.InboundValue{}, nil
	case *cffi.BamlOutboundValue_StringValue:
		return &cffi.InboundValue{Value: &cffi.InboundValue_StringValue{StringValue: item.StringValue}}, nil
	case *cffi.BamlOutboundValue_IntValue:
		return &cffi.InboundValue{Value: &cffi.InboundValue_IntValue{IntValue: item.IntValue}}, nil
	case *cffi.BamlOutboundValue_FloatValue:
		return &cffi.InboundValue{Value: &cffi.InboundValue_FloatValue{FloatValue: item.FloatValue}}, nil
	case *cffi.BamlOutboundValue_BoolValue:
		return &cffi.InboundValue{Value: &cffi.InboundValue_BoolValue{BoolValue: item.BoolValue}}, nil
	case *cffi.BamlOutboundValue_BigintValue:
		return &cffi.InboundValue{Value: &cffi.InboundValue_BigintValue{BigintValue: item.BigintValue}}, nil
	case *cffi.BamlOutboundValue_Uint8ArrayValue:
		return &cffi.InboundValue{Value: &cffi.InboundValue_Uint8ArrayValue{
			Uint8ArrayValue: append([]byte(nil), item.Uint8ArrayValue...),
		}}, nil
	case *cffi.BamlOutboundValue_LiteralValue:
		return literalToInbound(item.LiteralValue)
	case *cffi.BamlOutboundValue_EnumValue:
		if item.EnumValue == nil {
			return nil, errors.New("thrown enum value is empty")
		}
		return &cffi.InboundValue{Value: &cffi.InboundValue_EnumValue{EnumValue: &cffi.InboundEnumValue{
			Name: item.EnumValue.GetName(), Value: item.EnumValue.GetValue(),
		}}}, nil
	case *cffi.BamlOutboundValue_UnionVariantValue:
		// A union is not stated on the wire: the selected member states
		// itself, and the engine holds it to the slot.
		return outboundToInbound(item.UnionVariantValue.GetValue(), transaction)
	case *cffi.BamlOutboundValue_ListValue:
		list := item.ListValue
		if list == nil {
			return nil, errors.New("thrown list value is empty")
		}
		stated, err := annotationStatement(ListBAMLType(BAMLType{value: list.GetItemType()}).value)
		if err != nil {
			return nil, fmt.Errorf("list: %w", err)
		}
		values := make([]*cffi.InboundValue, len(list.GetItems()))
		for index, element := range list.GetItems() {
			if values[index], err = outboundToInbound(element, transaction); err != nil {
				return nil, fmt.Errorf("[%d]: %w", index, err)
			}
		}
		return &cffi.InboundValue{ValueType: stated, Value: &cffi.InboundValue_ListValue{
			ListValue: &cffi.InboundListValue{Values: values},
		}}, nil
	case *cffi.BamlOutboundValue_MapValue:
		mapValue := item.MapValue
		if mapValue == nil {
			return nil, errors.New("thrown map value is empty")
		}
		stated, err := annotationStatement(MapBAMLType(
			BAMLType{value: mapValue.GetKeyType()},
			BAMLType{value: mapValue.GetValueType()},
		).value)
		if err != nil {
			return nil, fmt.Errorf("map: %w", err)
		}
		entries, err := outboundEntriesToInbound(mapValue.GetEntries(), transaction)
		if err != nil {
			return nil, err
		}
		return &cffi.InboundValue{ValueType: stated, Value: &cffi.InboundValue_MapValue{
			MapValue: &cffi.InboundMapValue{Entries: entries},
		}}, nil
	case *cffi.BamlOutboundValue_ClassValue:
		class := item.ClassValue
		if class == nil {
			return nil, errors.New("thrown class value is empty")
		}
		stated, err := annotationStatement(&cffi.BamlTy{Ty: &cffi.BamlTy_ClassTy{ClassTy: &cffi.BamlTyClass{
			Head:     &cffi.BamlTyClass_Name{Name: class.GetName()},
			TypeArgs: class.GetTypeArgs(),
		}}})
		if err != nil {
			return nil, fmt.Errorf("class %q: %w", class.GetName(), err)
		}
		fields, err := outboundEntriesToInbound(class.GetFields(), transaction)
		if err != nil {
			return nil, fmt.Errorf("class %q%w", class.GetName(), err)
		}
		return &cffi.InboundValue{ValueType: stated, Value: &cffi.InboundValue_ClassValue{
			ClassValue: &cffi.InboundClassValue{Fields: fields},
		}}, nil
	case *cffi.BamlOutboundValue_HandleValue:
		return handleToInbound(item.HandleValue, transaction)
	case *cffi.BamlOutboundValue_MediaValue:
		return &cffi.InboundValue{Value: &cffi.InboundValue_MediaValue{MediaValue: item.MediaValue}}, nil
	case *cffi.BamlOutboundValue_PromptAstValue:
		return &cffi.InboundValue{Value: &cffi.InboundValue_PromptAstValue{PromptAstValue: item.PromptAstValue}}, nil
	case *cffi.BamlOutboundValue_TyValue:
		if err := validateBAMLType(item.TyValue, typeStated); err != nil {
			return nil, fmt.Errorf("type value: %w", err)
		}
		stated, err := repointLiveHeads(item.TyValue, func(key uint64) (uint64, error) {
			return cloneForInput(transaction, key, "runtime declaration")
		})
		if err != nil {
			return nil, err
		}
		return &cffi.InboundValue{Value: &cffi.InboundValue_TyValue{TyValue: stated}}, nil
	default:
		return nil, fmt.Errorf("thrown value arm %T has no inbound form", item)
	}
}

func outboundEntriesToInbound(entries []*cffi.BamlOutboundMapEntry, transaction *inputTransaction) ([]*cffi.InboundMapEntry, error) {
	encoded := make([]*cffi.InboundMapEntry, len(entries))
	for index, entry := range entries {
		value, err := outboundToInbound(entry.GetValue(), transaction)
		if err != nil {
			return nil, fmt.Errorf(".%s: %w", entry.GetKey(), err)
		}
		encoded[index] = &cffi.InboundMapEntry{
			Key:   &cffi.InboundMapEntry_StringKey{StringKey: entry.GetKey()},
			Value: value,
		}
	}
	return encoded, nil
}

func literalToInbound(literal *cffi.BamlLiteralValue) (*cffi.InboundValue, error) {
	switch item := literal.GetLiteral().(type) {
	case *cffi.BamlLiteralValue_StringValue:
		return &cffi.InboundValue{Value: &cffi.InboundValue_StringValue{StringValue: item.StringValue}}, nil
	case *cffi.BamlLiteralValue_IntValue:
		return &cffi.InboundValue{Value: &cffi.InboundValue_IntValue{IntValue: item.IntValue}}, nil
	case *cffi.BamlLiteralValue_BigintValue:
		return &cffi.InboundValue{Value: &cffi.InboundValue_BigintValue{BigintValue: item.BigintValue}}, nil
	case *cffi.BamlLiteralValue_FloatValue:
		float, err := strconv.ParseFloat(item.FloatValue, 64)
		if err != nil {
			return nil, fmt.Errorf("thrown float literal %q: %w", item.FloatValue, err)
		}
		return &cffi.InboundValue{Value: &cffi.InboundValue_FloatValue{FloatValue: float}}, nil
	case *cffi.BamlLiteralValue_BoolValue:
		return &cffi.InboundValue{Value: &cffi.InboundValue_BoolValue{BoolValue: item.BoolValue}}, nil
	default:
		return nil, fmt.Errorf("thrown literal arm %T has no inbound form", item)
	}
}

// handleToInbound sends a handle back as it arrived: an engine handle as a
// fresh clone of its own kind, a host key as the same key (a host key is one
// registration however often it occurs), and a host callable stating the
// function type the engine annotated it with.
func handleToInbound(handle *cffi.BamlOutboundHandle, transaction *inputTransaction) (*cffi.InboundValue, error) {
	if handle == nil || handle.GetKey() == 0 {
		return nil, errors.New("thrown handle is empty")
	}
	switch handle.GetHandleType() {
	case cffi.BamlHandleType_HOST_VALUE_OPAQUE:
		return hostValueInput(handle.GetKey(), handle.GetHandleType()), nil
	case cffi.BamlHandleType_HOST_VALUE_CALLABLE:
		stated, err := annotationStatement(handle.GetTy())
		if err != nil {
			return nil, fmt.Errorf("host callable: %w", err)
		}
		value := hostValueInput(handle.GetKey(), handle.GetHandleType())
		value.ValueType = stated
		return value, nil
	default:
		cloned, err := cloneForInput(transaction, handle.GetKey(), "thrown value's")
		if err != nil {
			return nil, err
		}
		return &cffi.InboundValue{Value: &cffi.InboundValue_Handle{Handle: &cffi.BamlHandle{
			Key: cloned, HandleType: handle.GetHandleType(),
		}}}, nil
	}
}

// annotationStatement states an annotation the engine sent beside a value.
// An annotation transfers no reference, so one naming a run-time declaration
// (by label or by a row Go holds no reference to) cannot be stated back.
func annotationStatement(ty *cffi.BamlTy) (*cffi.BamlTy, error) {
	if ty == nil {
		return nil, errors.New("the engine sent no type annotation")
	}
	if err := validateBAMLType(ty, typeStated); err != nil {
		return nil, fmt.Errorf("cannot state the type the engine annotated: %w", err)
	}
	if len(liveHeadKeys(ty)) != 0 {
		return nil, errors.New("cannot state the type the engine annotated: it names a run-time declaration")
	}
	return ty, nil
}

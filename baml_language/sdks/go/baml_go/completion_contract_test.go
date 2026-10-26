package baml_go

import (
	"context"
	"errors"
	"reflect"
	"slices"
	"testing"
	"time"

	"github.com/boundaryml/baml-go/internal/cffi"
	"google.golang.org/protobuf/proto"
)

// completionOutcome names the arm a completion carries.
func completionOutcome(t *testing.T, completion *cffi.HostCallCompletion) string {
	t.Helper()
	switch completion.GetOutcome().(type) {
	case *cffi.HostCallCompletion_Returned:
		return "returned"
	case *cffi.HostCallCompletion_Thrown:
		return "thrown"
	case *cffi.HostCallCompletion_Raised:
		return "raised"
	case *cffi.HostCallCompletion_ContractViolation:
		return "contract_violation"
	case *cffi.HostCallCompletion_BridgeFailure:
		return "bridge_failure"
	default:
		return "none"
	}
}

// runCallback runs callable as a dispatched callback whose context is ctx
// and returns the completion it sent.
func runCallback(t *testing.T, ctx context.Context, callable HostCallableFunc, args ...*cffi.BamlToHostArg) *cffi.HostCallCompletion {
	t.Helper()
	previous := completeNativeHostCall
	completions := make(chan []byte, 2)
	completeNativeHostCall = func(_ uint32, payload []byte) { completions <- payload }
	t.Cleanup(func() { completeNativeHostCall = previous })

	key := registerHostValue(hostValue{callable: callable})
	t.Cleanup(func() { unregisterHostValue(key) })
	frame := &hostInvocationFrame{ctx: ctx, finish: func() {}}
	dispatchHostCall(key, 1, marshalHostDispatch(t, args...), frame)
	var payload []byte
	select {
	case payload = <-completions:
	case <-time.After(5 * time.Second):
		t.Fatal("callback did not complete")
	}
	cleanupCompletedHostError(payload)
	select {
	case <-completions:
		t.Fatal("callback completed more than once")
	default:
	}
	var completion cffi.HostCallCompletion
	if err := proto.Unmarshal(payload, &completion); err != nil {
		t.Fatalf("completion payload: %v", err)
	}
	return &completion
}

func nestedListInput(levels int) Input {
	value := Int64(1)
	itemType := PrimitiveBAMLType(IntType)
	for range levels {
		value = listInput([]Input{value}, &itemType)
		itemType = ListBAMLType(itemType)
	}
	return value
}

func TestCallbackCompletionIsClassifiedByWhatFailed(t *testing.T) {
	cancelled, cancel := context.WithCancel(context.Background())
	cancel()
	thrownError := &BamlError{Kind: "error", ClassName: "user.MyError", payload: &errorPayload{
		value: &cffi.BamlOutboundValue{Value: &cffi.BamlOutboundValue_ClassValue{ClassValue: &cffi.BamlValueClass{
			Name: "user.MyError",
			Fields: []*cffi.BamlOutboundMapEntry{{
				Key: "code", Value: &cffi.BamlOutboundValue{Value: &cffi.BamlOutboundValue_IntValue{IntValue: 42}},
			}},
		}}},
	}}
	tests := []struct {
		name     string
		ctx      context.Context
		callable HostCallableFunc
		want     string
	}{
		{"a value that fits", context.Background(), func(HostCallArguments) (Input, error) {
			return String("ok"), nil
		}, "returned"},
		{"an adapter arity fault", context.Background(), func(arguments HostCallArguments) (Input, error) {
			return InvalidInput("arity"), HostCallableArityError(1, arguments.RequiredCount())
		}, "bridge_failure"},
		{"an argument the adapter could not decode", context.Background(), func(HostCallArguments) (Input, error) {
			return InvalidInput("argument"), HostCallableArgumentError(0, errors.New("not an int"))
		}, "bridge_failure"},
		{"an unknown named argument", context.Background(), func(arguments HostCallArguments) (Input, error) {
			return InvalidInput("optional"), HostCallableOptionalCountError(0, 1)
		}, "bridge_failure"},
		{"an invocation after cancellation", cancelled, func(HostCallArguments) (Input, error) {
			t.Error("a cancelled callback ran")
			return String("late"), nil
		}, "bridge_failure"},
		{"a result nested deeper than the wire", context.Background(), func(HostCallArguments) (Input, error) {
			return nestedListInput(49), nil
		}, "bridge_failure"},
		{"a result at the wire's bound", context.Background(), func(HostCallArguments) (Input, error) {
			return nestedListInput(48), nil
		}, "returned"},
		{"a result that does not encode as its type", context.Background(), func(HostCallArguments) (Input, error) {
			return JSON(func() {}), nil
		}, "contract_violation"},
		{"a return from a callback declared never", context.Background(), func(HostCallArguments) (Input, error) {
			return InvalidInput("never"), HostCallableReturnedFromNever()
		}, "contract_violation"},
		{"a value thrown as the declared type", context.Background(), func(HostCallArguments) (Input, error) {
			return HostCallableError(Throw("declared"), String)
		}, "thrown"},
		{"a value thrown as another Go type", context.Background(), func(HostCallArguments) (Input, error) {
			return HostCallableError(Throw(7), String)
		}, "contract_violation"},
		{"a BAML error returned as it arrived", context.Background(), func(HostCallArguments) (Input, error) {
			return Input{}, thrownError
		}, "thrown"},
		{"a BAML error panicked with as it arrived", context.Background(), func(HostCallArguments) (Input, error) {
			panic(thrownError)
		}, "thrown"},
		{"a BamlError Go built itself", context.Background(), func(HostCallArguments) (Input, error) {
			return Input{}, &BamlError{Kind: "error", ClassName: "user.MyError"}
		}, "raised"},
		{"a Go error", context.Background(), func(HostCallArguments) (Input, error) {
			return Input{}, errors.New("host failure")
		}, "raised"},
		{"a wrapped BAML error", context.Background(), func(HostCallArguments) (Input, error) {
			return Input{}, errors.Join(errors.New("context"), thrownError)
		}, "raised"},
		{"a Go panic", context.Background(), func(HostCallArguments) (Input, error) {
			panic("host panic")
		}, "raised"},
	}
	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			completion := runCallback(t, test.ctx, test.callable)
			if got := completionOutcome(t, completion); got != test.want {
				t.Fatalf("outcome = %s (%v), want %s", got, completion, test.want)
			}
		})
	}
}

func TestForwardedBamlErrorIsThrownAsItArrived(t *testing.T) {
	previousClone := cloneInboundHandle
	cloneInboundHandle = func(key uint64) (uint64, error) { return key + 1000, nil }
	t.Cleanup(func() { cloneInboundHandle = previousClone })

	itemType := &cffi.BamlTy{Ty: &cffi.BamlTy_Primitive{Primitive: &cffi.BamlTyPrimitive{Kind: cffi.BamlTyPrimitiveKind_BAML_TY_PRIMITIVE_STRING}}}
	received := &BamlError{Kind: "panic", ClassName: "user.Boxed", payload: &errorPayload{
		value: &cffi.BamlOutboundValue{Value: &cffi.BamlOutboundValue_UnionVariantValue{UnionVariantValue: &cffi.BamlValueUnionVariant{
			Value: &cffi.BamlOutboundValue{Value: &cffi.BamlOutboundValue_ClassValue{ClassValue: &cffi.BamlValueClass{
				Name: "user.Boxed",
				Fields: []*cffi.BamlOutboundMapEntry{
					{Key: "items", Value: &cffi.BamlOutboundValue{Value: &cffi.BamlOutboundValue_ListValue{ListValue: &cffi.BamlValueList{
						ItemType: itemType,
						Items: []*cffi.BamlOutboundValue{
							{Value: &cffi.BamlOutboundValue_StringValue{StringValue: "a"}},
						},
					}}}},
					{Key: "state", Value: &cffi.BamlOutboundValue{Value: &cffi.BamlOutboundValue_HandleValue{HandleValue: &cffi.BamlOutboundHandle{
						Key: 7, HandleType: cffi.BamlHandleType_UNTAGGED_RUST_DATA,
					}}}},
					{Key: "origin", Value: &cffi.BamlOutboundValue{Value: &cffi.BamlOutboundValue_HandleValue{HandleValue: &cffi.BamlOutboundHandle{
						Key: 9, HandleType: cffi.BamlHandleType_HOST_VALUE_OPAQUE,
					}}}},
				},
			}}},
		}}},
	}}
	completion := runCallback(t, context.Background(), func(HostCallArguments) (Input, error) {
		return Input{}, received
	})
	thrown := completion.GetThrown()
	if thrown == nil {
		t.Fatalf("completion = %v, want thrown", completion)
	}
	if name := thrown.GetValueType().GetClassTy().GetName(); name != "user.Boxed" {
		t.Fatalf("thrown statement = %v, want the class the engine annotated", thrown.GetValueType())
	}
	fields := thrown.GetClassValue().GetFields()
	items := fields[0].GetValue()
	if items.GetValueType().GetList().GetItem().GetPrimitive().GetKind() != cffi.BamlTyPrimitiveKind_BAML_TY_PRIMITIVE_STRING ||
		items.GetListValue().GetValues()[0].GetStringValue() != "a" {
		t.Fatalf("thrown list = %v, want string[] stated beside it", items)
	}
	if state := fields[1].GetValue().GetHandle(); state.GetKey() != 1007 || state.GetHandleType() != cffi.BamlHandleType_UNTAGGED_RUST_DATA {
		t.Fatalf("thrown engine handle = %v, want a fresh clone of its own kind", state)
	}
	if origin := fields[2].GetValue().GetHandle(); origin.GetKey() != 9 || origin.GetHandleType() != cffi.BamlHandleType_HOST_VALUE_OPAQUE {
		t.Fatalf("thrown host key = %v, want the key as it arrived", origin)
	}
}

func TestForwardedValueNamingAnUnnameableDeclarationIsABridgeFailure(t *testing.T) {
	received := &BamlError{Kind: "error", ClassName: "user.Holder", payload: &errorPayload{
		value: &cffi.BamlOutboundValue{Value: &cffi.BamlOutboundValue_ListValue{ListValue: &cffi.BamlValueList{
			ItemType: &cffi.BamlTy{Ty: &cffi.BamlTy_ClassTy{ClassTy: &cffi.BamlTyClass{
				Head: &cffi.BamlTyClass_AnonymousLabel{AnonymousLabel: "RuntimeRow"},
			}}},
		}}},
	}}
	completion := runCallback(t, context.Background(), func(HostCallArguments) (Input, error) {
		return Input{}, received
	})
	if got := completionOutcome(t, completion); got != "bridge_failure" {
		t.Fatalf("outcome = %s (%v), want bridge_failure", got, completion)
	}
}

func TestResultHandleCloneFailureIsABridgeFailureAndReleasesEarlierClones(t *testing.T) {
	previousClone, previousRelease := cloneInboundHandle, releaseInboundHandle
	var released []uint64
	cloneInboundHandle = func(key uint64) (uint64, error) {
		if key == 2 {
			return 0, errors.New("handle table full")
		}
		return key + 100, nil
	}
	releaseInboundHandle = func(key uint64) { released = append(released, key) }
	t.Cleanup(func() { cloneInboundHandle, releaseInboundHandle = previousClone, previousRelease })

	first := RustType{handle: &rustTypeHandle{key: 1}}
	second := RustType{handle: &rustTypeHandle{key: 2}}
	completion := runCallback(t, context.Background(), func(HostCallArguments) (Input, error) {
		itemType := RustTypeBAMLType()
		return listInput([]Input{RustTypeInput(first), RustTypeInput(second)}, &itemType), nil
	})
	if got := completionOutcome(t, completion); got != "bridge_failure" {
		t.Fatalf("outcome = %s (%v), want bridge_failure", got, completion)
	}
	if !slices.Equal(released, []uint64{101}) {
		t.Fatalf("released clones = %v, want the first clone released", released)
	}
}

// Every arm of the messages the measure walks is accounted for, so a schema
// change cannot silently measure a new arm as flat.
func TestWireDepthMeasuresEveryArm(t *testing.T) {
	for _, message := range []proto.Message{&cffi.InboundValue{}, &cffi.BamlTy{}} {
		descriptor := message.ProtoReflect().Descriptor()
		oneofs := descriptor.Oneofs()
		for index := range oneofs.Len() {
			fields := oneofs.Get(index).Fields()
			for field := range fields.Len() {
				arm := fields.Get(field)
				populated := message.ProtoReflect().New()
				populated.Set(arm, populated.NewField(arm))
				if !populated.Has(arm) {
					t.Fatalf("%s.%s was not populated", descriptor.Name(), arm.Name())
				}
				func() {
					defer func() {
						if recovered := recover(); recovered != nil {
							t.Errorf("%s.%s: %v", descriptor.Name(), arm.Name(), recovered)
						}
					}()
					switch value := populated.Interface().(type) {
					case *cffi.InboundValue:
						inboundValueDepth(value)
					case *cffi.BamlTy:
						bamlTyDepth(value)
					default:
						t.Fatalf("unexpected message %T", value)
					}
				}()
			}
		}
	}
}

func TestWireDepthMatchesTheDecodersAccounting(t *testing.T) {
	// A typed list of n levels nests 2n+2 messages; a chain of n classes
	// ending in a scalar, 3n.
	for levels, want := range map[int]int{1: 4, 2: 6, 48: 98} {
		encoded, err := nestedListInput(levels).encodeValue(&inputTransaction{})
		if err != nil {
			t.Fatal(err)
		}
		if got := inboundValueDepth(encoded); got != want {
			t.Fatalf("%d-level typed list nests %d messages, want %d", levels, got, want)
		}
	}
	class := &cffi.InboundValue{Value: &cffi.InboundValue_IntValue{IntValue: 1}}
	for range 3 {
		class = &cffi.InboundValue{
			ValueType: &cffi.BamlTy{Ty: &cffi.BamlTy_ClassTy{ClassTy: &cffi.BamlTyClass{Head: &cffi.BamlTyClass_Name{Name: "Node"}}}},
			Value: &cffi.InboundValue_ClassValue{ClassValue: &cffi.InboundClassValue{Fields: []*cffi.InboundMapEntry{{
				Key: &cffi.InboundMapEntry_StringKey{StringKey: "next"}, Value: class,
			}}}},
		}
	}
	if got := inboundValueDepth(class); got != 9 {
		t.Fatalf("3-level class chain nests %d messages, want 9", got)
	}
	if !reflect.DeepEqual(checkInboundDepth(class, wireNestingLimit-9), error(nil)) {
		t.Fatal("a value exactly at the limit was refused")
	}
	var refused *wireDepthError
	if err := checkInboundDepth(class, wireNestingLimit-8); !errors.As(err, &refused) || !isBridgeFault(err) {
		t.Fatalf("a value one message past the limit = %v, want a bridge fault", err)
	}
}

// A result refused for its depth never reaches the engine, so the clones its
// encoding made are released rather than leaked.
func TestResultTooDeepForTheWireReleasesItsClones(t *testing.T) {
	previousClone, previousRelease := cloneInboundHandle, releaseInboundHandle
	var released []uint64
	cloneInboundHandle = func(key uint64) (uint64, error) { return key + 100, nil }
	releaseInboundHandle = func(key uint64) { released = append(released, key) }
	t.Cleanup(func() { cloneInboundHandle, releaseInboundHandle = previousClone, previousRelease })

	leaf := RustType{handle: &rustTypeHandle{key: 5}}
	completion := runCallback(t, context.Background(), func(HostCallArguments) (Input, error) {
		value := RustTypeInput(leaf)
		itemType := RustTypeBAMLType()
		for range 49 {
			value = listInput([]Input{value}, &itemType)
			itemType = ListBAMLType(itemType)
		}
		return value, nil
	})
	if got := completionOutcome(t, completion); got != "bridge_failure" {
		t.Fatalf("outcome = %s (%v), want bridge_failure", got, completion)
	}
	if !slices.Equal(released, []uint64{105}) {
		t.Fatalf("released clones = %v, want the leaf's clone released", released)
	}
}

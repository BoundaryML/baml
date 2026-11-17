package baml_go

import (
	"fmt"
	"reflect"
	"sync"

	"github.com/boundaryml/baml-go/internal/cffi"
)


func TupleBAMLType(items ...BAMLType) BAMLType {
	encoded := make([]*cffi.BamlTy, len(items))
	for index, item := range items {
		encoded[index] = item.value
	}
	return composeBAMLType(&cffi.BamlTy{Ty: &cffi.BamlTy_Tuple{Tuple: &cffi.BamlTyTuple{Items: encoded}}}, items...)
}

func tupleArity(value BAMLType) (int, bool) {
	tuple, ok := value.value.GetTy().(*cffi.BamlTy_Tuple)
	if !ok || tuple.Tuple == nil {
		return 0, false
	}
	return len(tuple.Tuple.Items), true
}

func TupleInput(tupleType BAMLType, items ...Input) Input {
	if tupleType.err != nil {
		return Input{err: fmt.Errorf("tuple type: %w", tupleType.err)}
	}
	arity, ok := tupleArity(tupleType)
	if !ok {
		return Input{err: fmt.Errorf("tuple input states a non-tuple type")}
	}
	if len(items) != arity {
		return Input{err: fmt.Errorf("tuple of arity %d has %d slots", arity, len(items))}
	}
	prepare := func(transaction *inputTransaction) (*cffi.InboundValue, error) {
		stated, err := tupleType.stated(transaction)
		if err != nil {
			return nil, fmt.Errorf("tuple type: %w", err)
		}
		encoded := make([]*cffi.InboundValue, 0, len(items))
		for index, item := range items {
			value, err := item.encodeValue(transaction)
			if err != nil {
				return nil, fmt.Errorf("tuple slot %d: %w", index, err)
			}
			encoded = append(encoded, value)
		}
		return &cffi.InboundValue{
			ValueType: stated,
			Value: &cffi.InboundValue_ListValue{
				ListValue: &cffi.InboundListValue{Values: encoded},
			},
		}, nil
	}
	if inputsAreStatic(items) && !tupleType.transfers() {
		value, err := prepare(nil)
		return Input{value: value, err: err}
	}
	return Input{deferred: &inputEncoder{encode: prepare}}
}

func TupleListEncoder(tupleType BAMLType) func([]any) Input {
	return func(values []any) Input {
		items := make([]Input, len(values))
		for index, value := range values {
			items[index] = Any(value)
		}
		return TupleInput(tupleType, items...)
	}
}

func (value Value) Tuple(arity int) ([]Value, error) {
	unwrapped, err := value.unwrapUnionVariants()
	if err != nil {
		return nil, err
	}
	item, ok := unwrapped.value.Value.(*cffi.BamlOutboundValue_TupleValue)
	if !ok || item.TupleValue == nil {
		return nil, fmt.Errorf("expected BAML tuple, got %T", unwrapped.value.Value)
	}
	if len(item.TupleValue.Items) != arity {
		return nil, fmt.Errorf("BAML tuple has %d slots, expected %d", len(item.TupleValue.Items), arity)
	}
	slots := make([]Value, arity)
	for index, encoded := range item.TupleValue.Items {
		if encoded == nil {
			return nil, fmt.Errorf("BAML tuple slot %d is empty", index)
		}
		slots[index] = Value{value: encoded, owner: value.owner}
	}
	return slots, nil
}

func DecodeTupleSlot[T any](slots []Value, index int, decode func(Value) (T, error)) (T, error) {
	decoded, err := decode(slots[index])
	if err != nil {
		var zero T
		return zero, fmt.Errorf("BAML tuple slot %d: %w", index, err)
	}
	return decoded, nil
}

func TupleListDecoder(arity int) func(Value) ([]any, error) {
	return func(value Value) ([]any, error) {
		slots, err := value.Tuple(arity)
		if err != nil {
			return nil, err
		}
		decoded := make([]any, arity)
		for index, slot := range slots {
			if decoded[index], err = decodeDynamicValue(slot, fmt.Sprintf("value[%d]", index), 1); err != nil {
				return nil, err
			}
		}
		return decoded, nil
	}
}

type tupleRegistration struct {
	tupleType *cffi.BamlTy
	goType    reflect.Type
	decode    func(Value) (any, error)
}

var tupleRegistry struct {
	sync.RWMutex
	byArity map[int][]tupleRegistration
}

func RegisterTuple[T interface {
	DynamicType
	DynamicDecoder
}]() {
	var zero T
	tupleType := zero.BAMLType()
	if tupleType.err != nil {
		return
	}
	arity, ok := tupleArity(tupleType)
	if !ok {
		return
	}
	tupleRegistry.Lock()
	defer tupleRegistry.Unlock()
	for _, existing := range tupleRegistry.byArity[arity] {
		if equalBAMLType(existing.tupleType, tupleType.value, false) {
			return
		}
	}
	if tupleRegistry.byArity == nil {
		tupleRegistry.byArity = make(map[int][]tupleRegistration)
	}
	tupleRegistry.byArity[arity] = append(tupleRegistry.byArity[arity], tupleRegistration{
		tupleType: tupleType.value,
		goType:    reflect.TypeFor[T](),
		decode:    zero.BAMLDecode,
	})
}

func registeredTuple(tupleType *cffi.BamlTy) (tupleRegistration, bool) {
	tuple, ok := tupleType.GetTy().(*cffi.BamlTy_Tuple)
	if !ok || tuple.Tuple == nil {
		return tupleRegistration{}, false
	}
	tupleRegistry.RLock()
	defer tupleRegistry.RUnlock()
	for _, registration := range tupleRegistry.byArity[len(tuple.Tuple.Items)] {
		if equalBAMLType(registration.tupleType, tupleType, false) {
			return registration, true
		}
	}
	return tupleRegistration{}, false
}

func tupleJoinGoType(slots []*cffi.BamlTy) reflect.Type {
	if len(slots) == 0 {
		return nil
	}
	for _, slot := range slots[1:] {
		if !equalBAMLType(slot, slots[0], false) {
			return nil
		}
	}
	switch item := slots[0].GetTy().(type) {
	case *cffi.BamlTy_Primitive:
		switch item.Primitive.GetKind() {
		case cffi.BamlTyPrimitiveKind_BAML_TY_PRIMITIVE_STRING:
			return reflect.TypeFor[string]()
		case cffi.BamlTyPrimitiveKind_BAML_TY_PRIMITIVE_INT:
			return reflect.TypeFor[int64]()
		case cffi.BamlTyPrimitiveKind_BAML_TY_PRIMITIVE_FLOAT:
			return reflect.TypeFor[float64]()
		case cffi.BamlTyPrimitiveKind_BAML_TY_PRIMITIVE_BOOL:
			return reflect.TypeFor[bool]()
		}
	case *cffi.BamlTy_Tuple:
		if registration, ok := registeredTuple(slots[0]); ok {
			return registration.goType
		}
	}
	return nil
}

func decodeDynamicTuple(value Value, tuple *cffi.BamlValueTuple, path string, depth int) (any, error) {
	carried := tuple.GetTupleType()
	if registration, ok := registeredTuple(carried); ok {
		decoded, err := registration.decode(value)
		if err != nil {
			return nil, fmt.Errorf("%s: %w", path, err)
		}
		return decoded, nil
	}
	decoded := make([]any, len(tuple.Items))
	for index, encoded := range tuple.Items {
		if encoded == nil {
			return nil, fmt.Errorf("%s[%d]: BAML value is empty", path, index)
		}
		var err error
		decoded[index], err = decodeDynamicValue(
			Value{value: encoded, owner: value.owner},
			fmt.Sprintf("%s[%d]", path, index),
			depth+1,
		)
		if err != nil {
			return nil, err
		}
	}
	slots := carried.GetTuple().GetItems()
	if len(slots) != len(decoded) {
		return decoded, nil
	}
	join := tupleJoinGoType(slots)
	if join == nil {
		return decoded, nil
	}
	list := reflect.MakeSlice(reflect.SliceOf(join), len(decoded), len(decoded))
	for index, item := range decoded {
		if item == nil || reflect.TypeOf(item) != join {
			return decoded, nil
		}
		list.Index(index).Set(reflect.ValueOf(item))
	}
	return list.Interface(), nil
}

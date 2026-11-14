package baml_go

import (
	"fmt"
	"reflect"
	"sync"

	"github.com/boundaryml/baml-go/internal/cffi"
)

// Go has no tuple type, so a BAML tuple crosses as one of two shapes:
//
//   - A generated named struct (`Tuple2_IntString`) whose fields are its
//     slots, in order. It is the representation of every tuple whose slot
//     types Go can spell. It encodes itself through TupleInput, states its
//     tuple type through BAMLType, and decodes through Value.Tuple and
//     DecodeTupleSlot, so it is found by tuple identity wherever the
//     generated codecs, DecodeAs or a dynamic union's candidates look.
//   - A `[]any` of exactly the tuple's slots, in order, when no named struct
//     represents the tuple (a slot mentions a type parameter, so its join is
//     dynamic). It still crosses as the tuple its declaration states:
//     TupleListEncoder states the declared tuple type and TupleListDecoder
//     checks the arity.
//
// Where no declaration names the tuple (an `any` position, a thrown value, a
// slot over a type parameter), the decoder finds the named struct by the
// tuple type the value carries: every generated named struct registers
// itself with RegisterTuple under its tuple type. A tuple whose type no
// struct registered decodes as a list of the join of its carried slot types:
// a typed slice when that join is one Go type (`[]int64` for `(int, int)`),
// else `[]any`, Go's carrier for a union. Either list holds exactly the
// tuple's slots, in order.
//
// Inbound, a tuple is a list stating its tuple type: the engine checks the
// arity and every slot against that statement. Outbound, it is the wire's
// tuple value, whose arity the decoder checks; a plain BAML list never
// decodes as a tuple.

// TupleBAMLType describes the fixed-arity BAML tuple `(items[0], items[1], ...)`.
func TupleBAMLType(items ...BAMLType) BAMLType {
	encoded := make([]*cffi.BamlTy, len(items))
	for index, item := range items {
		encoded[index] = item.value
	}
	return composeBAMLType(&cffi.BamlTy{Ty: &cffi.BamlTy_Tuple{Tuple: &cffi.BamlTyTuple{Items: encoded}}}, items...)
}

// tupleArity is the arity of a tuple descriptor, or false when value does not
// describe a tuple.
func tupleArity(value BAMLType) (int, bool) {
	tuple, ok := value.value.GetTy().(*cffi.BamlTy_Tuple)
	if !ok || tuple.Tuple == nil {
		return 0, false
	}
	return len(tuple.Tuple.Items), true
}

// TupleInput encodes the BAML tuple of type tupleType whose slots are items,
// in order. It is a list stating its tuple type, which is how a tuple crosses
// inbound; each item states or implies its own type, and the engine holds it
// to its slot.
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

// TupleListEncoder encodes a `[]any` standing in for the BAML tuple of type
// tupleType: exactly one element per slot, in order, each encoded as a
// dynamic value. The list states tupleType, so it crosses as that tuple, not
// as a list.
func TupleListEncoder(tupleType BAMLType) func([]any) Input {
	return func(values []any) Input {
		items := make([]Input, len(values))
		for index, value := range values {
			items[index] = Any(value)
		}
		return TupleInput(tupleType, items...)
	}
}

// Tuple returns the slots of a BAML tuple of exactly arity slots, in order.
// A BAML list is never a tuple, whatever its length.
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

// DecodeTupleSlot decodes slot index of slots, which Value.Tuple returned,
// with the converter of that slot's type.
func DecodeTupleSlot[T any](slots []Value, index int, decode func(Value) (T, error)) (T, error) {
	decoded, err := decode(slots[index])
	if err != nil {
		var zero T
		return zero, fmt.Errorf("BAML tuple slot %d: %w", index, err)
	}
	return decoded, nil
}

// TupleListDecoder decodes a BAML tuple of exactly arity slots into a `[]any`
// holding each slot's dynamic Go value, in order.
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

// tupleRegistration is a generated named tuple struct registered under its
// tuple type.
type tupleRegistration struct {
	tupleType *cffi.BamlTy
	goType    reflect.Type
	decode    func(Value) (any, error)
}

var tupleRegistry struct {
	sync.RWMutex
	// byArity buckets registrations by arity; a bucket is searched by tuple
	// type equality, which compares slots in order.
	byArity map[int][]tupleRegistration
}

// RegisterTuple registers the generated named tuple struct T under the tuple
// type it reifies, so a tuple whose declared type is out of reach (an `any`
// position, a thrown value, a slot over a type parameter) still decodes as T
// by the tuple type it carries. Generated packages call it from init; a
// second registration of one tuple type keeps the first.
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

// registeredTuple is the registration of tupleType, if a generated struct
// registered it.
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

// tupleJoinGoType is the Go type the dynamic decoder gives every value of
// the join of slots, or nil when that join is not one Go type: slots of one
// primitive scalar type, or of one registered tuple type.
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

// decodeDynamicTuple decodes a tuple no declaration names, by the tuple type
// it carries: its registered named struct, else the list of the join of its
// carried slot types, else `[]any`. The slots are decoded once, whichever
// representation holds them.
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

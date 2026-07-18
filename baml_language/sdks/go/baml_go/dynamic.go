package baml_go

import (
	"fmt"
	"math"
	"math/big"
	"reflect"
	"sort"

	"github.com/boundaryml/baml-go/internal/cffi"
)

// InputMarshaler is implemented by generated closed-union values. It only
// answers whether a Go value can cross the ABI; BAML remains authoritative for
// semantic assignability to a function's declared parameter type.
type InputMarshaler interface {
	BAMLInput() Input
}

// DynamicType is implemented by generated closed unions. It lets reflective
// container encoding preserve a precise element type even for empty values.
type DynamicType interface {
	BAMLType() BAMLType
}

// DynamicClass is implemented by generated BAML classes so a class carried in
// an `any` union preserves its nominal wire identity.
type DynamicClass interface {
	BAMLClassName() string
}

// DynamicEnum is implemented by generated BAML enums so a value carried in an
// `any` union preserves its nominal wire identity and closed variant set.
type DynamicEnum interface {
	BAMLEnumName() string
	BAMLEnumVariants() []string
}

// InvalidInput constructs a failed input without panicking. Generated union
// encoders use it for an uninitialized zero-value union.
func InvalidInput(message string) Input {
	return Input{err: fmt.Errorf("%s", message)}
}

// SelectedUnionInput attaches the exact generated arm identity to a union
// payload. The BAML runtime validates both the descriptor and the payload
// against the callable's canonical parameter type.
func SelectedUnionInput(payload Input, unionType, selectedType BAMLType) Input {
	if payload.err != nil {
		return payload
	}
	if payload.value == nil && payload.deferred == nil {
		return InvalidInput("selected union payload is uninitialized")
	}
	if unionType.value == nil || selectedType.value == nil {
		return InvalidInput("selected union type metadata is uninitialized")
	}
	prepare := func(transaction *inputTransaction) (*cffi.InboundValue, error) {
		value, err := payload.encodeValue(transaction)
		if err != nil {
			return nil, fmt.Errorf("selected union payload: %w", err)
		}
		return &cffi.InboundValue{Value: &cffi.InboundValue_UnionVariantValue{
			UnionVariantValue: &cffi.InboundUnionVariantValue{
				SelfType:     unionType.value,
				SelectedType: selectedType.value,
				Value:        value,
			},
		}}, nil
	}
	if payload.deferred == nil {
		value, err := prepare(nil)
		return Input{value: value, err: err}
	}
	return Input{deferred: &inputEncoder{encode: prepare}}
}

// Any converts an ordinary dynamic Go value into the generic ABI value tree.
// It intentionally performs no BAML assignability checking; once serialized,
// the BAML runtime validates the value against the callable's canonical type.
func Any(value any) Input {
	return encodeAny(reflect.ValueOf(value), make(map[visit]bool), "value", 0)
}

type visit struct {
	typ reflect.Type
	ptr uintptr
}

func anyList(values []Input, itemType *BAMLType) Input {
	return listInput(values, itemType)
}

func anyMap(values map[string]Input, valueType *BAMLType) Input {
	keys := make([]string, 0, len(values))
	for key := range values {
		keys = append(keys, key)
	}
	sort.Strings(keys)
	inputs := make([]Input, 0, len(keys))
	for _, key := range keys {
		inputs = append(inputs, values[key])
	}
	return mapInput(keys, inputs, valueType)
}

func reflectedBAMLType(typ reflect.Type) (BAMLType, bool) {
	if typ == reflect.TypeOf(BAMLType{}) {
		return MetaTypeBAMLType(), true
	}
	bigIntPointer := reflect.TypeOf((*big.Int)(nil))
	if typ == bigIntPointer {
		return PrimitiveBAMLType(BigintType), true
	}
	if typ.Kind() == reflect.Pointer {
		inner, ok := reflectedBAMLType(typ.Elem())
		if !ok {
			return BAMLType{}, false
		}
		return OptionalBAMLType(inner), true
	}
	if typ.Implements(reflect.TypeOf((*DynamicType)(nil)).Elem()) {
		return reflect.Zero(typ).Interface().(DynamicType).BAMLType(), true
	}
	if typ.Implements(reflect.TypeOf((*DynamicEnum)(nil)).Elem()) {
		enum := reflect.Zero(typ).Interface().(DynamicEnum)
		return EnumBAMLType(enum.BAMLEnumName()), true
	}
	if typ.Implements(reflect.TypeOf((*DynamicClass)(nil)).Elem()) {
		class := reflect.Zero(typ).Interface().(DynamicClass)
		return ClassBAMLType(class.BAMLClassName()), true
	}

	switch typ.Kind() {
	case reflect.String:
		return PrimitiveBAMLType(StringType), true
	case reflect.Bool:
		return PrimitiveBAMLType(BoolType), true
	case reflect.Int, reflect.Int8, reflect.Int16, reflect.Int32, reflect.Int64,
		reflect.Uint, reflect.Uint8, reflect.Uint16, reflect.Uint32, reflect.Uint64, reflect.Uintptr:
		return PrimitiveBAMLType(IntType), true
	case reflect.Float32, reflect.Float64:
		return PrimitiveBAMLType(FloatType), true
	case reflect.Slice:
		if typ.Elem().Kind() == reflect.Uint8 {
			return PrimitiveBAMLType(BytesType), true
		}
		fallthrough
	case reflect.Array:
		inner, ok := reflectedBAMLType(typ.Elem())
		if !ok {
			return BAMLType{}, false
		}
		return ListBAMLType(inner), true
	case reflect.Map:
		if typ.Key().Kind() != reflect.String {
			return BAMLType{}, false
		}
		value, ok := reflectedBAMLType(typ.Elem())
		if !ok {
			return BAMLType{}, false
		}
		return MapBAMLType(PrimitiveBAMLType(StringType), value), true
	default:
		return BAMLType{}, false
	}
}

func encodeAny(value reflect.Value, active map[visit]bool, path string, depth int) Input {
	if depth > 256 {
		return InvalidInput(fmt.Sprintf("%s: dynamic Go value nesting exceeds 256 levels (possible cycle)", path))
	}
	if !value.IsValid() {
		return NullInput(Null{})
	}
	if value.Kind() == reflect.Interface {
		if value.IsNil() {
			return NullInput(Null{})
		}
		return encodeAny(value.Elem(), active, path, depth+1)
	}
	// A nil pointer is BAML null regardless of the methods its static type
	// implements. Checking this before InputMarshaler, *big.Int, class, or enum
	// dispatch prevents calls through nil receivers and the resulting panic.
	if value.Kind() == reflect.Pointer && value.IsNil() {
		return NullInput(Null{})
	}
	if value.CanInterface() {
		if reflectedType, ok := value.Interface().(BAMLType); ok {
			return Type(reflectedType)
		}
		if marshaler, ok := value.Interface().(InputMarshaler); ok {
			return marshaler.BAMLInput()
		}
		if integer, ok := value.Interface().(*big.Int); ok {
			return BigInt(integer)
		}
		if enum, ok := value.Interface().(DynamicEnum); ok {
			enumValue := value
			for enumValue.Kind() == reflect.Pointer {
				if enumValue.IsNil() {
					return NullInput(Null{})
				}
				enumValue = enumValue.Elem()
			}
			if enumValue.Kind() != reflect.String {
				return InvalidInput(fmt.Sprintf("%s: generated BAML enum has non-string Go type %s", path, value.Type()))
			}
			return Enum(enum.BAMLEnumName(), enumValue.String(), enum.BAMLEnumVariants()...)
		}
	}

	switch value.Kind() {
	case reflect.Pointer:
		if value.IsNil() {
			return NullInput(Null{})
		}
		key := visit{typ: value.Type(), ptr: value.Pointer()}
		if active[key] {
			return InvalidInput(fmt.Sprintf("%s: cyclic Go value cannot cross the BAML ABI", path))
		}
		active[key] = true
		defer delete(active, key)
		return encodeAny(value.Elem(), active, path, depth+1)
	case reflect.String:
		return String(value.String())
	case reflect.Bool:
		return Bool(value.Bool())
	case reflect.Int, reflect.Int8, reflect.Int16, reflect.Int32, reflect.Int64:
		return Int64(value.Int())
	case reflect.Uint, reflect.Uint8, reflect.Uint16, reflect.Uint32, reflect.Uint64, reflect.Uintptr:
		integer := value.Uint()
		if integer > math.MaxInt64 {
			return InvalidInput(fmt.Sprintf("%s: unsigned integer %d overflows BAML int", path, integer))
		}
		return Int64(int64(integer))
	case reflect.Float32, reflect.Float64:
		return Float64(value.Float())
	case reflect.Slice:
		if value.Type().Elem().Kind() == reflect.Uint8 {
			bytes := make([]byte, value.Len())
			reflect.Copy(reflect.ValueOf(bytes), value)
			return Uint8Array(bytes)
		}
		fallthrough
	case reflect.Array:
		items := make([]Input, value.Len())
		for index := range items {
			items[index] = encodeAny(value.Index(index), active, fmt.Sprintf("%s[%d]", path, index), depth+1)
			if items[index].err != nil {
				return items[index]
			}
		}
		var itemType *BAMLType
		if inferred, ok := reflectedBAMLType(value.Type().Elem()); ok {
			itemType = &inferred
		}
		return anyList(items, itemType)
	case reflect.Map:
		if value.Type().Key().Kind() != reflect.String {
			return InvalidInput(fmt.Sprintf("%s: BAML maps require string keys, got %s", path, value.Type().Key()))
		}
		entries := make(map[string]Input, value.Len())
		iterator := value.MapRange()
		for iterator.Next() {
			key := iterator.Key().String()
			encoded := encodeAny(iterator.Value(), active, fmt.Sprintf("%s[%q]", path, key), depth+1)
			if encoded.err != nil {
				return encoded
			}
			entries[key] = encoded
		}
		var valueType *BAMLType
		if inferred, ok := reflectedBAMLType(value.Type().Elem()); ok {
			valueType = &inferred
		}
		return anyMap(entries, valueType)
	case reflect.Struct:
		if !value.CanInterface() {
			break
		}
		class, ok := value.Interface().(DynamicClass)
		if !ok {
			break
		}
		fields := make(map[string]Input)
		for index := 0; index < value.NumField(); index++ {
			fieldInfo := value.Type().Field(index)
			wireName := fieldInfo.Tag.Get("baml")
			if wireName == "" || wireName == "-" {
				continue
			}
			field := encodeAny(value.Field(index), active, path+"."+fieldInfo.Name, depth+1)
			if field.err != nil {
				return field
			}
			fields[wireName] = field
		}
		return Class(class.BAMLClassName(), fields)
	}
	return InvalidInput(fmt.Sprintf("%s: unsupported Go value of type %s", path, value.Type()))
}

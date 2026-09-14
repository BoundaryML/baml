package baml_go

import (
	"fmt"
	"math"
	"math/big"
	"reflect"
	"sort"
	"strconv"

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

// DynamicDecoder is implemented by generated generic classes. A zero value is
// used as a type-directed factory when a generic function returns an arbitrary
// T whose concrete Go shape is known only at instantiation time.
type DynamicDecoder interface {
	BAMLDecode(Value) (any, error)
}

// DynamicFieldDecoder is implemented by generated generic classes whose Go
// field shape contains an erased union. It restores the concrete candidate
// decoders that reflection alone cannot recover from an `any` field.
type DynamicFieldDecoder interface {
	BAMLDecodeField(string, Value) (decoded any, handled bool, err error)
}

// DynamicDecodeCandidate pairs one exact selected-arm descriptor with its Go
// decoder. Generated generic classes use these to decode unions containing a
// type parameter without reducing nominal classes or enums to raw maps/strings.
type DynamicDecodeCandidate struct {
	Type   BAMLType
	Decode func(Value) (any, error)
}

// DynamicDecodeAs adapts DecodeAs to the erased candidate decoder signature.
func DynamicDecodeAs[T any](value Value) (any, error) { return DecodeAs[T](value) }

// DynamicUnionDecoder constructs a decoder for one erased union position.
// The BAML runtime's selected descriptor remains authoritative.
func DynamicUnionDecoder(candidates ...DynamicDecodeCandidate) func(Value) (any, error) {
	return func(value Value) (any, error) {
		isNull, err := value.IsNull()
		if err != nil {
			return nil, err
		}
		if isNull {
			return nil, nil
		}
		selected, payload, err := value.UnionVariant()
		if err != nil {
			return nil, err
		}
		for index, candidate := range candidates {
			if err := validateBAMLTypeValue(candidate.Type); err != nil {
				return nil, fmt.Errorf("dynamic union candidate %d: %w", index, err)
			}
			if candidate.Decode == nil {
				return nil, fmt.Errorf("dynamic union candidate %d has no decoder", index)
			}
			if selected.MatchesUnionArm(candidate.Type) {
				return candidate.Decode(payload)
			}
		}
		return nil, UnexpectedUnionVariant("dynamic generic union", selected)
	}
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

// SelectedUnionInput projects a generated union to its selected payload and
// attaches that arm's exact node type. The enclosing union stays in the BAML
// callable's declared type; inbound values never carry a union envelope.
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
		annotated := &cffi.InboundValue{
			ValueType: value.ValueType,
			Value:     value.Value,
		}
		if annotated.ValueType == nil {
			annotated.ValueType = selectedType.value
		}
		return annotated, nil
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

// AnyEncoder is the generic-function form of Any. Its type parameter lets Go
// preserve element types when it is composed with ListEncoder/MapEncoder.
func AnyEncoder[T any](value T) Input { return Any(value) }

// GeneratedClassInput encodes one generated class without redispatching to
// its BAMLInput method. Generated methods call this helper so Any can preserve
// normal InputMarshaler precedence without recursing back into the method.
func GeneratedClassInput(value any) Input {
	reflected := reflect.ValueOf(value)
	for reflected.IsValid() && reflected.Kind() == reflect.Pointer {
		if reflected.IsNil() {
			return NullInput(Null{})
		}
		reflected = reflected.Elem()
	}
	if !reflected.IsValid() || reflected.Kind() != reflect.Struct {
		return InvalidInput(fmt.Sprintf("generated BAML class has Go type %T, expected struct", value))
	}
	return encodeGeneratedClass(reflected, make(map[visit]bool), "value", 0)
}

// TypeOf returns the canonical BAML descriptor for T. Unsupported host-only
// Go shapes return an uninitialized descriptor; CallWithTypeArgs reports that
// as a normal input error before invoking the native runtime.
//
// In particular, Go's `any` erases the nominal identity of BAML's canonical
// `baml.json.json` alias. JSON remains native `any` in non-generic generated
// APIs, but a generic API instantiated as `any` cannot reify that type argument
// and fails normally instead of guessing an unsound descriptor.
func TypeOf[T any]() BAMLType {
	typeOf, _ := reflectedBAMLType(reflect.TypeFor[T]())
	return typeOf
}

// DefinitionOf promotes a known sparse token to the portable definition form
// used by reflected handles. Unsupported Go-only shapes remain invalid.
func DefinitionOf[T any]() BAMLType {
	value := TypeOf[T]()
	definition, err := value.definitionCopy()
	if err != nil {
		return BAMLType{err: fmt.Errorf("unsupported Go type token %s", reflect.TypeFor[T]())}
	}
	return BAMLType{definition: definition}
}

// DecodeAs decodes a BAML value into the concrete Go instantiation T.
func DecodeAs[T any](value Value) (T, error) {
	var zero T
	decoded, err := decodeReflected(value, reflect.TypeFor[T](), "value", 0)
	if err != nil {
		return zero, err
	}
	if !decoded.IsValid() {
		return zero, fmt.Errorf("decoded BAML value is invalid")
	}
	result, ok := decoded.Interface().(T)
	if !ok {
		return zero, fmt.Errorf("decoded BAML value has Go type %s, expected %s", decoded.Type(), reflect.TypeFor[T]())
	}
	return result, nil
}

// DecodeGeneratedClass is the implementation target for generated zero-value
// class decode hooks. It bypasses the hook at the outer level and recursively
// applies type-directed decoding to every tagged field.
func DecodeGeneratedClass[T any](value Value) (T, error) {
	var zero T
	decoded, err := decodeGeneratedClass(value, reflect.TypeFor[T](), "value", 0)
	if err != nil {
		return zero, err
	}
	return decoded.Interface().(T), nil
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
	// Marker interfaces describe concrete generated/runtime values. Their zero
	// interface value has no dynamic receiver and must not be invoked.
	if typ.Kind() == reflect.Interface {
		return BAMLType{}, false
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
			// Generated classes must stay inside this traversal so pointer cycle
			// and depth state is not reset by their public BAMLInput hook.
			if _, generatedClass := value.Interface().(DynamicClass); !generatedClass {
				return marshaler.BAMLInput()
			}
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
		if _, ok := value.Interface().(DynamicClass); !ok {
			break
		}
		return encodeGeneratedClass(value, active, path, depth)
	}
	return InvalidInput(fmt.Sprintf("%s: unsupported Go value of type %s", path, value.Type()))
}

func encodeGeneratedClass(value reflect.Value, active map[visit]bool, path string, depth int) Input {
	class := value.Interface().(DynamicClass)
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
	descriptor, ok := reflectedBAMLType(value.Type())
	if !ok || descriptor.value.GetClassTy() == nil {
		return InvalidInput(fmt.Sprintf("%s: generated BAML class %s has no class descriptor", path, value.Type()))
	}
	typeArgs := descriptor.value.GetClassTy().GetTypeArgs()
	publicTypeArgs := make([]BAMLType, len(typeArgs))
	for index, typeArg := range typeArgs {
		publicTypeArgs[index] = BAMLType{value: typeArg}
	}
	return ClassWithTypeArgs(class.BAMLClassName(), publicTypeArgs, fields)
}

func decodeReflected(value Value, typ reflect.Type, path string, depth int) (reflect.Value, error) {
	if depth > 256 {
		return reflect.Value{}, fmt.Errorf("%s: BAML value nesting exceeds 256 levels", path)
	}
	if typ == reflect.TypeOf(BAMLType{}) {
		decoded, err := value.Type()
		return reflect.ValueOf(decoded), err
	}
	if typ == reflect.TypeOf((*big.Int)(nil)) {
		decoded, err := value.BigInt()
		return reflect.ValueOf(decoded), err
	}
	if typ == reflect.TypeOf(Null{}) {
		decoded, err := value.Null()
		return reflect.ValueOf(decoded), err
	}
	if typ == reflect.TypeOf(Image{}) {
		decoded, err := value.Image()
		return reflect.ValueOf(decoded), err
	}
	if typ == reflect.TypeOf(Audio{}) {
		decoded, err := value.Audio()
		return reflect.ValueOf(decoded), err
	}
	if typ == reflect.TypeOf(Video{}) {
		decoded, err := value.Video()
		return reflect.ValueOf(decoded), err
	}
	if typ == reflect.TypeOf(Pdf{}) {
		decoded, err := value.Pdf()
		return reflect.ValueOf(decoded), err
	}
	if typ == reflect.TypeOf(RustType{}) {
		decoded, err := value.RustType()
		return reflect.ValueOf(decoded), err
	}
	if typ.Kind() == reflect.Pointer {
		isNull, err := value.IsNull()
		if err != nil {
			return reflect.Value{}, err
		}
		if isNull {
			return reflect.Zero(typ), nil
		}
		inner, err := decodeReflected(value, typ.Elem(), path, depth+1)
		if err != nil {
			return reflect.Value{}, err
		}
		pointer := reflect.New(typ.Elem())
		pointer.Elem().Set(inner)
		return pointer, nil
	}
	if typ.Kind() == reflect.Interface {
		if typ.NumMethod() != 0 {
			return reflect.Value{}, fmt.Errorf("%s: unsupported generic Go output interface %s", path, typ)
		}
		decoded, err := decodeDynamicValue(value, path, depth)
		if err != nil {
			return reflect.Value{}, err
		}
		if decoded == nil {
			return reflect.Zero(typ), nil
		}
		return reflect.ValueOf(decoded), nil
	}
	if typ.Implements(reflect.TypeOf((*DynamicDecoder)(nil)).Elem()) {
		decoded, err := reflect.Zero(typ).Interface().(DynamicDecoder).BAMLDecode(value)
		if err != nil {
			return reflect.Value{}, err
		}
		result := reflect.ValueOf(decoded)
		if !result.IsValid() || !result.Type().AssignableTo(typ) {
			return reflect.Value{}, fmt.Errorf("%s: generated decoder returned %T for %s", path, decoded, typ)
		}
		return result, nil
	}
	if typ.Implements(reflect.TypeOf((*DynamicEnum)(nil)).Elem()) {
		enum := reflect.Zero(typ).Interface().(DynamicEnum)
		decoded, err := value.Enum(enum.BAMLEnumName(), enum.BAMLEnumVariants()...)
		if err != nil {
			return reflect.Value{}, err
		}
		result := reflect.New(typ).Elem()
		result.SetString(decoded)
		return result, nil
	}

	switch typ.Kind() {
	case reflect.String:
		decoded, err := value.String()
		if err != nil {
			return reflect.Value{}, err
		}
		result := reflect.New(typ).Elem()
		result.SetString(decoded)
		return result, nil
	case reflect.Bool:
		decoded, err := value.Bool()
		if err != nil {
			return reflect.Value{}, err
		}
		result := reflect.New(typ).Elem()
		result.SetBool(decoded)
		return result, nil
	case reflect.Int, reflect.Int8, reflect.Int16, reflect.Int32, reflect.Int64:
		decoded, err := value.Int64()
		if err != nil {
			return reflect.Value{}, err
		}
		result := reflect.New(typ).Elem()
		if result.OverflowInt(decoded) {
			return reflect.Value{}, fmt.Errorf("%s: BAML int %d overflows %s", path, decoded, typ)
		}
		result.SetInt(decoded)
		return result, nil
	case reflect.Uint, reflect.Uint8, reflect.Uint16, reflect.Uint32, reflect.Uint64, reflect.Uintptr:
		decoded, err := value.Int64()
		if err != nil {
			return reflect.Value{}, err
		}
		if decoded < 0 {
			return reflect.Value{}, fmt.Errorf("%s: negative BAML int %d cannot decode as %s", path, decoded, typ)
		}
		result := reflect.New(typ).Elem()
		if result.OverflowUint(uint64(decoded)) {
			return reflect.Value{}, fmt.Errorf("%s: BAML int %d overflows %s", path, decoded, typ)
		}
		result.SetUint(uint64(decoded))
		return result, nil
	case reflect.Float32, reflect.Float64:
		decoded, err := value.Float64()
		if err != nil {
			return reflect.Value{}, err
		}
		result := reflect.New(typ).Elem()
		if result.OverflowFloat(decoded) {
			return reflect.Value{}, fmt.Errorf("%s: BAML float %g overflows %s", path, decoded, typ)
		}
		result.SetFloat(decoded)
		return result, nil
	case reflect.Slice:
		if typ.Elem().Kind() == reflect.Uint8 {
			decoded, err := value.Uint8Array()
			if err != nil {
				return reflect.Value{}, err
			}
			result := reflect.MakeSlice(typ, len(decoded), len(decoded))
			reflect.Copy(result, reflect.ValueOf(decoded))
			return result, nil
		}
		return decodeReflectedList(value, typ, path, depth)
	case reflect.Array:
		return decodeReflectedList(value, typ, path, depth)
	case reflect.Map:
		return decodeReflectedMap(value, typ, path, depth)
	case reflect.Struct:
		return decodeGeneratedClass(value, typ, path, depth)
	default:
		return reflect.Value{}, fmt.Errorf("%s: unsupported generic Go output type %s", path, typ)
	}
}

func decodeDynamicValue(value Value, path string, depth int) (any, error) {
	if depth > 256 {
		return nil, fmt.Errorf("%s: BAML value nesting exceeds 256 levels", path)
	}
	unwrapped, err := value.unwrapUnionVariants()
	if err != nil {
		return nil, err
	}
	if unwrapped.value == nil {
		return nil, fmt.Errorf("%s: BAML value is uninitialized", path)
	}
	value = unwrapped
	switch item := value.value.Value.(type) {
	case nil:
		return nil, nil
	case *cffi.BamlOutboundValue_NullValue:
		if item.NullValue == nil {
			return nil, fmt.Errorf("%s: BAML null payload is empty", path)
		}
		return nil, nil
	case *cffi.BamlOutboundValue_StringValue:
		return item.StringValue, nil
	case *cffi.BamlOutboundValue_IntValue:
		return item.IntValue, nil
	case *cffi.BamlOutboundValue_BigintValue:
		decoded, ok := new(big.Int).SetString(item.BigintValue, 16)
		if !ok {
			return nil, fmt.Errorf("%s: BAML returned invalid bigint %q", path, item.BigintValue)
		}
		return decoded, nil
	case *cffi.BamlOutboundValue_FloatValue:
		return item.FloatValue, nil
	case *cffi.BamlOutboundValue_BoolValue:
		return item.BoolValue, nil
	case *cffi.BamlOutboundValue_Uint8ArrayValue:
		return append([]byte(nil), item.Uint8ArrayValue...), nil
	case *cffi.BamlOutboundValue_TyDefValue:
		return value.Type()
	case *cffi.BamlOutboundValue_LiteralValue:
		if item.LiteralValue == nil {
			return nil, fmt.Errorf("%s: BAML literal payload is empty", path)
		}
		switch literal := item.LiteralValue.Literal.(type) {
		case *cffi.BamlLiteralValue_StringValue:
			return literal.StringValue, nil
		case *cffi.BamlLiteralValue_IntValue:
			return literal.IntValue, nil
		case *cffi.BamlLiteralValue_BigintValue:
			decoded, ok := new(big.Int).SetString(literal.BigintValue, 16)
			if !ok {
				return nil, fmt.Errorf("%s: BAML returned invalid bigint literal %q", path, literal.BigintValue)
			}
			return decoded, nil
		case *cffi.BamlLiteralValue_FloatValue:
			decoded, err := strconv.ParseFloat(literal.FloatValue, 64)
			if err != nil {
				return nil, fmt.Errorf("%s: BAML returned invalid float literal %q: %w", path, literal.FloatValue, err)
			}
			return decoded, nil
		case *cffi.BamlLiteralValue_BoolValue:
			return literal.BoolValue, nil
		default:
			return nil, fmt.Errorf("%s: unsupported BAML literal %T", path, item.LiteralValue.Literal)
		}
	case *cffi.BamlOutboundValue_ListValue:
		if item.ListValue == nil {
			return nil, fmt.Errorf("%s: BAML list payload is empty", path)
		}
		decoded := make([]any, len(item.ListValue.Items))
		for index, encoded := range item.ListValue.Items {
			if encoded == nil {
				return nil, fmt.Errorf("%s[%d]: BAML value is empty", path, index)
			}
			decoded[index], err = decodeDynamicValue(
				Value{value: encoded, owner: value.owner},
				fmt.Sprintf("%s[%d]", path, index),
				depth+1,
			)
			if err != nil {
				return nil, err
			}
		}
		return decoded, nil
	case *cffi.BamlOutboundValue_MapValue:
		if item.MapValue == nil {
			return nil, fmt.Errorf("%s: BAML map payload is empty", path)
		}
		decoded := make(map[string]any, len(item.MapValue.Entries))
		for index, entry := range item.MapValue.Entries {
			if entry == nil || entry.Value == nil {
				return nil, fmt.Errorf("%s: BAML map entry %d is empty", path, index)
			}
			if _, duplicate := decoded[entry.Key]; duplicate {
				return nil, fmt.Errorf("%s: duplicate BAML map key %q", path, entry.Key)
			}
			decoded[entry.Key], err = decodeDynamicValue(
				Value{value: entry.Value, owner: value.owner},
				fmt.Sprintf("%s[%q]", path, entry.Key),
				depth+1,
			)
			if err != nil {
				return nil, err
			}
		}
		return decoded, nil
	case *cffi.BamlOutboundValue_EnumValue:
		if item.EnumValue == nil {
			return nil, fmt.Errorf("%s: BAML enum payload is empty", path)
		}
		return item.EnumValue.Value, nil
	case *cffi.BamlOutboundValue_ClassValue:
		if item.ClassValue == nil {
			return nil, fmt.Errorf("%s: BAML class payload is empty", path)
		}
		switch item.ClassValue.Name {
		case mediaClassName(mediaKindImage):
			return value.Image()
		case mediaClassName(mediaKindAudio):
			return value.Audio()
		case mediaClassName(mediaKindVideo):
			return value.Video()
		case mediaClassName(mediaKindPDF):
			return value.Pdf()
		}
		decoded := make(map[string]any, len(item.ClassValue.Fields))
		for index, field := range item.ClassValue.Fields {
			if field == nil || field.Value == nil {
				return nil, fmt.Errorf("%s: BAML class field %d is empty", path, index)
			}
			if _, duplicate := decoded[field.Key]; duplicate {
				return nil, fmt.Errorf("%s: duplicate BAML class field %q", path, field.Key)
			}
			decoded[field.Key], err = decodeDynamicValue(
				Value{value: field.Value, owner: value.owner},
				path+"."+field.Key,
				depth+1,
			)
			if err != nil {
				return nil, err
			}
		}
		return decoded, nil
	case *cffi.BamlOutboundValue_MediaValue:
		if item.MediaValue == nil {
			return nil, fmt.Errorf("%s: BAML media payload is empty", path)
		}
		switch item.MediaValue.Media {
		case cffi.MediaTypeEnum_IMAGE:
			return value.Image()
		case cffi.MediaTypeEnum_AUDIO:
			return value.Audio()
		case cffi.MediaTypeEnum_VIDEO:
			return value.Video()
		case cffi.MediaTypeEnum_PDF:
			return value.Pdf()
		default:
			return nil, fmt.Errorf("%s: unsupported BAML media kind %s", path, item.MediaValue.Media)
		}
	case *cffi.BamlOutboundValue_HandleValue:
		return value.RustType()
	case *cffi.BamlOutboundValue_TyValue:
		return value.Type()
	default:
		return nil, fmt.Errorf("%s: unsupported dynamic BAML output %T", path, value.value.Value)
	}
}

func decodeReflectedList(value Value, typ reflect.Type, path string, depth int) (reflect.Value, error) {
	unwrapped, err := value.unwrapUnionVariants()
	if err != nil {
		return reflect.Value{}, err
	}
	item, ok := unwrapped.value.Value.(*cffi.BamlOutboundValue_ListValue)
	if !ok || item.ListValue == nil {
		return reflect.Value{}, fmt.Errorf("%s: expected BAML list, got %T", path, unwrapped.value.Value)
	}
	if typ.Kind() == reflect.Array && len(item.ListValue.Items) != typ.Len() {
		return reflect.Value{}, fmt.Errorf("%s: BAML list has length %d, expected %d", path, len(item.ListValue.Items), typ.Len())
	}
	var result reflect.Value
	if typ.Kind() == reflect.Array {
		result = reflect.New(typ).Elem()
	} else {
		result = reflect.MakeSlice(typ, len(item.ListValue.Items), len(item.ListValue.Items))
	}
	for index, encoded := range item.ListValue.Items {
		if encoded == nil {
			return reflect.Value{}, fmt.Errorf("%s[%d]: empty BAML value", path, index)
		}
		decoded, err := decodeReflected(Value{value: encoded, owner: value.owner}, typ.Elem(), fmt.Sprintf("%s[%d]", path, index), depth+1)
		if err != nil {
			return reflect.Value{}, err
		}
		result.Index(index).Set(decoded)
	}
	return result, nil
}

func decodeReflectedMap(value Value, typ reflect.Type, path string, depth int) (reflect.Value, error) {
	if typ.Key().Kind() != reflect.String {
		return reflect.Value{}, fmt.Errorf("%s: BAML maps require string keys, got %s", path, typ.Key())
	}
	unwrapped, err := value.unwrapUnionVariants()
	if err != nil {
		return reflect.Value{}, err
	}
	item, ok := unwrapped.value.Value.(*cffi.BamlOutboundValue_MapValue)
	if !ok || item.MapValue == nil {
		return reflect.Value{}, fmt.Errorf("%s: expected BAML map, got %T", path, unwrapped.value.Value)
	}
	result := reflect.MakeMapWithSize(typ, len(item.MapValue.Entries))
	seen := make(map[string]struct{}, len(item.MapValue.Entries))
	for index, entry := range item.MapValue.Entries {
		if entry == nil || entry.Value == nil {
			return reflect.Value{}, fmt.Errorf("%s: empty BAML map entry %d", path, index)
		}
		if _, duplicate := seen[entry.Key]; duplicate {
			return reflect.Value{}, fmt.Errorf("%s: duplicate BAML map key %q", path, entry.Key)
		}
		seen[entry.Key] = struct{}{}
		decoded, err := decodeReflected(Value{value: entry.Value, owner: value.owner}, typ.Elem(), fmt.Sprintf("%s[%q]", path, entry.Key), depth+1)
		if err != nil {
			return reflect.Value{}, err
		}
		key := reflect.New(typ.Key()).Elem()
		key.SetString(entry.Key)
		result.SetMapIndex(key, decoded)
	}
	return result, nil
}

func decodeGeneratedClass(value Value, typ reflect.Type, path string, depth int) (reflect.Value, error) {
	if typ.Kind() != reflect.Struct || !typ.Implements(reflect.TypeOf((*DynamicClass)(nil)).Elem()) {
		return reflect.Value{}, fmt.Errorf("%s: %s is not a generated BAML class", path, typ)
	}
	class := reflect.Zero(typ).Interface().(DynamicClass)
	descriptor, ok := reflectedBAMLType(typ)
	if !ok || descriptor.value.GetClassTy() == nil {
		return reflect.Value{}, fmt.Errorf("%s: %s has no BAML class descriptor", path, typ)
	}
	rawArgs := descriptor.value.GetClassTy().GetTypeArgs()
	typeArgs := make([]BAMLType, len(rawArgs))
	for index, arg := range rawArgs {
		typeArgs[index] = BAMLType{value: arg}
	}
	classValue, err := value.ClassWithTypeArgs(class.BAMLClassName(), typeArgs)
	if err != nil {
		return reflect.Value{}, err
	}
	result := reflect.New(typ).Elem()
	fieldDecoder, hasFieldDecoder := reflect.Zero(typ).Interface().(DynamicFieldDecoder)
	for index := 0; index < typ.NumField(); index++ {
		fieldInfo := typ.Field(index)
		wireName := fieldInfo.Tag.Get("baml")
		if wireName == "" || wireName == "-" {
			continue
		}
		field, err := classValue.Field(wireName)
		if err != nil {
			return reflect.Value{}, fmt.Errorf("%s.%s: %w", path, fieldInfo.Name, err)
		}
		if hasFieldDecoder {
			decoded, handled, err := fieldDecoder.BAMLDecodeField(wireName, field)
			if err != nil {
				return reflect.Value{}, fmt.Errorf("%s.%s: %w", path, fieldInfo.Name, err)
			}
			if handled {
				if decoded == nil {
					result.Field(index).Set(reflect.Zero(fieldInfo.Type))
					continue
				}
				reflected := reflect.ValueOf(decoded)
				if !reflected.Type().AssignableTo(fieldInfo.Type) {
					return reflect.Value{}, fmt.Errorf("%s.%s: generated field decoder returned %s, expected %s", path, fieldInfo.Name, reflected.Type(), fieldInfo.Type)
				}
				result.Field(index).Set(reflected)
				continue
			}
		}
		decoded, err := decodeReflected(field, fieldInfo.Type, path+"."+fieldInfo.Name, depth+1)
		if err != nil {
			return reflect.Value{}, err
		}
		result.Field(index).Set(decoded)
	}
	return result, nil
}

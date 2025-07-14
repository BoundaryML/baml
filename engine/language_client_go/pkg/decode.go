package baml

import (
	"encoding/json"
	"fmt"
	"os"
	"reflect"

	"github.com/boundaryml/baml/engine/language_client_go/pkg/cffi"
)

type TypeMap map[string]reflect.Type

// debugLog prints debug information if BAML_INTERNAL_LOG=trace is set
func debugLog(format string, args ...interface{}) {
	if os.Getenv("BAML_INTERNAL_LOG") == "trace" {
		fmt.Printf(format, args...)
		if format[len(format)-1] != '\n' {
			fmt.Println()
		}
	}
}

type BamlClassDeserializer interface {
	Decode(holder *cffi.CFFIValueClass)
}

type BamlEnumDeserializer interface {
	Decode(holder *cffi.CFFIValueEnum)
}

type BamlUnionDeserializer interface {
	Decode(holder *cffi.CFFIValueUnionVariant)
}

type DynamicClass struct {
	Name   string
	Fields map[string]any
}

func (d *DynamicClass) Decode(holder *cffi.CFFIValueClass) {
	typeName := holder.Name
	if typeName == nil {
		panic(fmt.Sprintf("DynamicClass.Decode: typeName is nil, holder=%+v", holder))
	}
	d.Name = string(typeName.Name)
	if len(holder.Fields) > 0 {
		panic(fmt.Sprintf("DynamicClass.Decode: unexpected fields present, holder.Fields=%+v", holder.Fields))
	}
	fieldCount := len(holder.DynamicFields)
	d.Fields = make(map[string]any, fieldCount)
	for i := 0; i < fieldCount; i++ {
		field := holder.DynamicFields[i]
		if field == nil {
			panic(fmt.Sprintf("DynamicClass.Decode: field[%d] is nil, holder.DynamicFields=%+v", i, holder.DynamicFields))
		}
		key := field.Key
		valueHolder := field.Value
		d.Fields[key] = Decode(valueHolder).Interface()
	}
}

type DynamicEnum struct {
	Name  string
	Value string
}

func (d *DynamicEnum) Decode(holder *cffi.CFFIValueEnum) {
	if holder.Name == nil {
		panic(fmt.Sprintf("DynamicEnum.Decode: holder.Name is nil, holder=%+v", holder))
	}
	d.Name = string(holder.Name.Name)
	d.Value = string(holder.Value)
}

func decodeListValue(valueList *cffi.CFFIValueList) reflect.Value {
	debugLog("decodeListValue: valueList=%+v\n", valueList)
	if valueList == nil {
		panic("decodeListValue: valueList is nil")
	}

	elementType := valueList.ValueType
	goElementType := convertFieldTypeToGoType(elementType)

	// // check if goValueType is a pointer
	// isValueTypePtr := goElementType.Kind() == reflect.Ptr

	// // is union, enum, or class (i.e. implements BamlClassDeserializer, BamlEnumDeserializer, BamlUnionDeserializer)
	// isValueTypeUnion := elementType.GetUnionVariantType() != nil
	// isValueTypeEnum := elementType.GetEnumType() != nil
	// isValueTypeClass := elementType.GetClassType() != nil
	// isValueCustomType := isValueTypeUnion || isValueTypeEnum || isValueTypeClass
	// castToPointer := (isValueTypePtr && !isValueCustomType)

	length := len(valueList.Values)
	values := reflect.MakeSlice(reflect.SliceOf(goElementType), length, length)

	for i, v := range valueList.Values {
		decodedValue := Decode(v)
		values.Index(i).Set(decodedValue)
	}

	return values
}

func decodeMapValue(valueMap *cffi.CFFIValueMap) reflect.Value {
	if valueMap == nil {
		panic("decodeMapValue: valueMap is nil")
	}
	keyType := valueMap.KeyType
	valueType := valueMap.ValueType
	goKeyType := convertFieldTypeToGoType(keyType)
	goValueType := convertFieldTypeToGoType(valueType)

	// // check if goValueType is a pointer
	// isValueTypePtr := goValueType.Kind() == reflect.Ptr

	// // is union, enum, or class (i.e. implements BamlClassDeserializer, BamlEnumDeserializer, BamlUnionDeserializer)
	// isValueTypeUnion := valueType.GetUnionVariantType() != nil
	// isValueTypeEnum := valueType.GetEnumType() != nil
	// isValueTypeClass := valueType.GetClassType() != nil
	// isValueCustomType := isValueTypeUnion || isValueTypeEnum || isValueTypeClass
	// castToPointer := (isValueTypePtr && !isValueCustomType)
	// debugLog("castToPointer %v\nisValueTypePtr %v\nisValueCustomType %v\ngoValueType %v\ngoKeyType %v\n", castToPointer, isValueTypePtr, isValueCustomType, goValueType, goKeyType)

	values := reflect.MakeMap(reflect.MapOf(goKeyType, goValueType))

	for _, entry := range valueMap.Entries {
		key := entry.Key
		value := entry.Value
		decodedValue := Decode(value)
		debugLog("key: %v, decodedValue: %v\n", key, decodedValue)
		values.SetMapIndex(reflect.ValueOf(key), decodedValue)
	}
	return values
}

func decodeStreamingStateValue(valueStreamingState *cffi.CFFIValueStreamingState) StreamState[any] {
	if valueStreamingState == nil {
		panic("error decoding value")
	}
	value := valueStreamingState.Value
	return StreamState[any]{
		Value: Decode(value).Interface(),
		State: decodeStreamStateType(valueStreamingState.State),
	}
}

type BamlDecoder interface {
	BamlDecode(decodedMap map[string]any)
}

func decodeClassValue(valueClass *cffi.CFFIValueClass) reflect.Value {
	if valueClass == nil {
		panic("decodeClassValue: valueClass is nil")
	}

	typeName := valueClass.Name
	namespace := typeName.Namespace.String()
	className := string(typeName.Name)
	found, ok := typeMap[namespace+"."+className]
	if !ok {
		// This is a fully dynamic class, so we need to decode it as a map
		dynamicClass := DynamicClass{
			Name: className,
		}
		dynamicClass.Decode(valueClass)
		return reflect.ValueOf(dynamicClass)
	}

	cls := reflect.New(found)
	as_interface := cls.Interface().(BamlClassDeserializer)
	as_interface.Decode(valueClass)
	return cls.Elem()
}

func decodeEnumValue(valueEnum *cffi.CFFIValueEnum) reflect.Value {
	if valueEnum == nil {
		panic("decodeEnumValue: valueEnum is nil")
	}

	typeName := valueEnum.Name
	namespace := typeName.Namespace.String()
	enumName := string(typeName.Name)
	found, ok := typeMap[namespace+"."+enumName]
	if !ok {
		dynamicEnum := DynamicEnum{Name: enumName, Value: string(valueEnum.Value)}
		return reflect.ValueOf(dynamicEnum)
	}
	enum := reflect.New(found)
	as_interface := enum.Interface().(BamlEnumDeserializer)
	as_interface.Decode(valueEnum)
	return enum.Elem()
}

func decodeUnionValue(valueUnion *cffi.CFFIValueUnionVariant) reflect.Value {
	if valueUnion == nil {
		panic("decodeUnionValue: valueUnion is nil")
	}

	typeName := valueUnion.Name
	namespace := typeName.Namespace.String()
	unionName := string(typeName.Name)

	var isOptionalPattern bool = false

	// Check if this is an optional pattern (T | null)
	if len(valueUnion.FieldTypes) == 2 {
		hasNull := false
		hasNonNull := false
		for _, ft := range valueUnion.FieldTypes {
			if ft.GetNullType() != nil {
				hasNull = true
			} else {
				hasNonNull = true
			}
		}
		isOptionalPattern = hasNull && hasNonNull
	}

	// For optional patterns (T | null), decode the inner value directly
	// These shouldn't be looked up as union types
	if isOptionalPattern {
		value := valueUnion.Value
		return Decode(value)
	}

	found, ok := typeMap[namespace+"."+unionName]
	if !ok {
		// This is a fully dynamic union, so we
		// decode the value as the value and drop
		// union type information
		value := valueUnion.Value
		return Decode(value)
	}

	union := reflect.New(found)
	as_interface := union.Interface().(BamlUnionDeserializer)
	as_interface.Decode(valueUnion)
	return union.Elem()

}

// Check corresponds to the Python Check model.
type Check struct {
	Name       string `json:"name"`
	Expression string `json:"expression"`
	Status     string `json:"status"`
}

// Checked is a generic struct that contains a value of any type T and a map of checks,
// where the key type CN has an underlying type string.
type Checked[T any] struct {
	Value  T                `json:"value"`
	Checks map[string]Check `json:"checks"`
}

func decodeCheckedValue[T any](valueChecked *cffi.CFFIValueChecked) Checked[T] {
	if valueChecked == nil {
		panic("decodeCheckedValue: valueChecked is nil")
	}

	value := valueChecked.Value
	checks := make(map[string]Check, len(valueChecked.Checks))
	for _, check := range valueChecked.Checks {
		checks[string(check.Name)] = Check{
			Name:       string(check.Name),
			Expression: string(check.Expression),
			Status:     string(check.Status),
		}
	}

	return Checked[T]{
		Value:  Decode(value).Interface().(T),
		Checks: checks,
	}
}

type StreamStateType string

const (
	StreamStatePending    StreamStateType = "Pending"
	StreamStateIncomplete StreamStateType = "Incomplete"
	StreamStateComplete   StreamStateType = "Complete"
)

// Values returns all allowed values for the AliasedEnum type.
func (StreamStateType) Values() []StreamStateType {
	return []StreamStateType{
		StreamStatePending,
		StreamStateIncomplete,
		StreamStateComplete,
	}
}

// IsValid checks whether the given AliasedEnum value is valid.
func (e StreamStateType) IsValid() bool {

	for _, v := range e.Values() {
		if e == v {
			return true
		}
	}
	return false

}

// MarshalJSON customizes JSON marshaling for AliasedEnum.
func (e StreamStateType) MarshalJSON() ([]byte, error) {
	if !e.IsValid() {
		return nil, fmt.Errorf("invalid StreamStateType: %q", e)
	}
	return json.Marshal(string(e))
}

// UnmarshalJSON customizes JSON unmarshaling for AliasedEnum.
func (e *StreamStateType) UnmarshalJSON(data []byte) error {
	var s string
	if err := json.Unmarshal(data, &s); err != nil {
		return err
	}
	*e = StreamStateType(s)
	if !e.IsValid() {
		return fmt.Errorf("invalid StreamStateType: %q", s)
	}
	return nil
}

type StreamState[T any] struct {
	Value T               `json:"value"`
	State StreamStateType `json:"state"`
}

func decodeStreamStateType(state cffi.CFFIStreamState) StreamStateType {
	switch state {
	case cffi.CFFIStreamState_PENDING:
		return StreamStatePending
	case cffi.CFFIStreamState_STARTED:
		return StreamStateIncomplete
	case cffi.CFFIStreamState_DONE:
		return StreamStateComplete
	default:
		panic("unexpected stream state")
	}
}

func convertFieldTypeToGoType(fieldType *cffi.CFFIFieldTypeHolder) reflect.Type {
	if fieldType == nil {
		panic("error decoding value")
	}

	type_ := fieldType.Type

	if _, ok := type_.(*cffi.CFFIFieldTypeHolder_StringType); ok {
		return reflect.TypeOf("")
	}

	if _, ok := type_.(*cffi.CFFIFieldTypeHolder_BoolType); ok {
		return reflect.TypeOf(false)
	}

	if _, ok := type_.(*cffi.CFFIFieldTypeHolder_IntType); ok {
		return reflect.TypeOf(int64(0))
	}

	if _, ok := type_.(*cffi.CFFIFieldTypeHolder_FloatType); ok {
		return reflect.TypeOf(float64(0))
	}

	if class, ok := type_.(*cffi.CFFIFieldTypeHolder_ClassType); ok {
		name := class.ClassType.Name.Name
		namespace := class.ClassType.Name.Namespace.Enum().String()
		goType, ok := typeMap[namespace+"."+name]
		if !ok {
			panic("error decoding value, class not found: " + namespace + "." + name)
		}
		return goType
	}

	if enum, ok := type_.(*cffi.CFFIFieldTypeHolder_EnumType); ok {
		name := enum.EnumType.Name
		namespace := cffi.CFFITypeNamespace_TYPES.String()
		goType, ok := typeMap[namespace+"."+name]
		if !ok {
			panic("error decoding value, enum not found: " + namespace + "." + name)
		}
		return goType
	}

	if union, ok := type_.(*cffi.CFFIFieldTypeHolder_UnionVariantType); ok {
		unionVariantType := union.UnionVariantType
		unionVariantName := unionVariantType.Name.Name
		unionVariantNamespace := unionVariantType.Name.Namespace.Enum().String()
		goType, ok := typeMap[unionVariantNamespace+"."+unionVariantName]
		if !ok {
			panic("error decoding value, union not found: " + unionVariantNamespace + "." + unionVariantName)
		}
		return goType
	}

	if optional, ok := type_.(*cffi.CFFIFieldTypeHolder_OptionalType); ok {
		optionalType := optional.OptionalType
		goType := convertFieldTypeToGoType(optionalType.Value)
		return reflect.PointerTo(goType)
	}

	if checked, ok := type_.(*cffi.CFFIFieldTypeHolder_CheckedType); ok {
		checkedType := checked.CheckedType
		return convertFieldTypeToGoType(checkedType.Value)
	}

	if streamState, ok := type_.(*cffi.CFFIFieldTypeHolder_StreamStateType); ok {
		streamStateType := streamState.StreamStateType
		return convertFieldTypeToGoType(streamStateType.Value)
	}

	if list, ok := type_.(*cffi.CFFIFieldTypeHolder_ListType); ok {
		listType := list.ListType
		return reflect.SliceOf(convertFieldTypeToGoType(listType.Element))
	}

	if map_, ok := type_.(*cffi.CFFIFieldTypeHolder_MapType); ok {
		mapType := map_.MapType
		return reflect.MapOf(convertFieldTypeToGoType(mapType.Key), convertFieldTypeToGoType(mapType.Value))
	}

	if _, ok := type_.(*cffi.CFFIFieldTypeHolder_NullType); ok {
		return reflect.TypeOf(nil)
	}

	if typeAlias, ok := type_.(*cffi.CFFIFieldTypeHolder_TypeAliasType); ok {
		name := typeAlias.TypeAliasType.Name.Name
		namespace := typeAlias.TypeAliasType.Name.Namespace.String()
		goType, ok := typeMap[namespace+"."+name]
		if !ok {
			panic("error decoding value, type alias not found: " + namespace + "." + name)
		}
		return goType
	}

	panic("error decoding value, unknown field type: " + fmt.Sprintf("%+v", fieldType))
}

func maybeDecodePrimitive(holder *cffi.CFFIValueHolder) (*reflect.Value, bool) {
	value := holder.Value

	if boolVal, ok := value.(*cffi.CFFIValueHolder_BoolValue); ok {
		value := boolVal.BoolValue
		val := reflect.ValueOf(value)
		return &val, true
	}

	if intVal, ok := value.(*cffi.CFFIValueHolder_IntValue); ok {
		value := intVal.IntValue
		val := reflect.ValueOf(value)
		return &val, true
	}

	if strVal, ok := value.(*cffi.CFFIValueHolder_StringValue); ok {
		value := strVal.StringValue
		val := reflect.ValueOf(value)
		return &val, true
	}

	if floatVal, ok := value.(*cffi.CFFIValueHolder_FloatValue); ok {
		value := floatVal.FloatValue
		val := reflect.ValueOf(value)
		return &val, true
	}

	return nil, false
}

// Used when we have a nil value but its of unknown type

func maybeOptional(value reflect.Value, targetType *cffi.CFFIFieldTypeHolder, isUnion bool) reflect.Value {
	// debugLog("decoding value: %v\n", targetType)
	if optional, ok := targetType.Type.(*cffi.CFFIFieldTypeHolder_OptionalType); ok {
		optionalType := optional.OptionalType
		if optionalType.Value.GetUnionVariantType() != nil {
			if isUnion {
				ptr := reflect.New(value.Type())
				ptr.Elem().Set(value)
				return ptr
			}
		} else {
			goType := convertFieldTypeToGoType(optionalType.Value)
			ptr := reflect.New(goType)
			ptr.Elem().Set(value)
			return ptr
		}
	}
	debugLog("  -> Not making optional")
	return value
}

func Decode(holder *cffi.CFFIValueHolder) reflect.Value {
	value := holder.Value

	if _, ok := value.(*cffi.CFFIValueHolder_NullValue); ok {
		retType := convertFieldTypeToGoType(holder.Type)
		if retType == reflect.TypeOf(nil) {
			return reflect.Zero(reflect.TypeOf((*interface{})(nil)))
		}
		// return as the null value of the type.
		return reflect.Zero(retType)
	}

	if primitiveValue, found := maybeDecodePrimitive(holder); found {
		return maybeOptional(*primitiveValue, holder.Type, false)
	}

	if listVal, ok := value.(*cffi.CFFIValueHolder_ListValue); ok {
		return maybeOptional(decodeListValue(listVal.ListValue), holder.Type, false)
	}

	if mapVal, ok := value.(*cffi.CFFIValueHolder_MapValue); ok {
		return maybeOptional(decodeMapValue(mapVal.MapValue), holder.Type, false)
	}

	if classVal, ok := value.(*cffi.CFFIValueHolder_ClassValue); ok {
		return maybeOptional(decodeClassValue(classVal.ClassValue), holder.Type, false)
	}

	if enumVal, ok := value.(*cffi.CFFIValueHolder_EnumValue); ok {
		return maybeOptional(decodeEnumValue(enumVal.EnumValue), holder.Type, false)
	}

	if unionVal, ok := value.(*cffi.CFFIValueHolder_UnionVariantValue); ok {
		decoded := decodeUnionValue(unionVal.UnionVariantValue)
		return maybeOptional(decoded, holder.Type, true)
	}

	if checkedVal, ok := value.(*cffi.CFFIValueHolder_CheckedValue); ok {
		return maybeOptional(reflect.ValueOf(decodeCheckedValue[any](checkedVal.CheckedValue)).Elem(), holder.Type, false)
	}

	if streamingVal, ok := value.(*cffi.CFFIValueHolder_StreamingStateValue); ok {
		return reflect.ValueOf(decodeStreamingStateValue(streamingVal.StreamingStateValue)).Elem()
	}

	panic("error decoding value: " + holder.String())
}

func DecodeOptional[T any](valueHolder *cffi.CFFIValueHolder, decodeFunc func(*cffi.CFFIValueHolder) T) *T {
	value := Decode(valueHolder)
	return value.Interface().(*T)
}

func DecodeList[T any](valueHolder *cffi.CFFIValueHolder, decodeFunc func(*cffi.CFFIValueHolder) T) []T {
	list := valueHolder.GetListValue()
	if list == nil {
		panic("error decoding value, expected list")
	}

	values := make([]T, len(list.Values))
	for i, v := range list.Values {
		values[i] = decodeFunc(v)
	}
	return values
}

func DecodeMap[T any](valueHolder *cffi.CFFIValueHolder, decodeFunc func(*cffi.CFFIValueHolder) T) map[string]T {
	map_ := valueHolder.GetMapValue()
	if map_ == nil {
		panic("error decoding value, expected map")
	}

	values := make(map[string]T)
	for _, entry := range map_.Entries {
		key := entry.Key
		value := entry.Value
		values[key] = decodeFunc(value)
	}
	return values
}

func DecodeStreamingState[T any](valueHolder *cffi.CFFIValueHolder, decodeFunc func(*cffi.CFFIValueHolder) T) StreamState[T] {
	streamingState := valueHolder.GetStreamingStateValue()
	if streamingState == nil {
		// This happens due ot partialization of types sometimes.
		return StreamState[T]{
			State: StreamStatePending,
		}
	}

	return StreamState[T]{
		Value: decodeFunc(streamingState.Value),
		State: decodeStreamStateType(streamingState.State),
	}
}

package baml

import (
	"encoding/json"
	"fmt"
	"reflect"

	"github.com/boundaryml/baml/engine/language_client_go/pkg/cffi"
)

type TypeMap map[string]reflect.Type

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
		d.Fields[key] = decodeInternal(valueHolder)
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

func decodeListValue(valueList *cffi.CFFIValueList) any {
	if valueList == nil {
		panic("decodeListValue: valueList is nil")
	}

	elementType := valueList.ValueType
	goElementType := convertFieldTypeToGoType(elementType)

	// check if goValueType is a pointer
	isValueTypePtr := goElementType.Kind() == reflect.Ptr

	// is union, enum, or class (i.e. implements BamlClassDeserializer, BamlEnumDeserializer, BamlUnionDeserializer)
	isValueTypeUnion := elementType.GetUnionVariantType() != nil
	isValueTypeEnum := elementType.GetEnumType() != nil
	isValueTypeClass := elementType.GetClassType() != nil
	isValueCustomType := isValueTypeUnion || isValueTypeEnum || isValueTypeClass
	castToPointer := (isValueTypePtr && !isValueCustomType)

	length := len(valueList.Values)
	values := reflect.MakeSlice(reflect.SliceOf(goElementType), length, length)

	for i, v := range valueList.Values {
		decodedValue := decodeInternal(v)

		if castToPointer {
			if isValueCustomType {
				values.Index(i).Set(reflect.ValueOf(decodedValue))
			} else {
				values.Index(i).Set(reflect.ValueOf(&decodedValue))
			}
		} else {
			if isValueCustomType {
				// deref the value if it's a pointer
				values.Index(i).Set(reflect.ValueOf(decodedValue).Elem())
			} else {
				values.Index(i).Set(reflect.ValueOf(decodedValue))
			}
		}
	}

	return values.Interface()
}

func decodeMapValue(valueMap *cffi.CFFIValueMap) any {
	if valueMap == nil {
		panic("decodeMapValue: valueMap is nil")
	}
	keyType := valueMap.KeyType
	valueType := valueMap.ValueType
	goKeyType := convertFieldTypeToGoType(keyType)
	goValueType := convertFieldTypeToGoType(valueType)

	// check if goValueType is a pointer
	isValueTypePtr := goValueType.Kind() == reflect.Ptr

	// is union, enum, or class (i.e. implements BamlClassDeserializer, BamlEnumDeserializer, BamlUnionDeserializer)
	isValueTypeUnion := valueType.GetUnionVariantType() != nil
	isValueTypeEnum := valueType.GetEnumType() != nil
	isValueTypeClass := valueType.GetClassType() != nil
	isValueCustomType := isValueTypeUnion || isValueTypeEnum || isValueTypeClass
	castToPointer := (isValueTypePtr && !isValueCustomType)

	values := reflect.MakeMap(reflect.MapOf(goKeyType, goValueType))

	for _, entry := range valueMap.Entries {
		key := entry.Key
		value := entry.Value
		decodedValue := decodeInternal(value)
		if castToPointer {
			if isValueCustomType {
				values.SetMapIndex(reflect.ValueOf(key), reflect.ValueOf(decodedValue))
			} else {
				values.SetMapIndex(reflect.ValueOf(key), reflect.ValueOf(&decodedValue))
			}
		} else {
			if isValueCustomType {
				// deref the value if it's a pointer
				values.SetMapIndex(reflect.ValueOf(key), reflect.ValueOf(decodedValue).Elem())
			} else {
				values.SetMapIndex(reflect.ValueOf(key), reflect.ValueOf(decodedValue))
			}
		}
	}
	return values.Interface()
}

func decodeStreamingStateValue(valueStreamingState *cffi.CFFIValueStreamingState) StreamState[any] {
	if valueStreamingState == nil {
		panic("error decoding value")
	}
	value := valueStreamingState.Value
	return StreamState[any]{
		Value: decodeInternal(value),
		State: decodeStreamStateType(valueStreamingState.State),
	}
}

type BamlDecoder interface {
	BamlDecode(decodedMap map[string]any)
}

func decodeClassValue(valueClass *cffi.CFFIValueClass) any {
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
		return &dynamicClass
	}

	cls := reflect.New(found)
	as_interface := cls.Interface().(BamlClassDeserializer)
	as_interface.Decode(valueClass)
	return as_interface
}

func decodeEnumValue(valueEnum *cffi.CFFIValueEnum) any {
	if valueEnum == nil {
		panic("decodeEnumValue: valueEnum is nil")
	}

	typeName := valueEnum.Name
	namespace := typeName.Namespace.String()
	enumName := string(typeName.Name)
	found, ok := typeMap[namespace+"."+enumName]
	if !ok {
		return &DynamicEnum{Name: enumName, Value: string(valueEnum.Value)}
	}
	enum := reflect.New(found)
	as_interface := enum.Interface().(BamlEnumDeserializer)
	as_interface.Decode(valueEnum)
	return as_interface
}

func decodeUnionValue(valueUnion *cffi.CFFIValueUnionVariant) any {
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
		return decodeInternal(value)
	}

	found, ok := typeMap[namespace+"."+unionName]
	if !ok {
		// This is a fully dynamic union, so we
		// decode the value as the value and drop
		// union type information
		value := valueUnion.Value
		return decodeInternal(value)
	}

	union := reflect.New(found)
	as_interface := union.Interface().(BamlUnionDeserializer)
	as_interface.Decode(valueUnion)
	return as_interface

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
		Value:  decodeInternal(value).(T),
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

	panic("error decoding value, unknown field type: " + fmt.Sprintf("%+v", fieldType))
}

func Decode(holder *cffi.CFFIValueHolder) any {

	value := holder.Value

	if _, ok := value.(*cffi.CFFIValueHolder_NullValue); ok {
		return nil
	}

	if boolVal, ok := value.(*cffi.CFFIValueHolder_BoolValue); ok {
		value := boolVal.BoolValue
		return value
	}

	if intVal, ok := value.(*cffi.CFFIValueHolder_IntValue); ok {
		value := intVal.IntValue
		return value
	}

	if strVal, ok := value.(*cffi.CFFIValueHolder_StringValue); ok {
		value := strVal.StringValue
		return value
	}

	if floatVal, ok := value.(*cffi.CFFIValueHolder_FloatValue); ok {
		value := floatVal.FloatValue
		return value
	}

	if listVal, ok := value.(*cffi.CFFIValueHolder_ListValue); ok {
		return decodeListValue(listVal.ListValue)
	}

	if mapVal, ok := value.(*cffi.CFFIValueHolder_MapValue); ok {
		return decodeMapValue(mapVal.MapValue)
	}

	if classVal, ok := value.(*cffi.CFFIValueHolder_ClassValue); ok {
		return decodeClassValue(classVal.ClassValue)
	}

	if enumVal, ok := value.(*cffi.CFFIValueHolder_EnumValue); ok {
		return decodeEnumValue(enumVal.EnumValue)
	}

	if unionVal, ok := value.(*cffi.CFFIValueHolder_UnionVariantValue); ok {
		return decodeUnionValue(unionVal.UnionVariantValue)
	}

	if checkedVal, ok := value.(*cffi.CFFIValueHolder_CheckedValue); ok {
		return decodeCheckedValue[any](checkedVal.CheckedValue)
	}

	if streamingVal, ok := value.(*cffi.CFFIValueHolder_StreamingStateValue); ok {
		return decodeStreamingStateValue(streamingVal.StreamingStateValue)
	}

	panic("error decoding value: " + holder.String())
}

func decodeInternal(holder *cffi.CFFIValueHolder) any {
	decoded := Decode(holder)
	// reflected := reflect.ValueOf(decoded)
	// // if is *string, *int, *float, *bool, return the value
	// if reflected.Kind() == reflect.Ptr {
	// 	elem := reflected.Elem()
	// 	// if is *string, *int, *float, *bool, return the value
	// 	if elem.Kind() == reflect.String ||
	// 		elem.Kind() == reflect.Int ||
	// 		elem.Kind() == reflect.Float64 ||
	// 		elem.Kind() == reflect.Bool {
	// 		return elem
	// 	}
	// }

	return decoded
}

func DecodeOptional[T any](valueHolder *cffi.CFFIValueHolder, decodeFunc func(*cffi.CFFIValueHolder) T) *T {
	value := Decode(valueHolder)
	if value == nil {
		return nil
	}
	return value.(*T)
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

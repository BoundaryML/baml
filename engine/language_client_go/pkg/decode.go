package baml

import (
	"encoding/json"
	"fmt"
	"reflect"
	"strings"

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
		d.Fields[key] = Decode(valueHolder)
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

	length := len(valueList.Values)
	values := reflect.MakeSlice(reflect.SliceOf(goElementType), length, length)

	for i, v := range valueList.Values {
		rv := reflect.ValueOf(Decode(v))
		if rv.Kind() == reflect.Ptr {
			rv = rv.Elem()
		}
		values.Index(i).Set(rv)
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

	values := reflect.MakeMap(reflect.MapOf(goKeyType, goValueType))

	for _, entry := range valueMap.Entries {
		key := entry.Key
		value := entry.Value
		values.SetMapIndex(reflect.ValueOf(key), reflect.ValueOf(Decode(value)))
	}
	return values.Interface()
}

func decodeStreamingStateValue(valueStreamingState *cffi.CFFIValueStreamingState) StreamState[any] {
	if valueStreamingState == nil {
		panic("error decoding value")
	}
	value := valueStreamingState.Value
	return StreamState[any]{
		Value: Decode(value),
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
		fmt.Printf("decodeClassValue: class not found, namespace=%s, className=%s, typeMap=%+v\n", namespace, className, typeMap)
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
		fmt.Printf("decodeEnumValue: enum not found, namespace=%s, enumName=%s, typeMap=%+v\n", namespace, enumName, typeMap)
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

	fmt.Printf("decodeUnionValue: valueUnion=%+v\n", valueUnion)

	typeName := valueUnion.Name
	namespace := typeName.Namespace.String()
	unionName := string(typeName.Name)

	// === DECODE LOGGING START ===
	fmt.Printf("\n=== UNION DECODE ===\n")
	fmt.Printf("Type: %s.%s\n", namespace, unionName)
	fmt.Printf("Variant: %s\n", valueUnion.VariantName)

	// Show field types to identify union structure
	var fieldTypeStrs []string
	var isOptionalPattern bool = false
	for _, ft := range valueUnion.FieldTypes {
		if ft.GetClassType() != nil {
			fieldTypeStrs = append(fieldTypeStrs, ft.GetClassType().Name.Name)
		} else if ft.GetNullType() != nil {
			fieldTypeStrs = append(fieldTypeStrs, "null")
		} else if ft.GetStringType() != nil {
			fieldTypeStrs = append(fieldTypeStrs, "string")
		} else if ft.GetIntType() != nil {
			fieldTypeStrs = append(fieldTypeStrs, "int")
		} else {
			fieldTypeStrs = append(fieldTypeStrs, "?")
		}
	}

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

	fmt.Printf("Union structure: (%s)\n", strings.Join(fieldTypeStrs, " | "))
	if isOptionalPattern {
		fmt.Printf("Pattern: Optional type (T | null)\n")
	}
	fmt.Printf("===================\n\n")
	// === DECODE LOGGING END ===

	// For optional patterns (T | null), decode the inner value directly
	// These shouldn't be looked up as union types
	if isOptionalPattern {
		fmt.Printf("Handling as optional type, decoding inner value\n")
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

	fmt.Printf("decodeUnionValue: found=%+v\n", found)

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

	// TODO: Will this cast correctly?
	// this is a Checked[any], but we really want to return a Checked[T]
	return Checked[T]{
		Value:  Decode(value).(T),
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

	if _, ok := type_.(*cffi.CFFIFieldTypeHolder_UnionVariantType); ok {
		panic("union not supported yet")
		// name := union.UnionVariantType.Name.Name
		// namespace := cffi.CFFITypeNamespace_TYPES.String()
		// goType, ok := typeMap[namespace+"."+name]
		// if !ok {
		// 	panic("error decoding value, union not found: " + namespace + "." + name)
		// }
		// return goType
	}

	panic("error decoding value, unknown field type")
}

func Decode(holder *cffi.CFFIValueHolder) any {

	fmt.Printf("Decode: holder=%v\n", holder)

	value := holder.Value

	if _, ok := value.(*cffi.CFFIValueHolder_NullValue); ok {
		return nil
	}

	if boolVal, ok := value.(*cffi.CFFIValueHolder_BoolValue); ok {
		value := boolVal.BoolValue
		return &value
	}

	if intVal, ok := value.(*cffi.CFFIValueHolder_IntValue); ok {
		value := intVal.IntValue
		return &value
	}

	if strVal, ok := value.(*cffi.CFFIValueHolder_StringValue); ok {
		value := strVal.StringValue
		return &value
	}

	if floatVal, ok := value.(*cffi.CFFIValueHolder_FloatValue); ok {
		value := floatVal.FloatValue
		return &value
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
		fmt.Printf("DecodeStreamingState: streamingState is nil, please notify BAML team\n")
		return StreamState[T]{
			State: StreamStatePending,
		}
	}

	return StreamState[T]{
		Value: decodeFunc(streamingState.Value),
		State: decodeStreamStateType(streamingState.State),
	}
}

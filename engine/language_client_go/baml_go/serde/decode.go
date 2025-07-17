package serde

import (
	"fmt"
	"os"
	"reflect"

	"github.com/boundaryml/baml/engine/language_client_go/baml_go/shared"
	"github.com/boundaryml/baml/engine/language_client_go/pkg/cffi"
)

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
	Decode(holder *cffi.CFFIValueClass, typeMap TypeMap)
}

type BamlEnumDeserializer interface {
	Decode(holder *cffi.CFFIValueEnum, typeMap TypeMap)
}

type BamlUnionDeserializer interface {
	Decode(holder *cffi.CFFIValueUnionVariant, typeMap TypeMap)
}

type DynamicClass struct {
	Name   string
	Fields map[string]any
}

func (d *DynamicClass) Decode(holder *cffi.CFFIValueClass, typeMap TypeMap) {
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
		d.Fields[key] = Decode(valueHolder, typeMap).Interface()
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

func decodeListValue(valueList *cffi.CFFIValueList, typeMap TypeMap) reflect.Value {
	debugLog("decodeListValue: valueList=%+v\n", valueList)
	if valueList == nil {
		panic("decodeListValue: valueList is nil")
	}

	elementType := valueList.ValueType
	goElementType := convertFieldTypeToGoType(elementType, typeMap)

	length := len(valueList.Values)
	debugLog("goElementType: %v\n", goElementType)
	debugLog("length: %v\n", length)
	sliceOf := reflect.SliceOf(goElementType)
	debugLog("sliceOf: %v\n", sliceOf)
	values := reflect.MakeSlice(sliceOf, length, length)
	debugLog("values: %v\n", values)

	for i, v := range valueList.Values {
		decodedValue := Decode(v, typeMap)
		values.Index(i).Set(decodedValue)
	}

	return values
}

func decodeMapValue(valueMap *cffi.CFFIValueMap, typeMap TypeMap) reflect.Value {
	if valueMap == nil {
		panic("decodeMapValue: valueMap is nil")
	}
	debugLog("decodeMapValue: valueMap=%+v\n", valueMap)
	keyType := valueMap.KeyType
	valueType := valueMap.ValueType
	goKeyType := convertFieldTypeToGoType(keyType, typeMap)
	goValueType := convertFieldTypeToGoType(valueType, typeMap)

	debugLog("goKeyType: %v\n", goKeyType)
	debugLog("goValueType: %v\n", goValueType)

	values := reflect.MakeMap(reflect.MapOf(goKeyType, goValueType))

	for _, entry := range valueMap.Entries {
		key := entry.Key
		value := entry.Value
		decodedValue := Decode(value, typeMap)
		debugLog("key: %v, decodedValue: %v\n", key, decodedValue)
		values.SetMapIndex(reflect.ValueOf(key), decodedValue)
	}
	return values
}

func decodeStreamingStateValue(valueStreamingState *cffi.CFFIValueStreamingState, typeMap TypeMap) shared.StreamState[any] {
	if valueStreamingState == nil {
		panic("error decoding value")
	}
	value := valueStreamingState.Value
	return shared.StreamState[any]{
		Value: Decode(value, typeMap).Interface(),
		State: decodeStreamStateType(valueStreamingState.State),
	}
}

func decodeClassValue(valueClass *cffi.CFFIValueClass, typeMap TypeMap) reflect.Value {
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
		dynamicClass.Decode(valueClass, typeMap)
		return reflect.ValueOf(dynamicClass)
	}

	cls := reflect.New(found)
	as_interface := cls.Interface().(BamlClassDeserializer)
	as_interface.Decode(valueClass, typeMap)
	return cls.Elem()
}

func decodeEnumValue(valueEnum *cffi.CFFIValueEnum, typeMap TypeMap) reflect.Value {
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
	as_interface.Decode(valueEnum, typeMap)
	return enum.Elem()
}

func decodeUnionValue(valueUnion *cffi.CFFIValueUnionVariant, typeMap TypeMap) reflect.Value {
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
		return Decode(value, typeMap)
	}

	found, ok := typeMap[namespace+"."+unionName]
	if !ok {
		// This is a fully dynamic union, so we
		// decode the value as the value and drop
		// union type information
		value := valueUnion.Value
		return Decode(value, typeMap)
	}

	union := reflect.New(found)
	as_interface := union.Interface().(BamlUnionDeserializer)
	as_interface.Decode(valueUnion, typeMap)
	return union.Elem()

}

func decodeCheckedValue[T any](valueChecked *cffi.CFFIValueChecked, typeMap TypeMap) shared.Checked[T] {
	if valueChecked == nil {
		panic("decodeCheckedValue: valueChecked is nil")
	}

	value := valueChecked.Value
	checks := make(map[string]shared.Check, len(valueChecked.Checks))
	for _, check := range valueChecked.Checks {
		checks[string(check.Name)] = shared.Check{
			Name:       string(check.Name),
			Expression: string(check.Expression),
			Status:     string(check.Status),
		}
	}

	return shared.Checked[T]{
		Value:  Decode(value, typeMap).Interface().(T),
		Checks: checks,
	}
}

func decodeStreamStateType(state cffi.CFFIStreamState) shared.StreamStateType {
	switch state {
	case cffi.CFFIStreamState_PENDING:
		return shared.StreamStatePending
	case cffi.CFFIStreamState_STARTED:
		return shared.StreamStateIncomplete
	case cffi.CFFIStreamState_DONE:
		return shared.StreamStateComplete
	default:
		panic("unexpected stream state")
	}
}

func convertFieldTypeToGoType(fieldType *cffi.CFFIFieldTypeHolder, typeMap TypeMap) reflect.Type {
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
		goType := convertFieldTypeToGoType(optionalType.Value, typeMap)
		return reflect.PointerTo(goType)
	}

	if checked, ok := type_.(*cffi.CFFIFieldTypeHolder_CheckedType); ok {
		checkedType := checked.CheckedType
		return convertFieldTypeToGoType(checkedType.Value, typeMap)
	}

	if streamState, ok := type_.(*cffi.CFFIFieldTypeHolder_StreamStateType); ok {
		streamStateType := streamState.StreamStateType
		return convertFieldTypeToGoType(streamStateType.Value, typeMap)
	}

	if list, ok := type_.(*cffi.CFFIFieldTypeHolder_ListType); ok {
		listType := list.ListType
		return reflect.SliceOf(convertFieldTypeToGoType(listType.Element, typeMap))
	}

	if map_, ok := type_.(*cffi.CFFIFieldTypeHolder_MapType); ok {
		mapType := map_.MapType
		return reflect.MapOf(convertFieldTypeToGoType(mapType.Key, typeMap), convertFieldTypeToGoType(mapType.Value, typeMap))
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

	// any is weird in go, (alias for interface{})
	if _, ok := type_.(*cffi.CFFIFieldTypeHolder_NullType); ok {
		if _, ok := typeMap["INTERNAL.nil"]; ok {
			return reflect.TypeOf((*interface{})(nil)).Elem()
		}
		return reflect.TypeOf((*interface{})(nil))
	}
	if _, ok := type_.(*cffi.CFFIFieldTypeHolder_AnyType); ok {
		return reflect.TypeOf((*interface{})(nil)).Elem()
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

func maybeOptional(value reflect.Value, targetType *cffi.CFFIFieldTypeHolder, isUnion bool, typeMap TypeMap) reflect.Value {
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
			goType := convertFieldTypeToGoType(optionalType.Value, typeMap)
			ptr := reflect.New(goType)
			ptr.Elem().Set(value)
			return ptr
		}
	}
	debugLog("  -> Not making optional")
	return value
}

func Decode(holder *cffi.CFFIValueHolder, typeMap TypeMap) reflect.Value {
	value := holder.Value

	if _, ok := value.(*cffi.CFFIValueHolder_NullValue); ok {
		retType := convertFieldTypeToGoType(holder.Type, typeMap)
		// return as the null value of the type.
		return reflect.Zero(retType)
	}

	if primitiveValue, found := maybeDecodePrimitive(holder); found {
		return maybeOptional(*primitiveValue, holder.Type, false, typeMap)
	}

	if listVal, ok := value.(*cffi.CFFIValueHolder_ListValue); ok {
		return maybeOptional(decodeListValue(listVal.ListValue, typeMap), holder.Type, false, typeMap)
	}

	if mapVal, ok := value.(*cffi.CFFIValueHolder_MapValue); ok {
		return maybeOptional(decodeMapValue(mapVal.MapValue, typeMap), holder.Type, false, typeMap)
	}

	if classVal, ok := value.(*cffi.CFFIValueHolder_ClassValue); ok {
		return maybeOptional(decodeClassValue(classVal.ClassValue, typeMap), holder.Type, false, typeMap)
	}

	if enumVal, ok := value.(*cffi.CFFIValueHolder_EnumValue); ok {
		return maybeOptional(decodeEnumValue(enumVal.EnumValue, typeMap), holder.Type, false, typeMap)
	}

	if unionVal, ok := value.(*cffi.CFFIValueHolder_UnionVariantValue); ok {
		decoded := decodeUnionValue(unionVal.UnionVariantValue, typeMap)
		return maybeOptional(decoded, holder.Type, true, typeMap)
	}

	if checkedVal, ok := value.(*cffi.CFFIValueHolder_CheckedValue); ok {
		return maybeOptional(reflect.ValueOf(decodeCheckedValue[any](checkedVal.CheckedValue, typeMap)).Elem(), holder.Type, false, typeMap)
	}

	if streamingVal, ok := value.(*cffi.CFFIValueHolder_StreamingStateValue); ok {
		return reflect.ValueOf(decodeStreamingStateValue(streamingVal.StreamingStateValue, typeMap)).Elem()
	}

	panic("error decoding value: " + holder.String())
}

func DecodeStreamingState[T any](holder *cffi.CFFIValueHolder, decodeFunc func(inner *cffi.CFFIValueHolder) T) shared.StreamState[T] {
	value := holder.Value
	if streamingVal, ok := value.(*cffi.CFFIValueHolder_StreamingStateValue); ok {
		return shared.StreamState[T]{
			Value: decodeFunc(streamingVal.StreamingStateValue.Value),
			State: decodeStreamStateType(streamingVal.StreamingStateValue.State),
		}
	}
	panic("error decoding streaming state: " + holder.String())
}

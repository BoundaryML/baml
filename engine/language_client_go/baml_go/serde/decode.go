package serde

import (
	"fmt"
	"os"
	"reflect"
	"strconv"
	"strings"

	"github.com/boundaryml/baml/engine/language_client_go/baml_go/shared"
	"github.com/boundaryml/baml/engine/language_client_go/pkg/cffi"
)

// debugLog prints debug information if BAML_INTERNAL_LOG=trace is set
func debugLog(format string, args ...interface{}) {
	if os.Getenv("BAML_INTERNAL_LOG") == "trace" { // TODO: remove this once we have a proper logging system
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
	fieldCount := len(holder.Fields)
	d.Fields = make(map[string]any, fieldCount)
	for i := 0; i < fieldCount; i++ {
		field := holder.Fields[i]
		if field == nil {
			panic(fmt.Sprintf("DynamicClass.Decode: field[%d] is nil, holder.Fields=%+v", i, holder.Fields))
		}
		key := field.Key
		valueHolder := field.Value
		value, goType := Decode(valueHolder, typeMap)
		switch goType {
		case reflect.TypeOf(int64(0)):
			d.Fields[key] = value.Int()
		case reflect.TypeOf(float64(0)):
			d.Fields[key] = value.Float()
		case reflect.TypeOf(false):
			d.Fields[key] = value.Bool()
		default:
			d.Fields[key] = value.Interface()
		}
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

type DynamicUnion struct {
	Variant string
	Value   any
}

func (d *DynamicUnion) Decode(holder *cffi.CFFIValueUnionVariant, typeMap TypeMap) {
	d.Variant = string(holder.ValueOptionName)
	value, goType := Decode(holder.Value, typeMap)
	switch goType {
	case reflect.TypeOf(int64(0)):
		d.Value = value.Int()
	case reflect.TypeOf(float64(0)):
		d.Value = value.Float()
	case reflect.TypeOf(false):
		d.Value = value.Bool()
	default:
		d.Value = value.Interface()
	}
}

func decodeListValue(valueList *cffi.CFFIValueList, typeMap TypeMap) (reflect.Value, reflect.Type) {
	if valueList == nil {
		panic("decodeListValue: valueList is nil")
	}

	goElementType := convertFieldTypeToGoType(valueList.ItemType, typeMap)

	if goElementType == typeMap.typeMap["INTERNAL.nil"] {
		values := []any{}
		for _, v := range valueList.Items {
			decodedValue, _ := Decode(v, typeMap)
			values = append(values, decodedValue.Elem())
		}
		return reflect.ValueOf(values), reflect.TypeOf(values)
	} else {
		length := len(valueList.Items)
		sliceOf := reflect.SliceOf(goElementType)
		values := reflect.MakeSlice(sliceOf, length, length)

		for i, v := range valueList.Items {
			decodedValue, _ := Decode(v, typeMap)
			values.Index(i).Set(decodedValue)
		}
		return values, sliceOf
	}
}

func decodeMapValue(valueMap *cffi.CFFIValueMap, typeMap TypeMap) (reflect.Value, reflect.Type) {
	if valueMap == nil {
		panic("decodeMapValue: valueMap is nil")
	}
	keyType := valueMap.KeyType
	valueType := valueMap.ValueType
	goKeyType := convertFieldTypeToGoType(keyType, typeMap)
	goValueType := convertFieldTypeToGoType(valueType, typeMap)
	debugLog("goValueType: %+v\n", goValueType)
	debugLog("typeMap.typeMap[\"INTERNAL.nil\"]: %+v\n", typeMap.typeMap["INTERNAL.nil"])
	if goValueType == typeMap.typeMap["INTERNAL.nil"] {
		values := map[string]any{}
		for _, entry := range valueMap.Entries {
			key := entry.Key
			value := entry.Value
			decodedValue, goType := Decode(value, typeMap)
			switch goType {
			case reflect.TypeOf(int64(0)):
				values[key] = decodedValue.Int()
			case reflect.TypeOf(float64(0)):
				values[key] = decodedValue.Float()
			case reflect.TypeOf(false):
				values[key] = decodedValue.Bool()
			default:
				values[key] = decodedValue.Interface()
			}
		}
		return reflect.ValueOf(values), reflect.TypeOf(values)
	} else {
		mapType := reflect.MapOf(goKeyType, goValueType)
		values := reflect.MakeMap(mapType)
		for _, entry := range valueMap.Entries {
			key := entry.Key
			value := entry.Value
			decodedValue, _ := Decode(value, typeMap)
			values.SetMapIndex(reflect.ValueOf(key), decodedValue)
		}
		return values, mapType
	}
}

func decodeStreamingStateValue(valueStreamingState *cffi.CFFIValueStreamingState, typeMap TypeMap) (reflect.Value, reflect.Type) {
	if valueStreamingState == nil {
		panic("error decoding value")
	}
	value, _ := Decode(valueStreamingState.Value, typeMap)
	streamStateType := decodeStreamStateType(valueStreamingState.State)
	goType, ok := typeMap.GetType(valueStreamingState.Name)
	if !ok {
		panic("error decoding value, stream state type not found: " + valueStreamingState.Name.String())
	}
	streamState := reflect.New(goType)
	streamState.Elem().FieldByName("Value").Set(value)
	streamState.Elem().FieldByName("State").Set(reflect.ValueOf(streamStateType))
	return streamState.Elem(), goType
}

func decodeClassValue(valueClass *cffi.CFFIValueClass, typeMap TypeMap) (reflect.Value, reflect.Type) {
	if valueClass == nil {
		panic("decodeClassValue: valueClass is nil")
	}

	goType, ok := typeMap.GetType(valueClass.Name)
	if !ok {
		// This is a fully dynamic class, so we need to decode it as a map
		dynamicClass := DynamicClass{
			Name: valueClass.Name.Name,
		}
		dynamicClass.Decode(valueClass, typeMap)
		return reflect.ValueOf(dynamicClass), reflect.TypeOf(DynamicClass{})
	}

	cls := reflect.New(goType)
	as_interface := cls.Interface().(BamlClassDeserializer)
	as_interface.Decode(valueClass, typeMap)
	return cls.Elem(), goType
}

func decodeEnumValue(valueEnum *cffi.CFFIValueEnum, typeMap TypeMap) (reflect.Value, reflect.Type) {
	if valueEnum == nil {
		panic("decodeEnumValue: valueEnum is nil")
	}

	goType, ok := typeMap.GetType(valueEnum.Name)
	if !ok {
		dynamicEnum := DynamicEnum{Name: valueEnum.Name.Name, Value: valueEnum.Value}
		return reflect.ValueOf(dynamicEnum), reflect.TypeOf(DynamicEnum{})
	}
	enum := reflect.New(goType)
	as_interface := enum.Interface().(BamlEnumDeserializer)
	as_interface.Decode(valueEnum, typeMap)
	return enum.Elem(), goType
}

func decodeUnionValue(valueUnion *cffi.CFFIValueUnionVariant, typeMap TypeMap) (reflect.Value, reflect.Type) {
	if valueUnion == nil {
		panic("decodeUnionValue: valueUnion is nil")
	}

	value, goType := func() (reflect.Value, reflect.Type) {
		if ok := valueUnion.Value.GetNullValue(); ok != nil {
			// If the union value is null, return nil
			return reflect.ValueOf(nil), nil
		} else if valueUnion.IsSinglePattern {
			// For optional patterns (T | null), decode the inner value directly
			// These shouldn't be looked up as union types
			// Ignore the union-ness of it and just decode the inner value
			return Decode(valueUnion.Value, typeMap)
		} else {
			goType, ok := typeMap.GetType(valueUnion.Name)
			if !ok {
				// Union not found
				// This is a fully dynamic union, so we
				// decode the value as the value and drop
				// union type information
				value, goType := Decode(valueUnion.Value, typeMap)
				dynamicUnion := DynamicUnion{
					Variant: valueUnion.Name.Name,
				}

				switch goType {
				case reflect.TypeOf(int64(0)):
					dynamicUnion.Value = value.Int()
				case reflect.TypeOf(float64(0)):
					dynamicUnion.Value = value.Float()
				case reflect.TypeOf(false):
					dynamicUnion.Value = value.Bool()
				default:
					dynamicUnion.Value = value.Interface()
				}
				value = reflect.ValueOf(dynamicUnion)
				goType = reflect.TypeOf(DynamicUnion{})
				return value, goType
			}

			union := reflect.New(goType)
			as_interface := union.Interface().(BamlUnionDeserializer)
			as_interface.Decode(valueUnion, typeMap)
			return union.Elem(), goType
		}
	}()

	if valueUnion.IsOptional {
		if goType == nil {
			debugLog(" -> got a nil goType, so returning nil\n")
			// got a nill value, so return nil
			goType := convertFieldTypeToGoType(valueUnion.SelfType, typeMap)
			// goType should be a pointer
			ptr := reflect.New(goType)
			// use .Elem to return the inner pointer
			return ptr.Elem(), goType
		}
		ptr := reflect.New(goType)
		ptr.Elem().Set(value)
		return ptr, ptr.Type()
	}

	return value, goType
}

func decodeCheckedValue(valueChecked *cffi.CFFIValueChecked, typeMap TypeMap) (reflect.Value, reflect.Type) {
	if valueChecked == nil {
		panic("decodeCheckedValue: valueChecked is nil")
	}

	value := valueChecked.Value
	decodedValue, _ := Decode(value, typeMap)
	checks := make(map[string]shared.Check, len(valueChecked.Checks))
	for _, check := range valueChecked.Checks {
		checks[string(check.Name)] = shared.Check{
			Name:       string(check.Name),
			Expression: string(check.Expression),
			Status:     string(check.Status),
		}
	}
	goType, ok := typeMap.GetType(valueChecked.Name)
	if !ok {
		panic("error decoding value, checked type not found: " + valueChecked.Name.String())
	}
	checkedValue := reflect.New(goType)
	checkedValue.Elem().FieldByName("Value").Set(decodedValue)
	checkedValue.Elem().FieldByName("Checks").Set(reflect.ValueOf(checks))
	return checkedValue.Elem(), goType
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

	if literal, ok := type_.(*cffi.CFFIFieldTypeHolder_LiteralType); ok {
		literalType := literal.LiteralType
		switch literalType.Literal.(type) {
		case *cffi.CFFIFieldTypeLiteral_BoolLiteral:
			return reflect.TypeOf(false)
		case *cffi.CFFIFieldTypeLiteral_IntLiteral:
			return reflect.TypeOf(int64(0))
		case *cffi.CFFIFieldTypeLiteral_StringLiteral:
			return reflect.TypeOf("")
		default:
			panic(fmt.Sprintf("unexpected cffi.isCFFIFieldTypeLiteral_Literal: %#v", literalType.Literal))
		}
	}

	if class, ok := type_.(*cffi.CFFIFieldTypeHolder_ClassType); ok {
		name := class.ClassType.Name.Name
		goType, ok := typeMap.GetType(class.ClassType.Name)
		if !ok {
			// going to be a dynamic class
			return reflect.TypeOf(DynamicClass{
				Name: name,
			})
		}
		return goType
	}

	if enum, ok := type_.(*cffi.CFFIFieldTypeHolder_EnumType); ok {
		name := enum.EnumType.Name
		namespace := cffi.CFFITypeNamespace_TYPES.String()
		goType, ok := typeMap.typeMap[namespace+"."+name]
		if !ok {
			// going to be a dynamic enum
			return reflect.TypeOf(DynamicEnum{
				Name:  name,
				Value: "",
			})
		}
		return goType
	}

	if union, ok := type_.(*cffi.CFFIFieldTypeHolder_UnionVariantType); ok {
		unionVariantType := union.UnionVariantType
		unionVariantName := unionVariantType.Name.Name
		goType, ok := typeMap.GetType(unionVariantType.Name)
		if !ok {
			// going to be a dynamic union
			if typeMap.allowDynamicUnion {
				return reflect.TypeOf(DynamicUnion{
					Variant: unionVariantName,
					Value:   nil,
				})
			} else {
				return typeMap.typeMap["INTERNAL.nil"]
			}
		} else {
			return goType
		}

	}

	if optional, ok := type_.(*cffi.CFFIFieldTypeHolder_OptionalType); ok {
		optionalType := optional.OptionalType
		goType := convertFieldTypeToGoType(optionalType.Value, typeMap)
		if goType == typeMap.typeMap["INTERNAL.nil"] {
			// Pointer to nil / any is same as nil / any
			return goType
		}
		return reflect.PointerTo(goType)
	}

	if checked, ok := type_.(*cffi.CFFIFieldTypeHolder_CheckedType); ok {
		checkedType := checked.CheckedType
		serializeType := typeToString(checkedType.Value)
		goType, ok := typeMap.typeMap["CHECKED_TYPES."+serializeType]
		if !ok {
			panic("error decoding type, checked type not found: " + serializeType + ", typeMap=" + fmt.Sprintf("%+v", typeMap))
		}
		return goType
	}

	if streamState, ok := type_.(*cffi.CFFIFieldTypeHolder_StreamStateType); ok {
		streamStateType := streamState.StreamStateType
		return convertFieldTypeToGoType(streamStateType.Value, typeMap)
	}

	if list, ok := type_.(*cffi.CFFIFieldTypeHolder_ListType); ok {
		listType := list.ListType
		goElementType := convertFieldTypeToGoType(listType.ItemType, typeMap)
		if goElementType == typeMap.typeMap["INTERNAL.nil"] {
			return reflect.TypeOf([]any{})
		}
		return reflect.SliceOf(goElementType)
	}

	if map_, ok := type_.(*cffi.CFFIFieldTypeHolder_MapType); ok {
		mapType := map_.MapType
		goKeyType := convertFieldTypeToGoType(mapType.KeyType, typeMap)
		goValueType := convertFieldTypeToGoType(mapType.ValueType, typeMap)
		if goValueType == typeMap.typeMap["INTERNAL.nil"] {
			return reflect.TypeOf(map[string]any{})
		}
		return reflect.MapOf(goKeyType, goValueType)
	}

	if typeAlias, ok := type_.(*cffi.CFFIFieldTypeHolder_TypeAliasType); ok {
		name := typeAlias.TypeAliasType.Name.Name
		namespace := typeAlias.TypeAliasType.Name.Namespace.String()
		goType, ok := typeMap.typeMap[namespace+"."+name]
		if !ok {
			panic("error decoding value, type alias not found: " + namespace + "." + name)
		}
		return goType
	}

	// any is weird in go, (alias for interface{})
	if _, ok := type_.(*cffi.CFFIFieldTypeHolder_NullType); ok {
		if _, ok := typeMap.typeMap["INTERNAL.nil"]; ok {
			return reflect.TypeOf((*interface{})(nil)).Elem()
		}
		return reflect.TypeOf((*interface{})(nil))
	}
	if _, ok := type_.(*cffi.CFFIFieldTypeHolder_AnyType); ok {
		return reflect.TypeOf((*interface{})(nil)).Elem()
	}

	panic("error decoding value, unknown field type: " + fmt.Sprintf("%+v", fieldType))
}

func typeToString(fieldType *cffi.CFFIFieldTypeHolder) string {
	if fieldType == nil {
		panic("error decoding value")
	}

	if _, ok := fieldType.Type.(*cffi.CFFIFieldTypeHolder_StringType); ok {
		return "string"
	}
	if _, ok := fieldType.Type.(*cffi.CFFIFieldTypeHolder_BoolType); ok {
		return "bool"
	}
	if _, ok := fieldType.Type.(*cffi.CFFIFieldTypeHolder_IntType); ok {
		return "int"
	}
	if _, ok := fieldType.Type.(*cffi.CFFIFieldTypeHolder_FloatType); ok {
		return "float"
	}
	if literalType, ok := fieldType.Type.(*cffi.CFFIFieldTypeHolder_LiteralType); ok {
		literalType := literalType.LiteralType
		switch literalType.Literal.(type) {
		case *cffi.CFFIFieldTypeLiteral_BoolLiteral:
			literalValue := literalType.Literal.(*cffi.CFFIFieldTypeLiteral_BoolLiteral).BoolLiteral.Value
			if literalValue {
				return "bool_true"
			} else {
				return "bool_false"
			}
		case *cffi.CFFIFieldTypeLiteral_IntLiteral:
			literalValue := literalType.Literal.(*cffi.CFFIFieldTypeLiteral_IntLiteral).IntLiteral.Value
			return "int_literal:" + strconv.FormatInt(literalValue, 10)
		case *cffi.CFFIFieldTypeLiteral_StringLiteral:
			literalValue := literalType.Literal.(*cffi.CFFIFieldTypeLiteral_StringLiteral).StringLiteral.Value
			// replace all non-alphanumeric characters with an underscore
			safeLiteralValue := strings.ReplaceAll(literalValue, "[^a-zA-Z0-9]", "_")
			return "string_" + safeLiteralValue
		default:
			panic("error decoding value, unknown literal type: " + fmt.Sprintf("%+v", literalType.Literal))
		}
	}
	if _, ok := fieldType.Type.(*cffi.CFFIFieldTypeHolder_ClassType); ok {
		return "class"
	}
	if enumType, ok := fieldType.Type.(*cffi.CFFIFieldTypeHolder_EnumType); ok {
		enumType := enumType.EnumType
		enumName := enumType.Name
		return enumName
	}
	if _, ok := fieldType.Type.(*cffi.CFFIFieldTypeHolder_UnionVariantType); ok {
		return "union"
	}
	if _, ok := fieldType.Type.(*cffi.CFFIFieldTypeHolder_OptionalType); ok {
		return "optional"
	}
	if _, ok := fieldType.Type.(*cffi.CFFIFieldTypeHolder_CheckedType); ok {
		return "checked"
	}
	if _, ok := fieldType.Type.(*cffi.CFFIFieldTypeHolder_StreamStateType); ok {
		return "stream_state"
	}
	if _, ok := fieldType.Type.(*cffi.CFFIFieldTypeHolder_ListType); ok {
		return "list"
	}
	if _, ok := fieldType.Type.(*cffi.CFFIFieldTypeHolder_MapType); ok {
		return "map"
	}
	if _, ok := fieldType.Type.(*cffi.CFFIFieldTypeHolder_TypeAliasType); ok {
		return "type_alias"
	}
	if _, ok := fieldType.Type.(*cffi.CFFIFieldTypeHolder_NullType); ok {
		return "null"
	}
	if _, ok := fieldType.Type.(*cffi.CFFIFieldTypeHolder_AnyType); ok {
		return "any"
	}
	panic("error decoding value, unknown field type: " + fmt.Sprintf("%+v", fieldType))
}

func Decode(holder *cffi.CFFIValueHolder, typeMap TypeMap) (reflect.Value, reflect.Type) {
	value := holder.Value
	debugLog("Decode: holder=%+v\n", value)

	switch value := value.(type) {
	case *cffi.CFFIValueHolder_NullValue:
		goType := typeMap.typeMap["INTERNAL.nil"]
		return reflect.ValueOf(nil), goType
	case *cffi.CFFIValueHolder_StringValue:
		val := reflect.ValueOf(value.StringValue)
		return val, reflect.TypeOf("")
	case *cffi.CFFIValueHolder_IntValue:
		val := reflect.ValueOf(value.IntValue)
		return val, reflect.TypeOf(int64(0))
	case *cffi.CFFIValueHolder_FloatValue:
		val := reflect.ValueOf(value.FloatValue)
		return val, reflect.TypeOf(float64(0))
	case *cffi.CFFIValueHolder_BoolValue:
		val := reflect.ValueOf(value.BoolValue)
		return val, reflect.TypeOf(false)
	case *cffi.CFFIValueHolder_ClassValue:
		return decodeClassValue(value.ClassValue, typeMap)
	case *cffi.CFFIValueHolder_EnumValue:
		return decodeEnumValue(value.EnumValue, typeMap)
	case *cffi.CFFIValueHolder_ListValue:
		return decodeListValue(value.ListValue, typeMap)
	case *cffi.CFFIValueHolder_MapValue:
		return decodeMapValue(value.MapValue, typeMap)
	case *cffi.CFFIValueHolder_UnionVariantValue:
		return decodeUnionValue(value.UnionVariantValue, typeMap)
	case *cffi.CFFIValueHolder_CheckedValue:
		return decodeCheckedValue(value.CheckedValue, typeMap)
	case *cffi.CFFIValueHolder_StreamingStateValue:
		return decodeStreamingStateValue(value.StreamingStateValue, typeMap)
	case *cffi.CFFIValueHolder_LiteralValue:
		return decodeLiteralValue(value.LiteralValue, typeMap)
	case *cffi.CFFIValueHolder_ObjectValue:
		panic("ObjectValue is not yet supported: " + holder.String())
	default:
		panic("error decoding value: " + holder.String())
	}
}

func decodeLiteralValue(valueLiteral *cffi.CFFIFieldTypeLiteral, _ TypeMap) (reflect.Value, reflect.Type) {
	if valueLiteral == nil {
		panic("decodeLiteralValue: valueLiteral is nil")
	}

	switch value := valueLiteral.Literal.(type) {
	case *cffi.CFFIFieldTypeLiteral_BoolLiteral:
		return reflect.ValueOf(value.BoolLiteral.Value), reflect.TypeOf(false)
	case *cffi.CFFIFieldTypeLiteral_IntLiteral:
		return reflect.ValueOf(value.IntLiteral.Value), reflect.TypeOf(int64(0))
	case *cffi.CFFIFieldTypeLiteral_StringLiteral:
		return reflect.ValueOf(value.StringLiteral.Value), reflect.TypeOf("")
	default:
		panic("error decoding value, unknown literal type: " + fmt.Sprintf("%+v", value))
	}
}

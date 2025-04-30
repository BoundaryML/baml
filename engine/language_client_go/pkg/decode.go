package baml

import (
	"encoding/json"
	"fmt"
	"reflect"

	"github.com/boundaryml/baml/engine/language_client_go/pkg/cffi"
	flatbuffers "github.com/google/flatbuffers/go"
)

type TypeMap map[string]reflect.Type

type BamlClassDeserializer interface {
	Decode(holder cffi.CFFIValueClass)
}

type BamlEnumDeserializer interface {
	Decode(holder cffi.CFFIValueEnum)
}

type BamlUnionDeserializer interface {
	Decode(holder *cffi.CFFIValueUnionVariant)
}

type cffiValue[U any] interface {
	Init(buf []byte, i flatbuffers.UOffsetT)
	Value() U
}

type DynamicClass struct {
	Name   string
	Fields map[string]any
}

func (d *DynamicClass) Decode(holder cffi.CFFIValueClass) {
	d.Name = string(holder.Name())
	if holder.FieldsLength() > 0 {
		panic("error decoding value")
	}
	fieldCount := holder.DynamicFieldsLength()
	d.Fields = make(map[string]any, fieldCount)
	for i := 0; i < fieldCount; i++ {
		var field cffi.CFFIMapEntry
		if holder.DynamicFields(&field, i) {
			key := string(field.Key())
			valueHolder := field.Value(nil)
			d.Fields[key] = Decode(valueHolder)
		} else {
			panic("error decoding value")
		}
	}
}

type DynamicEnum struct {
	Name  string
	Value string
}

func (d *DynamicEnum) Decode(holder cffi.CFFIValueEnum) {
	d.Name = string(holder.Name())
	d.Value = string(holder.Value())
}

func decodePrimitiveValue[U any, T cffiValue[U]](valueHolder *cffi.CFFIValueHolder, t T) U {
	var tbl flatbuffers.Table
	if !valueHolder.Value(&tbl) {
		panic("error decoding value")
	}

	t.Init(tbl.Bytes, tbl.Pos)
	return t.Value()
}

func decodeListValue(valueHolder *cffi.CFFIValueHolder) any {
	var tbl flatbuffers.Table
	if !valueHolder.Value(&tbl) {
		panic("error decoding value")
	}

	var valueList cffi.CFFIValueList
	valueList.Init(tbl.Bytes, tbl.Pos)

	fieldType := valueList.FieldType(nil)
	if fieldType.TypeType() != cffi.CFFIFieldTypeUnionCFFIFieldTypeList {
		panic("error decoding value, expected list got " + fieldType.TypeType().String())
	}

	var listFieldTable flatbuffers.Table
	if !fieldType.Type(&listFieldTable) {
		panic("error decoding value")
	}

	var listFieldType cffi.CFFIFieldTypeList
	listFieldType.Init(listFieldTable.Bytes, listFieldTable.Pos)

	elementType := listFieldType.Element(nil)

	goElementType := convertFieldTypeToGoType(elementType)

	length := valueList.ValuesLength()
	values := reflect.MakeSlice(reflect.SliceOf(goElementType), length, length)

	for i := 0; i < length; i++ {
		var value cffi.CFFIValueHolder
		if valueList.Values(&value, i) {
			rv := reflect.ValueOf(Decode(&value))
			if rv.Kind() == reflect.Ptr {
				rv = rv.Elem()
			}
			values.Index(i).Set(rv)
		} else {
			panic("error decoding value")
		}
	}
	return values.Interface()
}

func decodeMapValue(valueHolder *cffi.CFFIValueHolder) any {
	var tbl flatbuffers.Table
	if !valueHolder.Value(&tbl) {
		panic("error decoding value")
	}
	var valueMap cffi.CFFIValueMap
	valueMap.Init(tbl.Bytes, tbl.Pos)

	fieldTypes := valueMap.FieldTypes(nil)
	if fieldTypes.TypeType() != cffi.CFFIFieldTypeUnionCFFIFieldTypeMap {
		panic("error decoding value")
	}

	var mapFieldTable flatbuffers.Table
	if !fieldTypes.Type(&mapFieldTable) {
		panic("error decoding value")
	}
	var mapFieldType cffi.CFFIFieldTypeMap
	mapFieldType.Init(mapFieldTable.Bytes, mapFieldTable.Pos)

	keyType := mapFieldType.Key(nil)
	valueType := mapFieldType.Value(nil)

	goKeyType := convertFieldTypeToGoType(keyType)
	goValueType := convertFieldTypeToGoType(valueType)

	values := reflect.MakeMap(reflect.MapOf(goKeyType, goValueType))

	length := valueMap.EntriesLength()
	for i := 0; i < length; i++ {
		var value cffi.CFFIMapEntry
		if valueMap.Entries(&value, i) {
			key := string(value.Key())
			valueHolder := value.Value(nil)

			rv := reflect.ValueOf(Decode(valueHolder))
			if rv.Kind() == reflect.Ptr {
				rv = rv.Elem()
			}
			values.SetMapIndex(reflect.ValueOf(key), rv)
		} else {
			panic("error decoding value")
		}
	}
	return values.Interface()
}

type BamlDecoder interface {
	BamlDecode(decodedMap map[string]any)
}

func decodeClassValue(valueHolder *cffi.CFFIValueHolder) any {
	var tbl flatbuffers.Table
	if !valueHolder.Value(&tbl) {
		panic("error decoding value")
	}
	var valueClass cffi.CFFIValueClass
	valueClass.Init(tbl.Bytes, tbl.Pos)

	found, ok := typeMap[string(valueClass.Name())]
	if !ok {
		// This is a fully dynamic class, so we need to decode it as a map
		dynamicClass := DynamicClass{}
		dynamicClass.Decode(valueClass)
		return &dynamicClass
	}

	cls := reflect.New(found)
	as_interface := cls.Interface().(BamlClassDeserializer)
	as_interface.Decode(valueClass)
	return as_interface
}

func decodeEnumValue(valueHolder *cffi.CFFIValueHolder) any {
	var tbl flatbuffers.Table
	if !valueHolder.Value(&tbl) {
		panic("error decoding value")
	}
	var valueEnum cffi.CFFIValueEnum
	valueEnum.Init(tbl.Bytes, tbl.Pos)

	enumName := string(valueEnum.Name())
	found, ok := typeMap[enumName]
	if !ok {
		return &DynamicEnum{Name: enumName, Value: string(valueEnum.Value())}
	}
	enum := reflect.New(found)
	as_interface := enum.Interface().(BamlEnumDeserializer)
	as_interface.Decode(valueEnum)
	return as_interface
}

func decodeUnionValue(holder *cffi.CFFIValueHolder) any {
	var tbl flatbuffers.Table
	if !holder.Value(&tbl) {
		panic("error decoding value")
	}

	var valueUnion cffi.CFFIValueUnionVariant
	valueUnion.Init(tbl.Bytes, tbl.Pos)

	found, ok := typeMap[string(valueUnion.Name())]
	if !ok {
		// This is a fully dynamic union, so we
		// decode the value as the value and drop
		// union type information
		value := valueUnion.Value(nil)
		return Decode(value)
	}
	union := reflect.New(found)
	as_interface := union.Interface().(BamlUnionDeserializer)
	as_interface.Decode(&valueUnion)
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

func decodeCheckedValue(holder *cffi.CFFIValueHolder) Checked[any] {
	var tbl flatbuffers.Table
	if !holder.Value(&tbl) {
		panic("error decoding value")
	}
	var valueChecked cffi.CFFIValueChecked
	valueChecked.Init(tbl.Bytes, tbl.Pos)

	value := valueChecked.Value(nil)
	checksLength := valueChecked.ChecksLength()
	checks := make(map[string]Check, checksLength)
	for i := 0; i < checksLength; i++ {
		var check cffi.CFFICheckValue
		if valueChecked.Checks(&check, i) {
			panic("check not implemented")
		}
		checks[string(check.Name())] = Check{
			Name:       string(check.Name()),
			Expression: string(check.Expression()),
			Status:     string(check.Status()),
		}
	}
	return Checked[any]{
		Value:  Decode(value),
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
	case cffi.CFFIStreamStatePending:
		return StreamStatePending
	case cffi.CFFIStreamStateStarted:
		return StreamStateIncomplete
	case cffi.CFFIStreamStateDone:
		return StreamStateComplete
	default:
		panic("unexpected stream state")
	}
}

func decodeStreamingStateValue(holder *cffi.CFFIValueHolder) StreamState[any] {
	var tbl flatbuffers.Table
	if !holder.Value(&tbl) {
		panic("error decoding value")
	}
	var valueStreamingState cffi.CFFIValueStreamingState
	valueStreamingState.Init(tbl.Bytes, tbl.Pos)

	value := valueStreamingState.Value(nil)

	return StreamState[any]{
		Value: Decode(value),
		State: decodeStreamStateType(valueStreamingState.State()),
	}
}

func convertFieldTypeToGoType(fieldType *cffi.CFFIFieldTypeHolder) reflect.Type {
	switch fieldType.TypeType() {
	case cffi.CFFIFieldTypeUnionCFFIFieldTypeString:
		return reflect.TypeOf("")
	case cffi.CFFIFieldTypeUnionCFFIFieldTypeInt:
		var i int64
		return reflect.TypeOf(i)
	case cffi.CFFIFieldTypeUnionCFFIFieldTypeFloat:
		var f float64
		return reflect.TypeOf(f)
	case cffi.CFFIFieldTypeUnionCFFIFieldTypeBool:
		return reflect.TypeOf(false)
	case cffi.CFFIFieldTypeUnionCFFIFieldTypeClass:
		var classTable flatbuffers.Table
		if !fieldType.Type(&classTable) {
			panic("error decoding value")
		}

		var classType cffi.CFFIFieldTypeClass
		classType.Init(classTable.Bytes, classTable.Pos)

		goType, ok := typeMap[string(classType.Name())]
		if !ok {
			panic("error decoding value")
		}

		return goType
	default:
		panic(fmt.Sprintf("unexpected field type %d", fieldType.TypeType()))
	}
}

func Decode(holder *cffi.CFFIValueHolder) any {
	valueType := holder.ValueType()
	switch valueType {
	case cffi.CFFIValueUnionNONE:
		return nil
	case cffi.CFFIValueUnionCFFIValueString:
		valueString := decodePrimitiveValue(holder, &cffi.CFFIValueString{})
		return string(valueString)
	case cffi.CFFIValueUnionCFFIValueInt:
		return decodePrimitiveValue(holder, &cffi.CFFIValueInt{})
	case cffi.CFFIValueUnionCFFIValueFloat:
		return decodePrimitiveValue(holder, &cffi.CFFIValueFloat{})
	case cffi.CFFIValueUnionCFFIValueBool:
		return decodePrimitiveValue(holder, &cffi.CFFIValueBool{})
	case cffi.CFFIValueUnionCFFIValueList:
		return decodeListValue(holder)
	case cffi.CFFIValueUnionCFFIValueMap:
		return decodeMapValue(holder)
	case cffi.CFFIValueUnionCFFIValueClass:
		return decodeClassValue(holder)
	case cffi.CFFIValueUnionCFFIValueEnum:
		return decodeEnumValue(holder)
	case cffi.CFFIValueUnionCFFIValueUnionVariant:
		return decodeUnionValue(holder)
	case cffi.CFFIValueUnionCFFIValueChecked:
		return decodeCheckedValue(holder)
	case cffi.CFFIValueUnionCFFIValueStreamingState:
		return decodeStreamingStateValue(holder)
	case cffi.CFFIValueUnionCFFIValueMedia:
		panic("media not implemented")
	case cffi.CFFIValueUnionCFFIValueTuple:
		panic("tuple not implemented")
	case cffi.CFFIValueUnionCFFIFunctionArguments:
		panic("function arguments are never decoded")
	default:
		panic("unexpected value type")
	}
}

func DecodeOptional[T any](valueHolder *cffi.CFFIValueHolder, decodeFunc func(*cffi.CFFIValueHolder) T) *T {
	value := Decode(valueHolder)
	if value == nil {
		return nil
	}
	return value.(*T)
}

func DecodeList[T any](valueHolder *cffi.CFFIValueHolder, decodeFunc func(*cffi.CFFIValueHolder) T) []T {
	var tbl flatbuffers.Table
	if !valueHolder.Value(&tbl) {
		panic("error decoding value")
	}

	var valueList cffi.CFFIValueList
	valueList.Init(tbl.Bytes, tbl.Pos)
	length := valueList.ValuesLength()
	values := make([]T, length)
	for i := range length {
		var value cffi.CFFIValueHolder
		if valueList.Values(&value, i) {
			values[i] = decodeFunc(&value)
		} else {
			panic("error decoding value")
		}
	}
	return values
}

func DecodeMap[T any](valueHolder cffi.CFFIValueHolder, decodeFunc func(*cffi.CFFIValueHolder, TypeMap) T) map[string]T {
	var tbl flatbuffers.Table
	if !valueHolder.Value(&tbl) {
		panic("error decoding value")
	}

	var valueMap cffi.CFFIValueMap
	valueMap.Init(tbl.Bytes, tbl.Pos)
	length := valueMap.EntriesLength()
	values := make(map[string]T)
	for i := range length {
		var value cffi.CFFIMapEntry
		if valueMap.Entries(&value, i) {
			key := string(value.Key())
			valueHolder := value.Value(nil)
			values[key] = decodeFunc(valueHolder, typeMap)
		} else {
			panic("error decoding value")
		}
	}
	return values
}

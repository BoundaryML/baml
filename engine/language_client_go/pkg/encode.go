package baml

import (
	"fmt"
	"reflect"

	"github.com/boundaryml/baml/engine/language_client_go/pkg/cffi"
	flatbuffers "github.com/google/flatbuffers/go"
)

// BamlSerializer interface for custom class encoding
type BamlSerializer interface {
	Encode(builder *flatbuffers.Builder) (cffi.CFFIValueUnion, flatbuffers.UOffsetT, error)
	BamlTypeName() string
}

// implment BamlSerializer for anything that implements BamlClassSerializer, BamlEnumSerializer, or BamlUnionSerializer
func EncodeClass(builder *flatbuffers.Builder, name string, fields map[string]any, dynamicFields *map[string]any) (valueType cffi.CFFIValueUnion, offset flatbuffers.UOffsetT, err error) {

	nameOffset := builder.CreateString(name)

	// Encode Static Fields
	var staticFieldsVectorOffset flatbuffers.UOffsetT
	if len(fields) > 0 {
		staticFieldsVectorOffset, err = encodeMapEntries(builder, fields, "static class")
		if err != nil {
			return cffi.CFFIValueUnionNONE, 0, err // Error already includes context
		}
	}

	// Encode Dynamic Fields
	var dynamicFieldsVectorOffset flatbuffers.UOffsetT
	if dynamicFields != nil && len(*dynamicFields) > 0 {
		dynamicFieldsVectorOffset, err = encodeMapEntries(builder, *dynamicFields, "dynamic class")
		if err != nil {
			return cffi.CFFIValueUnionNONE, 0, err // Error already includes context
		}
	}

	// Create the CFFIValueClass table
	cffi.CFFIValueClassStart(builder)
	cffi.CFFIValueClassAddName(builder, nameOffset)

	// Add vectors only if they were created (offset > 0)
	if staticFieldsVectorOffset > 0 {
		cffi.CFFIValueClassAddFields(builder, staticFieldsVectorOffset)
	}
	if dynamicFieldsVectorOffset > 0 {
		cffi.CFFIValueClassAddDynamicFields(builder, dynamicFieldsVectorOffset)
	}

	return cffi.CFFIValueUnionCFFIValueClass, cffi.CFFIValueClassEnd(builder), nil
}

func EncodeEnum(builder *flatbuffers.Builder, name string, value string, isDynamic bool) (cffi.CFFIValueUnion, flatbuffers.UOffsetT, error) {
	nameOffset := builder.CreateString(name)
	valueOffset := builder.CreateString(value)

	cffi.CFFIValueEnumStart(builder)
	cffi.CFFIValueEnumAddName(builder, nameOffset)
	cffi.CFFIValueEnumAddValue(builder, valueOffset)
	cffi.CFFIValueEnumAddIsDynamic(builder, isDynamic) // Set based on input
	return cffi.CFFIValueUnionCFFIValueEnum, cffi.CFFIValueEnumEnd(builder), nil
}

func EncodeUnion(builder *flatbuffers.Builder, name string, variantName string, value any) (cffi.CFFIValueUnion, flatbuffers.UOffsetT, error) {
	nameOffset := builder.CreateString(name)
	variantNameOffset := builder.CreateString(variantName)
	valueHolderOffset, err := Encode(builder, value)
	if err != nil {
		return cffi.CFFIValueUnionNONE, 0, fmt.Errorf("encoding inner value for union variant '%s': %w", variantName, err)
	}

	cffi.CFFIValueUnionVariantStart(builder)
	cffi.CFFIValueUnionVariantAddName(builder, nameOffset)
	cffi.CFFIValueUnionVariantAddVariantName(builder, variantNameOffset)
	cffi.CFFIValueUnionVariantAddValue(builder, valueHolderOffset)
	// cffi.CFFIValueUnionVariantAddFieldTypes(builder, ...) // Omitted
	// cffi.CFFIValueUnionVariantAddValueTypeIndex(builder, ...) // Omitted
	return cffi.CFFIValueUnionCFFIValueUnionVariant, cffi.CFFIValueUnionVariantEnd(builder), nil
}

// encodeValue is the core recursive helper for Encode
// It takes a Go value, encodes it using the builder, and returns:
// 1. The CFFIValueUnion type enum value matching the encoded type.
// 2. The FlatBuffers offset (UOffsetT) of the encoded value table (e.g., CFFIValueString).
// 3. An error if encoding fails.
func encodeValue(builder *flatbuffers.Builder, value any) (cffi.CFFIValueUnion, flatbuffers.UOffsetT, error) {
	if value == nil {
		// For nil, we don't create a specific value table.
		// The CFFIValueHolder will have ValueType=NONE and Value=0.
		return cffi.CFFIValueUnionNONE, 0, nil
	}

	// Use reflection to determine the type of the value
	rv := reflect.ValueOf(value)
	originalValue := value // Keep track of the original for interface checks, might be pointer

	// Handle Pointers: Dereference non-nil pointers for kind checks, but use original for interfaces
	if rv.Kind() == reflect.Ptr {
		if rv.IsNil() {
			return cffi.CFFIValueUnionNONE, 0, nil // Treat nil pointers as nil values
		}
		// Work with the pointed-to value for subsequent kind checks
		rv = rv.Elem()
		// Update value variable ONLY if we are sure we won't need the pointer for interface checks later
		// value = rv.Interface() // Let's keep originalValue for interface checks
	}

	// Handle concrete types (Checked, StreamState) before general kinds
	// Use the potentially dereferenced value 'rv.Interface()' here if concrete types are structs
	concreteValue := rv.Interface() // Get the concrete value (dereferenced if original was pointer)

	// Check for custom serializers first using the original value (could be pointer or value)
	if serializer, ok := originalValue.(BamlSerializer); ok {
		valueType, offset, err := serializer.Encode(builder)
		if err != nil {
			return cffi.CFFIValueUnionNONE, 0, err
		}
		return valueType, offset, nil
	}

	switch v := concreteValue.(type) {
	case Checked[any]: // Use any here, or make encodeValue generic (more complex)
		offset, err := encodeChecked(builder, v)
		if err != nil {
			return cffi.CFFIValueUnionNONE, 0, fmt.Errorf("encoding Checked value: %w", err)
		}
		return cffi.CFFIValueUnionCFFIValueChecked, offset, nil
	case StreamState[any]: // Use any here
		offset, err := encodeStreamState(builder, v) // Pass typeMap
		if err != nil {
			return cffi.CFFIValueUnionNONE, 0, fmt.Errorf("encoding StreamState value: %w", err)
		}
		return cffi.CFFIValueUnionCFFIValueStreamingState, offset, nil
	case BamlFunctionArguments:
		offset, err := encodeFunctionArguments(builder, v)
		if err != nil {
			return cffi.CFFIValueUnionNONE, 0, fmt.Errorf("encoding function arguments: %w", err)
		}
		return cffi.CFFIValueUnionCFFIFunctionArguments, offset, nil
		// Add cases for Media and Tuple if implemented
		// case MyMediaStruct: ... return cffi.CFFIValueUnionCFFIValueMedia, offset, nil
		// case MyTupleStruct: ... return cffi.CFFIValueUnionCFFIValueTuple, offset, nil
	}

	// Handle primitive kinds and collections using reflection value rv (points to underlying value)
	switch rv.Kind() {
	case reflect.String:
		offset := encodeString(builder, rv.String())
		return cffi.CFFIValueUnionCFFIValueString, offset, nil

	case reflect.Int, reflect.Int8, reflect.Int16, reflect.Int32, reflect.Int64:
		offset := encodeInt(builder, rv.Int())
		return cffi.CFFIValueUnionCFFIValueInt, offset, nil

	case reflect.Float32, reflect.Float64:
		offset := encodeFloat(builder, rv.Float())
		return cffi.CFFIValueUnionCFFIValueFloat, offset, nil

	case reflect.Bool:
		offset := encodeBool(builder, rv.Bool())
		return cffi.CFFIValueUnionCFFIValueBool, offset, nil

	case reflect.Slice, reflect.Array:
		offset, err := encodeList(builder, rv)
		if err != nil {
			return cffi.CFFIValueUnionNONE, 0, fmt.Errorf("encoding list: %w", err)
		}
		return cffi.CFFIValueUnionCFFIValueList, offset, nil

	case reflect.Map:
		if rv.Type().Key().Kind() != reflect.String {
			return cffi.CFFIValueUnionNONE, 0, fmt.Errorf("map key type must be string, got %s", rv.Type().Key().Kind())
		}

		offset, err := encodeMap(builder, rv)
		if err != nil {
			return cffi.CFFIValueUnionNONE, 0, fmt.Errorf("encoding map: %w", err)
		}
		return cffi.CFFIValueUnionCFFIValueMap, offset, nil

	default:
		// Use originalValue's type for the error message as it's more accurate to the input
		return cffi.CFFIValueUnionNONE, 0, fmt.Errorf("unsupported type for BAML encoding: %T (Kind: %s)", originalValue, rv.Kind())
	}
}

// --- Encoding helpers for specific types ---

// encodeString remains the same (no nested types)
func encodeString(builder *flatbuffers.Builder, val string) flatbuffers.UOffsetT {
	strOffset := builder.CreateString(val)
	cffi.CFFIValueStringStart(builder)
	cffi.CFFIValueStringAddValue(builder, strOffset)
	return cffi.CFFIValueStringEnd(builder)
}

// encodeInt remains the same
func encodeInt(builder *flatbuffers.Builder, val int64) flatbuffers.UOffsetT {
	cffi.CFFIValueIntStart(builder)
	cffi.CFFIValueIntAddValue(builder, val)
	return cffi.CFFIValueIntEnd(builder)
}

// encodeFloat remains the same
func encodeFloat(builder *flatbuffers.Builder, val float64) flatbuffers.UOffsetT {
	cffi.CFFIValueFloatStart(builder)
	cffi.CFFIValueFloatAddValue(builder, val)
	return cffi.CFFIValueFloatEnd(builder)
}

// encodeBool remains the same
func encodeBool(builder *flatbuffers.Builder, val bool) flatbuffers.UOffsetT {
	cffi.CFFIValueBoolStart(builder)
	cffi.CFFIValueBoolAddValue(builder, val)
	return cffi.CFFIValueBoolEnd(builder)
}

// encodeList now accepts and passes TypeMap
func encodeList(builder *flatbuffers.Builder, value reflect.Value) (flatbuffers.UOffsetT, error) {
	elemOffsets := make([]flatbuffers.UOffsetT, value.Len())
	for i := value.Len() - 1; i >= 0; i-- { // Build elements backwards for FlatBuffers vector
		elemOffset, err := Encode(builder, value.Index(i).Interface()) // Pass typeMap recursively
		if err != nil {
			return 0, fmt.Errorf("encoding list element %d: %w", i, err)
		}
		elemOffsets[i] = elemOffset
	}

	// Create the vector of CFFIValueHolder offsets
	cffi.CFFIValueListStartValuesVector(builder, len(elemOffsets))
	for i := len(elemOffsets) - 1; i >= 0; i-- {
		builder.PrependUOffsetT(elemOffsets[i])
	}
	valuesVectorOffset := builder.EndVector(len(elemOffsets))

	fieldTypeOffset := encodeFieldType(builder, value.Type())

	// Create the CFFIValueList table
	cffi.CFFIValueListStart(builder)
	cffi.CFFIValueListAddValues(builder, valuesVectorOffset)
	cffi.CFFIValueListAddFieldType(builder, fieldTypeOffset)
	return cffi.CFFIValueListEnd(builder), nil
}

// encodeMap now accepts and passes TypeMap
func encodeMap(builder *flatbuffers.Builder, mapValue reflect.Value) (flatbuffers.UOffsetT, error) {

	mapLength := mapValue.Len()
	entryOffsets := make([]flatbuffers.UOffsetT, 0, mapLength)
	// Iterate map and build entries (order doesn't strictly matter for map, but FB requires building bottom-up)
	mapIter := mapValue.MapRange()
	for mapIter.Next() {
		key := mapIter.Key().String()
		value := mapIter.Value()
		keyOffset := builder.CreateString(key)
		valueHolderOffset, err := Encode(builder, value.Interface())
		if err != nil {
			return 0, fmt.Errorf("encoding map value for key '%s': %w", key, err)
		}

		cffi.CFFIMapEntryStart(builder)
		cffi.CFFIMapEntryAddKey(builder, keyOffset)
		cffi.CFFIMapEntryAddValue(builder, valueHolderOffset)
		entryOffset := cffi.CFFIMapEntryEnd(builder)
		entryOffsets = append(entryOffsets, entryOffset)
	}

	// Create the vector of CFFIMapEntry offsets
	cffi.CFFIValueMapStartEntriesVector(builder, len(entryOffsets))
	// Add entries in reverse order of creation
	for i := len(entryOffsets) - 1; i >= 0; i-- {
		builder.PrependUOffsetT(entryOffsets[i])
	}
	entriesVectorOffset := builder.EndVector(len(entryOffsets))

	fieldTypeOffset := encodeFieldType(builder, mapValue.Type())

	// Create the CFFIValueMap table
	cffi.CFFIValueMapStart(builder)
	cffi.CFFIValueMapAddFieldTypes(builder, fieldTypeOffset)
	cffi.CFFIValueMapAddEntries(builder, entriesVectorOffset)
	return cffi.CFFIValueMapEnd(builder), nil
}

// Helper function to encode map entries into a vector offset
func encodeMapEntries(builder *flatbuffers.Builder, fields map[string]any, context string) (flatbuffers.UOffsetT, error) {
	if len(fields) == 0 {
		return 0, nil // Return 0 offset for empty vector
	}

	entryOffsets := make([]flatbuffers.UOffsetT, 0, len(fields))
	// Build entries (order doesn't strictly matter, but need to build bottom-up)
	for k, v := range fields {
		keyOffset := builder.CreateString(k)
		valueHolderOffset, err := Encode(builder, v)
		if err != nil {
			return 0, fmt.Errorf("encoding %s field '%s': %w", context, k, err)
		}

		cffi.CFFIMapEntryStart(builder)
		cffi.CFFIMapEntryAddKey(builder, keyOffset)
		cffi.CFFIMapEntryAddValue(builder, valueHolderOffset)
		entryOffset := cffi.CFFIMapEntryEnd(builder)
		entryOffsets = append(entryOffsets, entryOffset)
	}

	// Create the vector of CFFIMapEntry offsets
	// Determine the correct Start*Vector function based on context if needed, assume generic for now
	// For CFFIValueClass, the specific functions are CFFIValueClassStartFieldsVector and CFFIValueClassStartDynamicFieldsVector
	// This helper is generic, so we use the general vector building approach. The caller uses the specific Start*Vector.
	numEntries := len(entryOffsets)
	builder.StartVector(4, numEntries, 4) // 4 bytes per UOffsetT
	for i := numEntries - 1; i >= 0; i-- {
		builder.PrependUOffsetT(entryOffsets[i])
	}
	return builder.EndVector(numEntries), nil
}

// encodeChecked now accepts and passes TypeMap
func encodeChecked(builder *flatbuffers.Builder, checkedVal Checked[any]) (flatbuffers.UOffsetT, error) {
	valueHolderOffset, err := Encode(builder, checkedVal.Value)
	if err != nil {
		return 0, fmt.Errorf("encoding inner value for Checked: %w", err)
	}

	checkOffsets := make([]flatbuffers.UOffsetT, 0, len(checkedVal.Checks))
	for _, check := range checkedVal.Checks {
		nameOffset := builder.CreateString(check.Name)
		exprOffset := builder.CreateString(check.Expression)
		statusOffset := builder.CreateString(check.Status)
		// Encode the check's inner value (if the schema requires it and Check struct holds it)
		// Assuming CFFICheckValue doesn't hold another value based on Check struct definition
		// checkValueHolderOffset, err := Encode(builder, check.Value, typeMap) // Would pass typeMap here too
		// if err != nil { ... }

		cffi.CFFICheckValueStart(builder)
		cffi.CFFICheckValueAddName(builder, nameOffset)
		cffi.CFFICheckValueAddExpression(builder, exprOffset)
		cffi.CFFICheckValueAddStatus(builder, statusOffset)
		// cffi.CFFICheckValueAddValue(builder, checkValueHolderOffset) // Add if needed
		checkOffset := cffi.CFFICheckValueEnd(builder)
		checkOffsets = append(checkOffsets, checkOffset)
	}

	var checksVectorOffset flatbuffers.UOffsetT
	if len(checkOffsets) > 0 {
		cffi.CFFIValueCheckedStartChecksVector(builder, len(checkOffsets))
		for i := len(checkOffsets) - 1; i >= 0; i-- {
			builder.PrependUOffsetT(checkOffsets[i])
		}
		checksVectorOffset = builder.EndVector(len(checkOffsets))
	}

	cffi.CFFIValueCheckedStart(builder)
	cffi.CFFIValueCheckedAddValue(builder, valueHolderOffset)
	if len(checkOffsets) > 0 {
		cffi.CFFIValueCheckedAddChecks(builder, checksVectorOffset)
	}
	return cffi.CFFIValueCheckedEnd(builder), nil
}

// encodeStreamStateType remains the same
func encodeStreamStateType(state StreamStateType) cffi.CFFIStreamState {
	switch state {
	case StreamStatePending:
		return cffi.CFFIStreamStatePending
	case StreamStateIncomplete:
		return cffi.CFFIStreamStateStarted
	case StreamStateComplete:
		return cffi.CFFIStreamStateDone
	default:
		panic(fmt.Sprintf("unexpected Go stream state: %s", state))
	}
}

// encodeStreamState now accepts and passes TypeMap
func encodeStreamState(builder *flatbuffers.Builder, streamStateVal StreamState[any]) (flatbuffers.UOffsetT, error) {
	valueHolderOffset, err := Encode(builder, streamStateVal.Value) // Pass typeMap
	if err != nil {
		return 0, fmt.Errorf("encoding inner value for StreamState: %w", err)
	}

	stateEnum := encodeStreamStateType(streamStateVal.State)

	cffi.CFFIValueStreamingStateStart(builder)
	cffi.CFFIValueStreamingStateAddValue(builder, valueHolderOffset)
	cffi.CFFIValueStreamingStateAddState(builder, stateEnum)
	return cffi.CFFIValueStreamingStateEnd(builder), nil
}

func encodeFunctionArguments(builder *flatbuffers.Builder, functionArgumentsVal BamlFunctionArguments) (flatbuffers.UOffsetT, error) {
	kwargsOffset, err := encodeMapEntries(builder, functionArgumentsVal.Kwargs, "function arguments")
	if err != nil {
		return 0, fmt.Errorf("encoding function arguments: %w", err)
	}

	cffi.CFFIFunctionArgumentsStart(builder)
	cffi.CFFIFunctionArgumentsAddKwargs(builder, kwargsOffset)
	return cffi.CFFIFunctionArgumentsEnd(builder), nil
}

func encodeFieldType(builder *flatbuffers.Builder, fieldType reflect.Type) flatbuffers.UOffsetT {
	var fieldTypeOffset flatbuffers.UOffsetT
	var fieldTypeUnion cffi.CFFIFieldTypeUnion

	switch fieldType.Kind() {
	case reflect.Ptr:
		return encodeFieldType(builder, fieldType.Elem())
	case reflect.String:
		cffi.CFFIFieldTypeStringStart(builder)
		fieldTypeOffset = cffi.CFFIFieldTypeStringEnd(builder)
		fieldTypeUnion = cffi.CFFIFieldTypeUnionCFFIFieldTypeString
	case reflect.Int, reflect.Int8, reflect.Int16, reflect.Int32, reflect.Int64:
		cffi.CFFIFieldTypeIntStart(builder)
		fieldTypeOffset = cffi.CFFIFieldTypeIntEnd(builder)
		fieldTypeUnion = cffi.CFFIFieldTypeUnionCFFIFieldTypeInt
	case reflect.Float32, reflect.Float64:
		cffi.CFFIFieldTypeFloatStart(builder)
		fieldTypeOffset = cffi.CFFIFieldTypeFloatEnd(builder)
		fieldTypeUnion = cffi.CFFIFieldTypeUnionCFFIFieldTypeFloat
	case reflect.Bool:
		cffi.CFFIFieldTypeBoolStart(builder)
		fieldTypeOffset = cffi.CFFIFieldTypeBoolEnd(builder)
		fieldTypeUnion = cffi.CFFIFieldTypeUnionCFFIFieldTypeBool
	case reflect.Slice, reflect.Array:
		sliceFieldTypeOffset := encodeFieldType(builder, fieldType.Elem())

		cffi.CFFIFieldTypeListStart(builder)
		cffi.CFFIFieldTypeListAddElement(builder, sliceFieldTypeOffset)
		fieldTypeOffset = cffi.CFFIFieldTypeListEnd(builder)
		fieldTypeUnion = cffi.CFFIFieldTypeUnionCFFIFieldTypeList
	case reflect.Map:
		keyType := fieldType.Key()
		valueType := fieldType.Elem()

		keyTypeOffset := encodeFieldType(builder, keyType)
		valueTypeOffset := encodeFieldType(builder, valueType)

		cffi.CFFIFieldTypeMapStart(builder)
		cffi.CFFIFieldTypeMapAddKey(builder, keyTypeOffset)
		cffi.CFFIFieldTypeMapAddValue(builder, valueTypeOffset)
		fieldTypeOffset = cffi.CFFIFieldTypeMapEnd(builder)
		fieldTypeUnion = cffi.CFFIFieldTypeUnionCFFIFieldTypeMap
	case reflect.Struct:
		// determine if the struct implements BamlSerializer
		if fieldType.Implements(reflect.TypeOf((*BamlSerializer)(nil)).Elem()) {
			serializer := reflect.New(fieldType).Interface().(BamlSerializer)
			nameOffset := builder.CreateString(serializer.BamlTypeName())
			cffi.CFFIFieldTypeClassStart(builder)
			cffi.CFFIFieldTypeClassAddName(builder, nameOffset)
			fieldTypeOffset = cffi.CFFIFieldTypeClassEnd(builder)
			fieldTypeUnion = cffi.CFFIFieldTypeUnionCFFIFieldTypeClass
		} else {
			panic(fmt.Sprintf("struct %s does not implement BamlSerializer", fieldType.Name()))
		}
	default:
		panic(fmt.Sprintf("unexpected field type: %s", fieldType.Kind()))
	}

	cffi.CFFIFieldTypeHolderStart(builder)
	cffi.CFFIFieldTypeHolderAddType(builder, fieldTypeOffset)
	cffi.CFFIFieldTypeHolderAddTypeType(builder, fieldTypeUnion)
	return cffi.CFFIFieldTypeHolderEnd(builder)
}

// EncodeRoot now accepts a TypeMap.
// Encode takes a Go value and returns the FlatBuffers encoded bytes for a CFFIValueHolder.
// It creates a new builder internally.
func EncodeRoot(value any) ([]byte, error) {
	builder := flatbuffers.NewBuilder(1024) // Initial buffer size
	rootOffset, err := Encode(builder, value)
	if err != nil {
		return nil, err
	}
	builder.Finish(rootOffset)
	return builder.FinishedBytes(), nil
}

// Encode now accepts and passes TypeMap.
// Encode takes a builder and a Go value, encodes the value, wraps it in a CFFIValueHolder,
// and returns the offset of the holder.
func Encode(builder *flatbuffers.Builder, value any) (flatbuffers.UOffsetT, error) {
	valueType, valueOffset, err := encodeValue(builder, value)
	if err != nil {
		return 0, err // Propagate error
	}

	// Build the CFFIValueHolder table
	cffi.CFFIValueHolderStart(builder)
	cffi.CFFIValueHolderAddValueType(builder, valueType)
	// Only add the value offset if it's not NONE (offset will be 0 for NONE)
	if valueType != cffi.CFFIValueUnionNONE {
		cffi.CFFIValueHolderAddValue(builder, valueOffset)
	}
	holderOffset := cffi.CFFIValueHolderEnd(builder)

	return holderOffset, nil
}

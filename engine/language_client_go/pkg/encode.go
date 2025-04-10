package baml

import (
	"fmt"
	"reflect"

	"github.com/boundaryml/baml/engine/language_client_go/pkg/cffi"
	flatbuffers "github.com/google/flatbuffers/go"
)

// BamlSerializer interface for custom class encoding
type BamlSerializer interface {
	// EncodeBamlClass returns the class name and a map of field names to values.
	// The TypeMap is provided in case the implementation needs it for nested types.
	Encode(builder *flatbuffers.Builder, typeMap TypeMap) (cffi.CFFIValueUnion, flatbuffers.UOffsetT, error)
}

// implment BamlSerializer for anything that implements BamlClassSerializer, BamlEnumSerializer, or BamlUnionSerializer
func EncodeClass(builder *flatbuffers.Builder, typeMap TypeMap,
	name string, fields map[string]any, dynamicFields *map[string]any) (valueType cffi.CFFIValueUnion, offset flatbuffers.UOffsetT, err error) {

	nameOffset := builder.CreateString(name)

	// Encode Static Fields
	var staticFieldsVectorOffset flatbuffers.UOffsetT
	if len(fields) > 0 {
		// Use the specific Start function before creating the vector
		cffi.CFFIValueClassStartFieldsVector(builder, len(fields))
		staticFieldsVectorOffset, err = encodeMapEntries(builder, fields, typeMap, "static class")
		if err != nil {
			return cffi.CFFIValueUnionNONE, 0, err // Error already includes context
		}
	}

	// Encode Dynamic Fields
	var dynamicFieldsVectorOffset flatbuffers.UOffsetT
	if dynamicFields != nil && len(*dynamicFields) > 0 {
		// Use the specific Start function before creating the vector
		cffi.CFFIValueClassStartDynamicFieldsVector(builder, len(*dynamicFields))
		dynamicFieldsVectorOffset, err = encodeMapEntries(builder, *dynamicFields, typeMap, "dynamic class")
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

// encodeEnum doesn't need TypeMap internally, but the interface provides it
func EncodeEnum(builder *flatbuffers.Builder, name string, value string, isDynamic bool) (cffi.CFFIValueUnion, flatbuffers.UOffsetT, error) {
	nameOffset := builder.CreateString(name)
	valueOffset := builder.CreateString(value)

	cffi.CFFIValueEnumStart(builder)
	cffi.CFFIValueEnumAddName(builder, nameOffset)
	cffi.CFFIValueEnumAddValue(builder, valueOffset)
	cffi.CFFIValueEnumAddIsDynamic(builder, isDynamic) // Set based on input
	return cffi.CFFIValueUnionCFFIValueEnum, cffi.CFFIValueEnumEnd(builder), nil
}

// encodeUnion now accepts and passes TypeMap
func EncodeUnion(builder *flatbuffers.Builder, typeMap TypeMap, variantName string, value any) (cffi.CFFIValueUnion, flatbuffers.UOffsetT, error) {
	nameOffset := builder.CreateString(variantName)
	valueHolderOffset, err := Encode(builder, value, typeMap) // Pass typeMap recursively
	if err != nil {
		return cffi.CFFIValueUnionNONE, 0, fmt.Errorf("encoding inner value for union variant '%s': %w", variantName, err)
	}

	// Note: The CFFIValueUnionVariant schema has field_types and value_type_index.
	// The BamlUnionSerializer interface currently doesn't provide these.
	// For now, we are omitting them, which might be incomplete depending on
	// how the receiving end uses this structure. If they are needed, the
	// interface and this function must be updated.

	cffi.CFFIValueUnionVariantStart(builder)
	cffi.CFFIValueUnionVariantAddName(builder, nameOffset)
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
func encodeValue(builder *flatbuffers.Builder, value any, typeMap TypeMap) (cffi.CFFIValueUnion, flatbuffers.UOffsetT, error) {
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
		valueType, offset, err := serializer.Encode(builder, typeMap) // Pass typeMap
		if err != nil {
			return cffi.CFFIValueUnionNONE, 0, err
		}
		return valueType, offset, nil
	}

	switch v := concreteValue.(type) {
	case Checked[any]: // Use any here, or make encodeValue generic (more complex)
		offset, err := encodeChecked(builder, v, typeMap) // Pass typeMap
		if err != nil {
			return cffi.CFFIValueUnionNONE, 0, fmt.Errorf("encoding Checked value: %w", err)
		}
		return cffi.CFFIValueUnionCFFIValueChecked, offset, nil
	case StreamState[any]: // Use any here
		offset, err := encodeStreamState(builder, v, typeMap) // Pass typeMap
		if err != nil {
			return cffi.CFFIValueUnionNONE, 0, fmt.Errorf("encoding StreamState value: %w", err)
		}
		return cffi.CFFIValueUnionCFFIValueStreamingState, offset, nil
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
		// Ensure elements are of type 'any' for encoding
		// We need to convert the slice rv to []any if it's not already
		var anySlice []any
		if rv.Type().Elem().Kind() == reflect.Interface && rv.Type().Elem().NumMethod() == 0 {
			// Attempt direct assertion if it looks like []any
			rawSlice, ok := rv.Interface().([]any)
			if !ok {
				// Fallback to element-by-element conversion if assertion fails
				count := rv.Len()
				anySlice = make([]any, count)
				for i := 0; i < count; i++ {
					anySlice[i] = rv.Index(i).Interface()
				}
			} else {
				anySlice = rawSlice
			}
		} else {
			// Convert slice elements to any
			count := rv.Len()
			anySlice = make([]any, count)
			for i := 0; i < count; i++ {
				anySlice[i] = rv.Index(i).Interface()
			}
		}
		offset, err := encodeList(builder, anySlice, typeMap) // Pass typeMap
		if err != nil {
			return cffi.CFFIValueUnionNONE, 0, fmt.Errorf("encoding list: %w", err)
		}
		return cffi.CFFIValueUnionCFFIValueList, offset, nil

	case reflect.Map:
		// Expect map[string]any
		if rv.Type().Key().Kind() != reflect.String {
			return cffi.CFFIValueUnionNONE, 0, fmt.Errorf("map key type must be string, got %s", rv.Type().Key().Kind())
		}

		// Convert map to map[string]any
		stringAnyMap := make(map[string]any)
		iter := rv.MapRange()
		for iter.Next() {
			key := iter.Key().String()
			stringAnyMap[key] = iter.Value().Interface()
		}

		offset, err := encodeMap(builder, stringAnyMap, typeMap) // Pass typeMap
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
func encodeList(builder *flatbuffers.Builder, values []any, typeMap TypeMap) (flatbuffers.UOffsetT, error) {
	elemOffsets := make([]flatbuffers.UOffsetT, len(values))
	for i := len(values) - 1; i >= 0; i-- { // Build elements backwards for FlatBuffers vector
		elemOffset, err := Encode(builder, values[i], typeMap) // Pass typeMap recursively
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

	// Create the CFFIValueList table
	cffi.CFFIValueListStart(builder)
	cffi.CFFIValueListAddValues(builder, valuesVectorOffset)
	return cffi.CFFIValueListEnd(builder), nil
}

// encodeMap now accepts and passes TypeMap
func encodeMap(builder *flatbuffers.Builder, kv map[string]any, typeMap TypeMap) (flatbuffers.UOffsetT, error) {
	entryOffsets := make([]flatbuffers.UOffsetT, 0, len(kv))
	// Iterate map and build entries (order doesn't strictly matter for map, but FB requires building bottom-up)
	for k, v := range kv {
		keyOffset := builder.CreateString(k)
		valueHolderOffset, err := Encode(builder, v, typeMap) // Pass typeMap recursively
		if err != nil {
			return 0, fmt.Errorf("encoding map value for key '%s': %w", k, err)
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

	// Create the CFFIValueMap table
	cffi.CFFIValueMapStart(builder)
	cffi.CFFIValueMapAddEntries(builder, entriesVectorOffset)
	return cffi.CFFIValueMapEnd(builder), nil
}

// Helper function to encode map entries into a vector offset
func encodeMapEntries(builder *flatbuffers.Builder, fields map[string]any, typeMap TypeMap, context string) (flatbuffers.UOffsetT, error) {
	if len(fields) == 0 {
		return 0, nil // Return 0 offset for empty vector
	}

	entryOffsets := make([]flatbuffers.UOffsetT, 0, len(fields))
	// Build entries (order doesn't strictly matter, but need to build bottom-up)
	for k, v := range fields {
		keyOffset := builder.CreateString(k)
		valueHolderOffset, err := Encode(builder, v, typeMap) // Pass typeMap recursively
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
func encodeChecked(builder *flatbuffers.Builder, checkedVal Checked[any], typeMap TypeMap) (flatbuffers.UOffsetT, error) {
	valueHolderOffset, err := Encode(builder, checkedVal.Value, typeMap) // Pass typeMap
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
func encodeStreamState(builder *flatbuffers.Builder, streamStateVal StreamState[any], typeMap TypeMap) (flatbuffers.UOffsetT, error) {
	valueHolderOffset, err := Encode(builder, streamStateVal.Value, typeMap) // Pass typeMap
	if err != nil {
		return 0, fmt.Errorf("encoding inner value for StreamState: %w", err)
	}

	stateEnum := encodeStreamStateType(streamStateVal.State)

	cffi.CFFIValueStreamingStateStart(builder)
	cffi.CFFIValueStreamingStateAddValue(builder, valueHolderOffset)
	cffi.CFFIValueStreamingStateAddState(builder, stateEnum)
	return cffi.CFFIValueStreamingStateEnd(builder), nil
}

// EncodeRoot now accepts a TypeMap.
// Encode takes a Go value and returns the FlatBuffers encoded bytes for a CFFIValueHolder.
// It creates a new builder internally.
func EncodeRoot(value any, typeMap TypeMap) ([]byte, error) {
	builder := flatbuffers.NewBuilder(1024)            // Initial buffer size
	rootOffset, err := Encode(builder, value, typeMap) // Pass typeMap
	if err != nil {
		return nil, err
	}
	builder.Finish(rootOffset)
	return builder.FinishedBytes(), nil
}

// Encode now accepts and passes TypeMap.
// Encode takes a builder and a Go value, encodes the value, wraps it in a CFFIValueHolder,
// and returns the offset of the holder.
func Encode(builder *flatbuffers.Builder, value any, typeMap TypeMap) (flatbuffers.UOffsetT, error) {
	valueType, valueOffset, err := encodeValue(builder, value, typeMap) // Pass typeMap
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

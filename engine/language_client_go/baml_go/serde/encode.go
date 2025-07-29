package serde

import (
	"fmt"
	"reflect"

	"github.com/boundaryml/baml/engine/language_client_go/baml_go/shared"
	"github.com/boundaryml/baml/engine/language_client_go/pkg/cffi"
)

// BamlSerializer interface for custom class encoding
type BamlSerializer interface {
	Encode() (*cffi.CFFIValueHolder, error)
	BamlTypeName() string
	BamlEncodeName() *cffi.CFFITypeName
}

type InternalBamlSerializer interface {
	InternalBamlSerializer()
	Encode() (*cffi.CFFIValueHolder, error)
	Type() (*cffi.CFFIFieldTypeHolder, error)
}

// implment BamlSerializer for anything that implements BamlClassSerializer, BamlEnumSerializer, or BamlUnionSerializer
func EncodeClass(nameEncoder func() *cffi.CFFITypeName, fields map[string]any, dynamicFields *map[string]any) (*cffi.CFFIValueHolder, error) {
	// Encode Static Fields
	staticFields, err := EncodeMapEntries(fields, "static class")
	if err != nil {
		return nil, err // Error already includes context
	}

	// Encode Dynamic Fields
	var dynamicFieldsEncoded []*cffi.CFFIMapEntry
	if dynamicFields != nil {
		dynamicFieldsEncoded, err = EncodeMapEntries(*dynamicFields, "dynamic class")
		if err != nil {
			return nil, err // Error already includes context
		}
	}

	// Create the CFFIValueClass table
	name := nameEncoder()

	class := cffi.CFFIValueClass{
		Name:          name,
		Fields:        staticFields,
		DynamicFields: dynamicFieldsEncoded,
	}

	return &cffi.CFFIValueHolder{
		Value: &cffi.CFFIValueHolder_ClassValue{
			ClassValue: &class,
		},
	}, nil
}

func EncodeEnum(nameEncoder func() *cffi.CFFITypeName, value string, isDynamic bool) (*cffi.CFFIValueHolder, error) {
	name := nameEncoder()
	enum := cffi.CFFIValueEnum{
		Name:      name,
		Value:     value,
		IsDynamic: isDynamic,
	}

	return &cffi.CFFIValueHolder{
		Value: &cffi.CFFIValueHolder_EnumValue{
			EnumValue: &enum,
		},
	}, nil
}

func EncodeUnion(nameEncoder func() *cffi.CFFITypeName, variantName string, value any) (*cffi.CFFIValueHolder, error) {
	valueHolder, err := encodeValue(value)
	if err != nil {
		return nil, fmt.Errorf("encoding inner value for union variant '%s': %w", variantName, err)
	}

	name := nameEncoder()
	union := cffi.CFFIValueUnionVariant{
		Name:        name,
		VariantName: variantName,
		Value:       valueHolder,
	}

	return &cffi.CFFIValueHolder{
		Value: &cffi.CFFIValueHolder_UnionVariantValue{
			UnionVariantValue: &union,
		},
	}, nil
}

// encodeValue is the core recursive helper for Encode
// It takes a Go value, encodes it using the builder, and returns
func encodeValue(value any) (*cffi.CFFIValueHolder, error) {
	value_type, err := encodeFieldType(reflect.TypeOf(value),)
	if err != nil {
		return nil, err
	}
	debugLog("encoding value: %v\n", value_type)

	if value == nil {
		return &cffi.CFFIValueHolder{
			Type:  value_type,
			Value: &cffi.CFFIValueHolder_NullValue{},
		}, nil
	}

	// Use reflection to determine the type of the value
	rv := reflect.ValueOf(value)
	originalValue := value // Keep track of the original for interface checks, might be pointer

	// Handle Pointers: Dereference non-nil pointers for kind checks, but use original for interfaces
	if rv.Kind() == reflect.Ptr {
		if rv.IsNil() {
			return &cffi.CFFIValueHolder{
				Type:  value_type,
				Value: &cffi.CFFIValueHolder_NullValue{},
			}, nil // Treat nil pointers as nil values
		}
		// Work with the pointed-to value for subsequent kind checks
		rv = rv.Elem()
		// Update value variable ONLY if we are sure we won't need the pointer for interface checks later
		// value = rv.Interface() // Let's keep originalValue for interface checks
	}

	// Handle concrete types (Checked, StreamState) before general kinds
	// Use the potentially dereferenced value 'rv.Interface()' here if concrete types are structs
	concreteValue := rv.Interface() // Get the concrete value (dereferenced if original was pointer)

	if internalObject, ok := originalValue.(InternalBamlSerializer); ok {
		return internalObject.Encode()
	}

	// Check for custom serializers first using the original value (could be pointer or value)
	if serializer, ok := originalValue.(BamlSerializer); ok {
		encoded, err := serializer.Encode()
		if err != nil {
			return nil, err
		}
		encoded.Type = value_type
		return encoded, nil
	}

	switch v := concreteValue.(type) {
	case shared.Checked[any]:
		return encodeChecked(v)
	case shared.StreamState[any]:
		return encodeStreamState(v)
	}

	// Handle primitive kinds and collections using reflection value rv (points to underlying value)
	switch rv.Kind() {
	case reflect.String:
		return &cffi.CFFIValueHolder{
			Type: value_type,
			Value: &cffi.CFFIValueHolder_StringValue{
				StringValue: rv.String(),
			},
		}, nil

	case reflect.Int, reflect.Int8, reflect.Int16, reflect.Int32, reflect.Int64:
		return &cffi.CFFIValueHolder{
			Type: value_type,
			Value: &cffi.CFFIValueHolder_IntValue{
				IntValue: rv.Int(),
			},
		}, nil

	case reflect.Float32, reflect.Float64:
		return &cffi.CFFIValueHolder{
			Type: value_type,
			Value: &cffi.CFFIValueHolder_FloatValue{
				FloatValue: rv.Float(),
			},
		}, nil

	case reflect.Bool:
		return &cffi.CFFIValueHolder{
			Type: value_type,
			Value: &cffi.CFFIValueHolder_BoolValue{
				BoolValue: rv.Bool(),
			},
		}, nil

	case reflect.Slice, reflect.Array:
		encoded, err := encodeList(rv)
		if err != nil {
			return nil, fmt.Errorf("encoding list: %w", err)
		}
		encoded.Type = value_type
		return encoded, nil

	case reflect.Map:
		if rv.Type().Key().Kind() != reflect.String {
			return nil, fmt.Errorf("map key type must be string, got %s", rv.Type().Key().Kind())
		}

		encoded, err := encodeMap(rv)
		if err != nil {
			return nil, fmt.Errorf("encoding map: %w", err)
		}
		encoded.Type = value_type
		return encoded, nil

	default:
		// Use originalValue's type for the error message as it's more accurate to the input
		return nil, fmt.Errorf("unsupported type for BAML encoding: %T (Kind: %s)", originalValue, rv.Kind())
	}
}

// --- Encoding helpers for specific types ---

// encodeList now accepts and passes TypeMap
func encodeList(value reflect.Value) (*cffi.CFFIValueHolder, error) {
	values := make([]*cffi.CFFIValueHolder, value.Len())
	for i := value.Len() - 1; i >= 0; i-- {
		elemOffset, err := encodeValue(value.Index(i).Interface()) // Pass typeMap recursively
		if err != nil {
			return nil, fmt.Errorf("encoding list element %d: %w", i, err)
		}
		values[i] = elemOffset
	}

	goType := value.Type()
	goValueType := goType.Elem()
	valueType, err := encodeFieldType(goValueType)
	if err != nil {
		return nil, fmt.Errorf("encoding list value type: %w", err)
	}

	return &cffi.CFFIValueHolder{
		Value: &cffi.CFFIValueHolder_ListValue{
			ListValue: &cffi.CFFIValueList{
				Values:    values,
				ValueType: valueType,
			},
		},
	}, nil
}

// encodeMap now accepts and passes TypeMap
func encodeMap(mapValue reflect.Value) (*cffi.CFFIValueHolder, error) {

	entries := make([]*cffi.CFFIMapEntry, 0, mapValue.Len())
	for _, key := range mapValue.MapKeys() {
		value := mapValue.MapIndex(key)
		valueHolder, err := encodeValue(value.Interface())
		if err != nil {
			return nil, fmt.Errorf("encoding map value: %w", err)
		}
		entries = append(entries, &cffi.CFFIMapEntry{
			Key:   key.String(),
			Value: valueHolder,
		})
	}

	keyType, err := encodeFieldType(mapValue.Type().Key())
	if err != nil {
		return nil, fmt.Errorf("encoding map key type: %w", err)
	}
	valueType, err := encodeFieldType(mapValue.Type().Elem())
	if err != nil {
		return nil, fmt.Errorf("encoding map value type: %w", err)
	}

	// Create the CFFIValueMap table
	return &cffi.CFFIValueHolder{
		Value: &cffi.CFFIValueHolder_MapValue{
			MapValue: &cffi.CFFIValueMap{
				Entries:   entries,
				KeyType:   keyType,
				ValueType: valueType,
			},
		},
	}, nil
}

// Helper function to encode map entries into a vector offset
func EncodeMapEntries(fields map[string]any, context string) ([]*cffi.CFFIMapEntry, error) {
	entries := make([]*cffi.CFFIMapEntry, 0, len(fields))
	// Build entries (order doesn't strictly matter, but need to build bottom-up)
	for k, v := range fields {
		key := k
		valueHolder, err := encodeValue(v)
		if err != nil {
			return nil, fmt.Errorf("encoding %s field '%s': %w", context, k, err)
		}

		entries = append(entries, &cffi.CFFIMapEntry{
			Key:   key,
			Value: valueHolder,
		})
	}

	return entries, nil
}

func EncodeEnvVar(fields map[string]string) ([]*cffi.CFFIEnvVar, error) {
	if len(fields) == 0 || fields == nil {
		return nil, nil
	}

	entries := make([]*cffi.CFFIEnvVar, 0, len(fields))
	for k, v := range fields {
		entries = append(entries, &cffi.CFFIEnvVar{
			Key:   k,
			Value: v,
		})
	}

	return entries, nil
}

// encodeChecked now accepts and passes TypeMap
func encodeChecked(checkedVal shared.Checked[any]) (*cffi.CFFIValueHolder, error) {
	valueHolder, err := encodeValue(checkedVal.Value)
	if err != nil {
		return nil, fmt.Errorf("encoding inner value for Checked: %w", err)
	}

	checks := make([]*cffi.CFFICheckValue, 0, len(checkedVal.Checks))
	for _, check := range checkedVal.Checks {
		checks = append(checks, &cffi.CFFICheckValue{
			Name:       check.Name,
			Expression: check.Expression,
			Status:     check.Status,
		})
	}

	return &cffi.CFFIValueHolder{
		Value: &cffi.CFFIValueHolder_CheckedValue{
			CheckedValue: &cffi.CFFIValueChecked{
				Value:  valueHolder,
				Checks: checks,
			},
		},
	}, nil
}

// encodeStreamStateType remains the same
func encodeStreamStateType(state shared.StreamStateType) cffi.CFFIStreamState {
	switch state {
	case shared.StreamStatePending:
		return cffi.CFFIStreamState_PENDING
	case shared.StreamStateIncomplete:
		return cffi.CFFIStreamState_STARTED
	case shared.StreamStateComplete:
		return cffi.CFFIStreamState_DONE
	default:
		panic(fmt.Sprintf("unexpected Go stream state: %s", state))
	}
}

// encodeStreamState now accepts and passes TypeMap
func encodeStreamState(streamStateVal shared.StreamState[any]) (*cffi.CFFIValueHolder, error) {
	valueHolder, err := encodeValue(streamStateVal.Value) // Pass typeMap
	if err != nil {
		return nil, fmt.Errorf("encoding inner value for StreamState: %w", err)
	}

	stateEnum := encodeStreamStateType(streamStateVal.State)

	return &cffi.CFFIValueHolder{
		Value: &cffi.CFFIValueHolder_StreamingStateValue{
			StreamingStateValue: &cffi.CFFIValueStreamingState{
				Value: valueHolder,
				State: stateEnum,
			},
		},
	}, nil
}

func encodeFieldType(fieldType reflect.Type) (*cffi.CFFIFieldTypeHolder, error) {
	debugLog("encoding fieldType: %+v\n", fieldType)

	// Someone passed in a `nil` directly
	if fieldType == nil {
		return &cffi.CFFIFieldTypeHolder{
			Type: &cffi.CFFIFieldTypeHolder_NullType{
				NullType: &cffi.CFFIFieldTypeNull{},
			},
		}, nil
	}

	switch fieldType.Kind() {
	case reflect.Interface:
		// check if known interface
		if fieldType.Implements(reflect.TypeOf((*InternalBamlSerializer)(nil)).Elem()) {
			// Handle specific media interfaces that implement InternalBamlSerializer
			switch fieldType.Name() {
			case "Image":
				return &cffi.CFFIFieldTypeHolder{
					Type: &cffi.CFFIFieldTypeHolder_MediaType{
						MediaType: &cffi.CFFIFieldTypeMedia{
							Media: cffi.MediaTypeEnum_IMAGE,
						},
					},
				}, nil
			case "Audio":
				return &cffi.CFFIFieldTypeHolder{
					Type: &cffi.CFFIFieldTypeHolder_MediaType{
						MediaType: &cffi.CFFIFieldTypeMedia{
							Media: cffi.MediaTypeEnum_AUDIO,
						},
					},
				}, nil
			case "PDF":
				return &cffi.CFFIFieldTypeHolder{
					Type: &cffi.CFFIFieldTypeHolder_MediaType{
						MediaType: &cffi.CFFIFieldTypeMedia{
							Media: cffi.MediaTypeEnum_PDF,
						},
					},
				}, nil
			case "Video":
				return &cffi.CFFIFieldTypeHolder{
					Type: &cffi.CFFIFieldTypeHolder_MediaType{
						MediaType: &cffi.CFFIFieldTypeMedia{
							Media: cffi.MediaTypeEnum_VIDEO,
						},
					},
				}, nil
			default:
				// For other interfaces that implement InternalBamlSerializer,
				// we can't instantiate them with reflect.New() since they're interfaces
				return nil, fmt.Errorf("cannot instantiate interface %s that implements InternalBamlSerializer", fieldType.Name())
			}
		}
		return nil, fmt.Errorf("interface %s does not implement InternalBamlSerializer", fieldType.Name())
	case reflect.Ptr:
		inner, err := encodeFieldType(fieldType.Elem())
		if err != nil {
			return nil, err
		}
		// this this is optional.
		return &cffi.CFFIFieldTypeHolder{
			Type: &cffi.CFFIFieldTypeHolder_OptionalType{
				OptionalType: &cffi.CFFIFieldTypeOptional{
					Value: inner,
				},
			},
		}, nil
	case reflect.String:
		// String that implements BamlSerializer is an enum
		if fieldType.Implements(reflect.TypeOf((*BamlSerializer)(nil)).Elem()) {
			serializer := reflect.New(fieldType).Interface().(BamlSerializer)
			name := serializer.BamlEncodeName()
			return &cffi.CFFIFieldTypeHolder{
				Type: &cffi.CFFIFieldTypeHolder_EnumType{
					EnumType: &cffi.CFFIFieldTypeEnum{
						Name: name.Name,
					},
				},
			}, nil
		}
		return &cffi.CFFIFieldTypeHolder{
			Type: &cffi.CFFIFieldTypeHolder_StringType{
				StringType: &cffi.CFFIFieldTypeString{},
			},
		}, nil
	case reflect.Int, reflect.Int8, reflect.Int16, reflect.Int32, reflect.Int64:
		return &cffi.CFFIFieldTypeHolder{
			Type: &cffi.CFFIFieldTypeHolder_IntType{
				IntType: &cffi.CFFIFieldTypeInt{},
			},
		}, nil
	case reflect.Float32, reflect.Float64:
		return &cffi.CFFIFieldTypeHolder{
			Type: &cffi.CFFIFieldTypeHolder_FloatType{
				FloatType: &cffi.CFFIFieldTypeFloat{},
			},
		}, nil
	case reflect.Bool:
		return &cffi.CFFIFieldTypeHolder{
			Type: &cffi.CFFIFieldTypeHolder_BoolType{
				BoolType: &cffi.CFFIFieldTypeBool{},
			},
		}, nil
	case reflect.Slice, reflect.Array:
		sliceFieldType, err := encodeFieldType(fieldType.Elem())
		if err != nil {
			return nil, err
		}

		return &cffi.CFFIFieldTypeHolder{
			Type: &cffi.CFFIFieldTypeHolder_ListType{
				ListType: &cffi.CFFIFieldTypeList{
					Element: sliceFieldType,
				},
			},
		}, nil
	case reflect.Map:
		keyType, err := encodeFieldType(fieldType.Key())
		if err != nil {
			return nil, err
		}
		valueType, err := encodeFieldType(fieldType.Elem())
		if err != nil {
			return nil, err
		}

		return &cffi.CFFIFieldTypeHolder{
			Type: &cffi.CFFIFieldTypeHolder_MapType{
				MapType: &cffi.CFFIFieldTypeMap{
					Key:   keyType,
					Value: valueType,
				},
			},
		}, nil
	case reflect.Struct:
		if serializer, ok := reflect.New(fieldType).Interface().(InternalBamlSerializer); ok {
			return serializer.Type()
		}

		// determine if the struct implements BamlSerializer
		if fieldType.Implements(reflect.TypeOf((*BamlSerializer)(nil)).Elem()) {
			serializer := reflect.New(fieldType).Interface().(BamlSerializer)
			name := serializer.BamlEncodeName()
			return &cffi.CFFIFieldTypeHolder{
				Type: &cffi.CFFIFieldTypeHolder_ClassType{
					ClassType: &cffi.CFFIFieldTypeClass{
						Name: name,
					},
				},
			}, nil
		} else {
			return nil, fmt.Errorf("struct %s does not implement BamlSerializer", fieldType.Name())
		}
	default:
		return nil, fmt.Errorf("unexpected field type: %+v %s", fieldType, fieldType.Kind())
	}
}

// This is only used for testing, do not use in production
func BAMLTESTINGONLY_InternalEncode(value any) (*cffi.CFFIValueHolder, error) {
	return encodeValue(value)
}

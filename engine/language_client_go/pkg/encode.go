package baml

import (
	"fmt"
	"reflect"

	"github.com/boundaryml/baml/engine/language_client_go/pkg/cffi"
	"google.golang.org/protobuf/proto"
)

// BamlSerializer interface for custom class encoding
type BamlSerializer interface {
	Encode() (*cffi.CFFIValueHolder, error)
	BamlTypeName() string
	BamlEncodeName() *cffi.CFFITypeName
}

// implment BamlSerializer for anything that implements BamlClassSerializer, BamlEnumSerializer, or BamlUnionSerializer
func EncodeClass(nameEncoder func() *cffi.CFFITypeName, fields map[string]any, dynamicFields *map[string]any) (*cffi.CFFIValueHolder, error) {
	// Encode Static Fields
	staticFields, err := encodeMapEntries(fields, "static class")
	if err != nil {
		return nil, err // Error already includes context
	}

	// Encode Dynamic Fields
	var dynamicFieldsEncoded []*cffi.CFFIMapEntry
	if dynamicFields != nil {
		dynamicFieldsEncoded, err = encodeMapEntries(*dynamicFields, "dynamic class")
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
	if value == nil {
		return &cffi.CFFIValueHolder{
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

	// Check for custom serializers first using the original value (could be pointer or value)
	if serializer, ok := originalValue.(BamlSerializer); ok {
		encoded, err := serializer.Encode()
		if err != nil {
			return nil, err
		}
		return encoded, nil
	}

	switch v := concreteValue.(type) {
	case Checked[any]: // Use any here, or make encodeValue generic (more complex)
		encoded, err := encodeChecked(v)
		if err != nil {
			return nil, fmt.Errorf("encoding Checked value: %w", err)
		}
		return encoded, nil
	case StreamState[any]: // Use any here
		encoded, err := encodeStreamState(v) // Pass typeMap
		if err != nil {
			return nil, fmt.Errorf("encoding StreamState value: %w", err)
		}
		return encoded, nil
	case BamlFunctionArguments:
		panic("BamlFunctionArguments not supported here, must be encoded separately")
	}

	// Handle primitive kinds and collections using reflection value rv (points to underlying value)
	switch rv.Kind() {
	case reflect.String:
		return &cffi.CFFIValueHolder{
			Value: &cffi.CFFIValueHolder_StringValue{
				StringValue: rv.String(),
			},
		}, nil

	case reflect.Int, reflect.Int8, reflect.Int16, reflect.Int32, reflect.Int64:
		return &cffi.CFFIValueHolder{
			Value: &cffi.CFFIValueHolder_IntValue{
				IntValue: rv.Int(),
			},
		}, nil

	case reflect.Float32, reflect.Float64:
		return &cffi.CFFIValueHolder{
			Value: &cffi.CFFIValueHolder_FloatValue{
				FloatValue: rv.Float(),
			},
		}, nil

	case reflect.Bool:
		return &cffi.CFFIValueHolder{
			Value: &cffi.CFFIValueHolder_BoolValue{
				BoolValue: rv.Bool(),
			},
		}, nil

	case reflect.Slice, reflect.Array:
		encoded, err := encodeList(rv)
		if err != nil {
			return nil, fmt.Errorf("encoding list: %w", err)
		}
		return encoded, nil

	case reflect.Map:
		if rv.Type().Key().Kind() != reflect.String {
			return nil, fmt.Errorf("map key type must be string, got %s", rv.Type().Key().Kind())
		}

		encoded, err := encodeMap(rv)
		if err != nil {
			return nil, fmt.Errorf("encoding map: %w", err)
		}
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
	valueType := encodeFieldType(goValueType)

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

	keyType := encodeFieldType(mapValue.Type().Key())
	valueType := encodeFieldType(mapValue.Type().Elem())

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
func encodeMapEntries(fields map[string]any, context string) ([]*cffi.CFFIMapEntry, error) {
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

func encodeEnvVar(fields map[string]string) ([]*cffi.CFFIEnvVar, error) {
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
func encodeChecked(checkedVal Checked[any]) (*cffi.CFFIValueHolder, error) {
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
func encodeStreamStateType(state StreamStateType) cffi.CFFIStreamState {
	switch state {
	case StreamStatePending:
		return cffi.CFFIStreamState_PENDING
	case StreamStateIncomplete:
		return cffi.CFFIStreamState_STARTED
	case StreamStateComplete:
		return cffi.CFFIStreamState_DONE
	default:
		panic(fmt.Sprintf("unexpected Go stream state: %s", state))
	}
}

// encodeStreamState now accepts and passes TypeMap
func encodeStreamState(streamStateVal StreamState[any]) (*cffi.CFFIValueHolder, error) {
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

func encodeFunctionArguments(functionArgumentsVal BamlFunctionArguments) (*cffi.CFFIFunctionArguments, error) {
	kwargs, err := encodeMapEntries(functionArgumentsVal.Kwargs, "function arguments")
	if err != nil {
		return nil, fmt.Errorf("encoding function arguments: %w", err)
	}

	var clientRegistry *cffi.CFFIClientRegistry
	if functionArgumentsVal.ClientRegistry != nil {
		clientRegistry, err = encodeClientRegistry(functionArgumentsVal.ClientRegistry)
		if err != nil {
			return nil, fmt.Errorf("encoding client registry: %w", err)
		}
	}

	var env []*cffi.CFFIEnvVar
	if functionArgumentsVal.Env != nil {
		env, err = encodeEnvVar(functionArgumentsVal.Env)
		if err != nil {
			return nil, fmt.Errorf("encoding env vars: %w", err)
		}
	}

	var collectors []*cffi.CFFICollector
	if functionArgumentsVal.Collectors != nil {
		collectors, err = encodeCollectors(functionArgumentsVal.Collectors)
		if err != nil {
			return nil, fmt.Errorf("encoding collectors: %w", err)
		}
	}

	functionArguments := cffi.CFFIFunctionArguments{
		Kwargs:         kwargs,
		ClientRegistry: clientRegistry,
		Env:            env,
		Collectors:     collectors,
	}

	return &functionArguments, nil
}

func encodeCollectors(collectorsVal []Collector) ([]*cffi.CFFICollector, error) {
	collectors := make([]*cffi.CFFICollector, 0, len(collectorsVal))
	for _, collector := range collectorsVal {
		collectors = append(collectors, &cffi.CFFICollector{
			Pointer: collector.id(),
		})
	}

	return collectors, nil
}

func encodeClientRegistry(clientRegistryVal *ClientRegistry) (*cffi.CFFIClientRegistry, error) {

	clientOffsets := make([]*cffi.CFFIClientProperty, 0, len(clientRegistryVal.clients))
	for _, client := range clientRegistryVal.clients {
		options, err := encodeMapEntries(client.options, "client options")
		if err != nil {
			return nil, fmt.Errorf("encoding client options: %w", err)
		}
		clientOffsets = append(clientOffsets, &cffi.CFFIClientProperty{
			Provider:    client.provider,
			RetryPolicy: client.retryPolicy,
			Options:     options,
		})
	}

	clients := cffi.CFFIClientRegistry{
		Clients: clientOffsets,
		Primary: clientRegistryVal.primary,
	}

	return &clients, nil
}

func encodeFieldType(fieldType reflect.Type) *cffi.CFFIFieldTypeHolder {

	switch fieldType.Kind() {
	case reflect.Ptr:
		return encodeFieldType(fieldType.Elem())
	case reflect.String:
		return &cffi.CFFIFieldTypeHolder{
			Type: &cffi.CFFIFieldTypeHolder_StringType{
				StringType: &cffi.CFFIFieldTypeString{},
			},
		}
	case reflect.Int, reflect.Int8, reflect.Int16, reflect.Int32, reflect.Int64:
		return &cffi.CFFIFieldTypeHolder{
			Type: &cffi.CFFIFieldTypeHolder_IntType{
				IntType: &cffi.CFFIFieldTypeInt{},
			},
		}
	case reflect.Float32, reflect.Float64:
		return &cffi.CFFIFieldTypeHolder{
			Type: &cffi.CFFIFieldTypeHolder_FloatType{
				FloatType: &cffi.CFFIFieldTypeFloat{},
			},
		}
	case reflect.Bool:
		return &cffi.CFFIFieldTypeHolder{
			Type: &cffi.CFFIFieldTypeHolder_BoolType{
				BoolType: &cffi.CFFIFieldTypeBool{},
			},
		}
	case reflect.Slice, reflect.Array:
		sliceFieldType := encodeFieldType(fieldType.Elem())

		return &cffi.CFFIFieldTypeHolder{
			Type: &cffi.CFFIFieldTypeHolder_ListType{
				ListType: &cffi.CFFIFieldTypeList{
					Element: sliceFieldType,
				},
			},
		}
	case reflect.Map:
		keyType := encodeFieldType(fieldType.Key())
		valueType := encodeFieldType(fieldType.Elem())

		return &cffi.CFFIFieldTypeHolder{
			Type: &cffi.CFFIFieldTypeHolder_MapType{
				MapType: &cffi.CFFIFieldTypeMap{
					Key:   keyType,
					Value: valueType,
				},
			},
		}
	case reflect.Struct:
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
			}
		} else {
			panic(fmt.Sprintf("struct %s does not implement BamlSerializer", fieldType.Name()))
		}
	default:
		panic(fmt.Sprintf("unexpected field type: %s", fieldType.Kind()))
	}
}

func EncodeArgs(args BamlFunctionArguments) ([]byte, error) {
	root, err := encodeFunctionArguments(args)
	if err != nil {
		return nil, err
	}
	return proto.Marshal(root)
}

// This is only used for testing, do not use in production
func BAMLTESTINGONLY_InternalEncode(value any) (*cffi.CFFIValueHolder, error) {
	return encodeValue(value)
}

package serde

import (
	"fmt"
	"reflect"

	"github.com/boundaryml/baml/engine/language_client_go/baml_go/shared"
	"github.com/boundaryml/baml/engine/language_client_go/pkg/cffi"
)

// BamlSerializer interface for custom class encoding
type BamlSerializer interface {
	Encode() (*cffi.HostValue, error)
	BamlTypeName() string
}

type InternalBamlSerializer interface {
	InternalBamlSerializer()
	Encode() (*cffi.BamlObjectHandle, error)
}

// implment BamlSerializer for anything that implements BamlClassSerializer, BamlEnumSerializer, or BamlUnionSerializer
func EncodeClass(name string, fields map[string]any, dynamicFields *map[string]any) (*cffi.HostValue, error) {
	all_fields := make(map[string]any)
	for k, v := range fields {
		all_fields[k] = v
	}
	if dynamicFields != nil {
		for k, v := range *dynamicFields {
			all_fields[k] = v
		}
	}

	// Encode Static Fields
	staticFields, err := EncodeMapEntries(all_fields, "static class")
	if err != nil {
		return nil, err // Error already includes context
	}

	// Create the CFFIValueClass table
	class := cffi.HostClassValue{
		Name:   name,
		Fields: staticFields,
	}

	return &cffi.HostValue{
		Value: &cffi.HostValue_ClassValue{
			ClassValue: &class,
		},
	}, nil
}

func EncodeEnum(name string, value string, isDynamic bool) (*cffi.HostValue, error) {
	return &cffi.HostValue{
		Value: &cffi.HostValue_EnumValue{
			EnumValue: &cffi.HostEnumValue{
				Name:  name,
				Value: value,
			},
		},
	}, nil
}

// encodeValue is the core recursive helper for Encode
// It takes a Go value, encodes it using the builder, and returns
func encodeValue(value any) (*cffi.HostValue, error) {
	if value == nil {
		return &cffi.HostValue{}, nil
	}

	// Use reflection to determine the type of the value
	rv := reflect.ValueOf(value)
	originalValue := value // Keep track of the original for interface checks, might be pointer

	// Handle Pointers: Dereference non-nil pointers for kind checks, but use original for interfaces
	if rv.Kind() == reflect.Ptr {
		if rv.IsNil() {
			return &cffi.HostValue{}, nil // Treat nil pointers as nil values
		}
		// Work with the pointed-to value for subsequent kind checks
		rv = rv.Elem()
	}

	// Handle concrete types (Checked, StreamState) before general kinds
	// Use the potentially dereferenced value 'rv.Interface()' here if concrete types are structs
	concreteValue := rv.Interface() // Get the concrete value (dereferenced if original was pointer)

	if internalObject, ok := originalValue.(InternalBamlSerializer); ok {
		handle, err := internalObject.Encode()
		if err != nil {
			return nil, fmt.Errorf("encoding internal object: %w", err)
		}
		return &cffi.HostValue{
			Value: &cffi.HostValue_Handle{
				Handle: handle,
			},
		}, nil
	}

	// Check for custom serializers first using the original value (could be pointer or value)
	if serializer, ok := originalValue.(BamlSerializer); ok {
		encoded, err := serializer.Encode()
		if err != nil {
			return nil, err
		}
		return encoded, nil
	}

	switch concreteValue.(type) {
	case shared.Checked[any]:
		return nil, fmt.Errorf("unsupported type: Checked[any] cannot be passed as inputs to baml functions")
	case shared.StreamState[any]:
		return nil, fmt.Errorf("unsupported type: StreamState[any] cannot be passed as inputs to baml functions")
	}

	// Handle primitive kinds and collections using reflection value rv (points to underlying value)
	switch rv.Kind() {
	case reflect.String:
		return &cffi.HostValue{
			Value: &cffi.HostValue_StringValue{
				StringValue: rv.String(),
			},
		}, nil

	case reflect.Int, reflect.Int8, reflect.Int16, reflect.Int32, reflect.Int64:
		return &cffi.HostValue{
			Value: &cffi.HostValue_IntValue{
				IntValue: rv.Int(),
			},
		}, nil

	case reflect.Float32, reflect.Float64:
		return &cffi.HostValue{
			Value: &cffi.HostValue_FloatValue{
				FloatValue: rv.Float(),
			},
		}, nil

	case reflect.Bool:
		return &cffi.HostValue{
			Value: &cffi.HostValue_BoolValue{
				BoolValue: rv.Bool(),
			},
		}, nil

	case reflect.Slice, reflect.Array:
		encoded, err := encodeList(rv)
		if err != nil {
			return nil, fmt.Errorf("encoding list: %w", err)
		}
		return &cffi.HostValue{
			Value: &cffi.HostValue_ListValue{
				ListValue: encoded,
			},
		}, nil

	case reflect.Map:
		if rv.Type().Key().Kind() != reflect.String {
			return nil, fmt.Errorf("map key type must be string, got %s", rv.Type().Key().Kind())
		}

		encoded, err := encodeMap(rv)
		if err != nil {
			return nil, fmt.Errorf("encoding map: %w", err)
		}
		return &cffi.HostValue{
			Value: &cffi.HostValue_MapValue{
				MapValue: encoded,
			},
		}, nil

	default:
		// Use originalValue's type for the error message as it's more accurate to the input
		return nil, fmt.Errorf("unsupported type for BAML encoding: %T (Kind: %s)", originalValue, rv.Kind())
	}
}

// --- Encoding helpers for specific types ---

// encodeList now accepts and passes TypeMap
func encodeList(value reflect.Value) (*cffi.HostListValue, error) {
	values := make([]*cffi.HostValue, value.Len())
	for i := value.Len() - 1; i >= 0; i-- {
		elemOffset, err := encodeValue(value.Index(i).Interface()) // Pass typeMap recursively
		if err != nil {
			return nil, fmt.Errorf("encoding list element %d: %w", i, err)
		}
		values[i] = elemOffset
	}

	return &cffi.HostListValue{
		Values: values,
	}, nil
}

// encodeMap now accepts and passes TypeMap
func encodeMap(mapValue reflect.Value) (*cffi.HostMapValue, error) {

	entries := make([]*cffi.HostMapEntry, 0, mapValue.Len())
	for _, key := range mapValue.MapKeys() {
		value := mapValue.MapIndex(key)
		valueHolder, err := encodeValue(value.Interface())
		if err != nil {
			return nil, fmt.Errorf("encoding map value: %w", err)
		}
		entries = append(entries, &cffi.HostMapEntry{
			Key:   &cffi.HostMapEntry_StringKey{StringKey: key.String()},
			Value: valueHolder,
		})
	}

	return &cffi.HostMapValue{
		Entries: entries,
	}, nil
}

// Helper function to encode map entries into a vector offset
func EncodeMapEntries(fields map[string]any, context string) ([]*cffi.HostMapEntry, error) {
	entries := make([]*cffi.HostMapEntry, 0, len(fields))
	// Build entries (order doesn't strictly matter, but need to build bottom-up)
	for k, v := range fields {
		key := k
		valueHolder, err := encodeValue(v)
		if err != nil {
			return nil, fmt.Errorf("encoding %s field '%s': %w", context, k, err)
		}

		entries = append(entries, &cffi.HostMapEntry{
			Key:   &cffi.HostMapEntry_StringKey{StringKey: key},
			Value: valueHolder,
		})
	}

	return entries, nil
}

func EncodeValue(value any) (*cffi.HostValue, error) {
	return encodeValue(value)
}

func EncodeEnvVar(fields map[string]string) ([]*cffi.HostEnvVar, error) {
	if len(fields) == 0 || fields == nil {
		return nil, nil
	}

	entries := make([]*cffi.HostEnvVar, 0, len(fields))
	for k, v := range fields {
		entries = append(entries, &cffi.HostEnvVar{
			Key:   k,
			Value: v,
		})
	}

	return entries, nil
}

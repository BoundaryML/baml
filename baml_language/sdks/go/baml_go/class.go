package baml_go

import (
	"fmt"
	"math/big"
	"sort"

	"github.com/boundaryml/baml-go/internal/cffi"
)

// Class constructs a non-generic BAML class value. name and every field key
// are exact BAML wire names, not their generated Go projections.
func Class(name string, fields map[string]Input) Input {
	keys := make([]string, 0, len(fields))
	for key := range fields {
		keys = append(keys, key)
	}
	sort.Strings(keys)

	entries := make([]*cffi.InboundMapEntry, 0, len(keys))
	for _, key := range keys {
		field := fields[key]
		if field.err != nil {
			return field
		}
		if field.value == nil {
			return Input{}
		}
		entries = append(entries, &cffi.InboundMapEntry{
			Key:   &cffi.InboundMapEntry_StringKey{StringKey: key},
			Value: field.value,
		})
	}

	return Input{value: &cffi.InboundValue{Value: &cffi.InboundValue_ClassValue{
		ClassValue: &cffi.InboundClassValue{
			Fields: entries,
			ClassTy: &cffi.BamlTyClass{
				Name: name,
			},
		},
	}}}
}

// ClassValue is a validated non-generic class returned by BAML.
type ClassValue struct {
	name   string
	fields map[string]Value
}

// Class validates that value is the named non-generic BAML class and indexes
// its fields for generated decoders.
func (value Value) Class(name string) (ClassValue, error) {
	if value.value == nil {
		return ClassValue{}, fmt.Errorf("BAML value is uninitialized")
	}
	item, ok := value.value.Value.(*cffi.BamlOutboundValue_ClassValue)
	if !ok || item.ClassValue == nil {
		return ClassValue{}, fmt.Errorf("expected BAML class %q, got %T", name, value.value.Value)
	}
	if item.ClassValue.Name != name {
		return ClassValue{}, fmt.Errorf("expected BAML class %q, got %q", name, item.ClassValue.Name)
	}
	if len(item.ClassValue.TypeArgs) != 0 {
		return ClassValue{}, fmt.Errorf("BAML class %q unexpectedly has %d type arguments", name, len(item.ClassValue.TypeArgs))
	}

	fields := make(map[string]Value, len(item.ClassValue.Fields))
	for index, entry := range item.ClassValue.Fields {
		if entry == nil {
			return ClassValue{}, fmt.Errorf("BAML class %q has an empty field entry at index %d", name, index)
		}
		if entry.Value == nil {
			return ClassValue{}, fmt.Errorf("BAML class %q field %q has an empty value", name, entry.Key)
		}
		if _, duplicate := fields[entry.Key]; duplicate {
			return ClassValue{}, fmt.Errorf("BAML class %q returned duplicate field %q", name, entry.Key)
		}
		fields[entry.Key] = Value{value: entry.Value}
	}
	return ClassValue{name: name, fields: fields}, nil
}

// Field returns one field by its exact BAML wire name.
func (value ClassValue) Field(name string) (Value, error) {
	field, ok := value.fields[name]
	if !ok {
		return Value{}, fmt.Errorf("BAML class %q is missing field %q", value.name, name)
	}
	return field, nil
}

// Class decodes a nested class field and validates its exact BAML class name.
func (value ClassValue) Class(fieldName string, className string) (ClassValue, error) {
	field, err := value.Field(fieldName)
	if err != nil {
		return ClassValue{}, err
	}
	result, err := field.Class(className)
	return result, classFieldError(value.name, fieldName, err)
}

func (value ClassValue) String(name string) (string, error) {
	field, err := value.Field(name)
	if err != nil {
		return "", err
	}
	result, err := field.String()
	return result, classFieldError(value.name, name, err)
}

func (value ClassValue) Int64(name string) (int64, error) {
	field, err := value.Field(name)
	if err != nil {
		return 0, err
	}
	result, err := field.Int64()
	return result, classFieldError(value.name, name, err)
}

func (value ClassValue) BigInt(name string) (*big.Int, error) {
	field, err := value.Field(name)
	if err != nil {
		return nil, err
	}
	result, err := field.BigInt()
	return result, classFieldError(value.name, name, err)
}

func (value ClassValue) Float64(name string) (float64, error) {
	field, err := value.Field(name)
	if err != nil {
		return 0, err
	}
	result, err := field.Float64()
	return result, classFieldError(value.name, name, err)
}

func (value ClassValue) Bool(name string) (bool, error) {
	field, err := value.Field(name)
	if err != nil {
		return false, err
	}
	result, err := field.Bool()
	return result, classFieldError(value.name, name, err)
}

func (value ClassValue) Null(name string) (Null, error) {
	field, err := value.Field(name)
	if err != nil {
		return Null{}, err
	}
	result, err := field.Null()
	return result, classFieldError(value.name, name, err)
}

func (value ClassValue) Uint8Array(name string) ([]byte, error) {
	field, err := value.Field(name)
	if err != nil {
		return nil, err
	}
	result, err := field.Uint8Array()
	return result, classFieldError(value.name, name, err)
}

func classFieldError(className string, fieldName string, err error) error {
	if err == nil {
		return nil
	}
	return fmt.Errorf("BAML class %q field %q: %w", className, fieldName, err)
}

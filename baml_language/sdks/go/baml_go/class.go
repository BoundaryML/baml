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
	return ClassWithTypeArgs(name, nil, fields)
}

// ClassWithTypeArgs constructs a parameterized BAML class value. Type
// arguments are positional in the class declaration's canonical order.
func ClassWithTypeArgs(name string, typeArgs []BAMLType, fields map[string]Input) Input {
	typeArgs = append([]BAMLType(nil), typeArgs...)
	keys := make([]string, 0, len(fields))
	for key := range fields {
		keys = append(keys, key)
	}
	sort.Strings(keys)
	inputs := make([]Input, 0, len(keys))
	for _, key := range keys {
		inputs = append(inputs, fields[key])
	}

	prepare := func(transaction *inputTransaction) (*cffi.InboundValue, error) {
		encodedTypeArgs := make([]*cffi.BamlTy, len(typeArgs))
		for index, typeArg := range typeArgs {
			if err := validateBAMLType(typeArg.value, 0); err != nil {
				return nil, fmt.Errorf("class %q type argument %d: %w", name, index, err)
			}
			encodedTypeArgs[index] = typeArg.value
		}
		entries := make([]*cffi.InboundMapEntry, 0, len(keys))
		for index, key := range keys {
			encoded, err := inputs[index].encodeValue(transaction)
			if err != nil {
				return nil, fmt.Errorf("class %q field %q: %w", name, key, err)
			}
			entries = append(entries, &cffi.InboundMapEntry{
				Key:   &cffi.InboundMapEntry_StringKey{StringKey: key},
				Value: encoded,
			})
		}

		return &cffi.InboundValue{
			ValueType: &cffi.BamlTy{
				Ty: &cffi.BamlTy_ClassTy{
					ClassTy: &cffi.BamlTyClass{
						Name:     name,
						TypeArgs: encodedTypeArgs,
					},
				},
			},
			Value: &cffi.InboundValue_ClassValue{
				ClassValue: &cffi.InboundClassValue{
					Fields: entries,
				},
			},
		}, nil
	}
	if inputsAreStatic(inputs) {
		value, err := prepare(nil)
		return Input{value: value, err: err}
	}
	return Input{deferred: &inputEncoder{encode: prepare}}
}

// ClassValue is a validated class returned by BAML, including its concrete
// generic arguments when parameterized.
type ClassValue struct {
	name     string
	typeArgs []BAMLType
	fields   map[string]Value
}

// Class validates that value is the named non-generic BAML class and indexes
// its fields for generated decoders.
func (value Value) Class(name string) (ClassValue, error) {
	return value.ClassWithTypeArgs(name, nil)
}

// ClassWithTypeArgs validates both the nominal class and its complete
// parameterization before exposing fields to generated decoders.
func (value Value) ClassWithTypeArgs(name string, typeArgs []BAMLType) (ClassValue, error) {
	unwrapped, err := value.unwrapUnionVariants()
	if err != nil {
		return ClassValue{}, err
	}
	value = unwrapped
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
	if len(item.ClassValue.TypeArgs) != len(typeArgs) {
		return ClassValue{}, fmt.Errorf("BAML class %q has %d type arguments, expected %d", name, len(item.ClassValue.TypeArgs), len(typeArgs))
	}
	decodedTypeArgs := make([]BAMLType, len(typeArgs))
	for index, expected := range typeArgs {
		actual := BAMLType{value: item.ClassValue.TypeArgs[index]}
		if err := validateBAMLType(actual.value, 0); err != nil {
			return ClassValue{}, fmt.Errorf("BAML class %q type argument %d: %w", name, index, err)
		}
		if !actual.Equal(expected) {
			return ClassValue{}, fmt.Errorf("BAML class %q type argument %d does not match the generated Go type", name, index)
		}
		decodedTypeArgs[index] = actual
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
		fields[entry.Key] = Value{value: entry.Value, owner: value.owner}
	}
	return ClassValue{name: name, typeArgs: decodedTypeArgs, fields: fields}, nil
}

// TypeArgs returns a copy of the validated concrete class arguments.
func (value ClassValue) TypeArgs() []BAMLType {
	return append([]BAMLType(nil), value.typeArgs...)
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

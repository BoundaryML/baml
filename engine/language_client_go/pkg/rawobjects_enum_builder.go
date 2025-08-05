package baml

import (
	"fmt"
	"unsafe"

	"github.com/boundaryml/baml/engine/language_client_go/baml_go/raw_objects"
	"github.com/boundaryml/baml/engine/language_client_go/pkg/cffi"
)

// enumBuilder provides enum construction functionality
type enumBuilder struct {
	*raw_objects.RawObject
	llmRenderableObject
}

func (eb *enumBuilder) ObjectType() cffi.CFFIObjectType {
	return cffi.CFFIObjectType_OBJECT_ENUM_BUILDER
}

func newEnumBuilder(ptr int64, rt unsafe.Pointer) EnumBuilder {
	bldr := enumBuilder{raw_objects.FromPointer(ptr, rt), llmRenderableObject{}}
	bldr.llmRenderableObject = llmRenderableObject{&bldr}
	return &bldr
}

// AddValue adds a new value to the enum
func (eb *enumBuilder) AddValue(value string) (EnumValueBuilder, error) {
	args := map[string]interface{}{
		"value": value,
	}
	result, err := raw_objects.CallMethod(eb, "add_value", args)
	if err != nil {
		return nil, err
	}

	enumValueBuilder, ok := result.(EnumValueBuilder)
	if !ok {
		return nil, fmt.Errorf("unexpected type for enum value builder: %T", result)
	}

	return enumValueBuilder, nil
}

// Type returns the type definition for this enum
func (eb *enumBuilder) Type() (Type, error) {
	result, err := raw_objects.CallMethod(eb, "type_", nil)
	if err != nil {
		return nil, err
	}

	typ, ok := result.(Type)
	if !ok {
		return nil, fmt.Errorf("unexpected type for enum type: %T", result)
	}

	return typ, nil
}

// ListValues returns all values in the enum
func (eb *enumBuilder) ListValues() ([]EnumValueBuilder, error) {
	result, err := raw_objects.CallMethod(eb, "list_values", nil)
	if err != nil {
		return nil, err
	}

	rawObjects, ok := result.([]raw_objects.RawPointer)
	if !ok {
		return nil, fmt.Errorf("unexpected type for enum value builders: %T", result)
	}

	enumValueBuilders := make([]EnumValueBuilder, len(rawObjects))
	for i, rawObject := range rawObjects {
		enumValueBuilders[i] = rawObject.(EnumValueBuilder)
	}

	return enumValueBuilders, nil
}

// Value gets a specific value from the enum
func (eb *enumBuilder) Value(name string) (EnumValueBuilder, error) {
	args := map[string]interface{}{
		"name": name,
	}
	result, err := raw_objects.CallMethod(eb, "value", args)
	if err != nil {
		return nil, err
	}

	enumValueBuilder, ok := result.(EnumValueBuilder)
	if !ok {
		return nil, fmt.Errorf("unexpected type for enum value builder: %T", result)
	}

	return enumValueBuilder, nil
}
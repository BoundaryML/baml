package baml

import (
	"fmt"
	"unsafe"

	"github.com/boundaryml/baml/engine/language_client_go/baml_go/raw_objects"
	"github.com/boundaryml/baml/engine/language_client_go/pkg/cffi"
)

// classBuilder provides class construction functionality
type classBuilder struct {
	*raw_objects.RawObject
	llmRenderableObject
}

func (cb *classBuilder) ObjectType() cffi.CFFIObjectType {
	return cffi.CFFIObjectType_OBJECT_CLASS_BUILDER
}

func newClassBuilder(ptr int64, rt unsafe.Pointer) ClassBuilder {
	bldr := classBuilder{raw_objects.FromPointer(ptr, rt), llmRenderableObject{}}
	bldr.llmRenderableObject = llmRenderableObject{&bldr}
	return &bldr
}

// Type returns the type definition for this class
func (cb *classBuilder) Type() (Type, error) {
	result, err := raw_objects.CallMethod(cb, "type_", nil)
	if err != nil {
		return nil, err
	}

	typ, ok := result.(Type)
	if !ok {
		return nil, fmt.Errorf("unexpected type for class type: %T", result)
	}

	return typ, nil
}

// ListProperties returns all properties in the class
func (cb *classBuilder) ListProperties() ([]ClassPropertyBuilder, error) {
	result, err := raw_objects.CallMethod(cb, "list_properties", nil)
	if err != nil {
		return nil, err
	}

	rawObjects, ok := result.([]raw_objects.RawPointer)
	if !ok {
		return nil, fmt.Errorf("unexpected type for class property builders: %T", result)
	}

	rawObjectsCast := make([]ClassPropertyBuilder, len(rawObjects))
	for i, rawObject := range rawObjects {
		rawObjectsCast[i] = rawObject.(ClassPropertyBuilder)
	}

	return rawObjectsCast, nil
}

// AddProperty adds a new property to the class
func (cb *classBuilder) AddProperty(name string, fieldType Type) (ClassPropertyBuilder, error) {
	args := map[string]interface{}{
		"name":       name,
		"field_type": fieldType,
	}
	result, err := raw_objects.CallMethod(cb, "add_property", args)
	if err != nil {
		return nil, err
	}

	classPropertyBuilder, ok := result.(ClassPropertyBuilder)
	if !ok {
		return nil, fmt.Errorf("unexpected type for class property builder: %T", result)
	}

	return classPropertyBuilder, nil
}

// Property gets a specific property from the class
func (cb *classBuilder) Property(name string) (ClassPropertyBuilder, error) {
	args := map[string]interface{}{
		"name": name,
	}
	result, err := raw_objects.CallMethod(cb, "property", args)
	if err != nil {
		return nil, err
	}

	classPropertyBuilder, ok := result.(ClassPropertyBuilder)
	if !ok {
		return nil, fmt.Errorf("unexpected type for class property builder: %T", result)
	}

	return classPropertyBuilder, nil
}

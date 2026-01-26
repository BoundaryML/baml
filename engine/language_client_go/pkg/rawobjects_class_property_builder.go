package baml

import (
	"fmt"
	"unsafe"

	"github.com/boundaryml/baml/engine/language_client_go/baml_go/raw_objects"
	"github.com/boundaryml/baml/engine/language_client_go/pkg/cffi"
)

// classPropertyBuilder provides class property construction functionality
type classPropertyBuilder struct {
	*raw_objects.RawObject
	llmRenderableObject
}

func (cpb *classPropertyBuilder) ObjectType() cffi.BamlObjectType {
	return cffi.BamlObjectType_OBJECT_CLASS_PROPERTY_BUILDER
}

func newClassPropertyBuilder(ptr int64, rt unsafe.Pointer) ClassPropertyBuilder {
	bldr := classPropertyBuilder{raw_objects.FromPointer(ptr, rt), llmRenderableObject{}}
	bldr.llmRenderableObject = llmRenderableObject{&bldr}
	return &bldr
}

// Type sets the type for the property
func (cpb *classPropertyBuilder) SetType(fieldType Type) error {
	args := map[string]interface{}{
		"field_type": fieldType,
	}
	_, err := raw_objects.CallMethod(cpb, "set_type", args)
	return err
}

func (cpb *classPropertyBuilder) Type() (Type, error) {
	result, err := raw_objects.CallMethod(cpb, "type_", nil)
	if err != nil {
		return nil, err
	}

	typ, ok := result.(Type)
	if !ok {
		return nil, fmt.Errorf("unexpected type for class property type: %T", result)
	}

	return typ, nil
}

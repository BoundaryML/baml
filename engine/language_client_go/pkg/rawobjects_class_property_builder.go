package baml

import (
	"unsafe"

	"github.com/boundaryml/baml/engine/language_client_go/baml_go/raw_objects"
	"github.com/boundaryml/baml/engine/language_client_go/pkg/cffi"
)

// classPropertyBuilder provides class property construction functionality
type classPropertyBuilder struct {
	*raw_objects.RawObject
}

func (cpb *classPropertyBuilder) ObjectType() cffi.CFFIObjectType {
	return cffi.CFFIObjectType_OBJECT_CLASS_PROPERTY_BUILDER
}

func newClassPropertyBuilder(ptr int64, rt unsafe.Pointer) ClassPropertyBuilder {
	return &classPropertyBuilder{raw_objects.FromPointer(ptr, rt)}
}

// Description sets the description for the property
func (cpb *classPropertyBuilder) Description(description string) error {
	args := map[string]interface{}{
		"description": description,
	}
	_, err := raw_objects.CallMethod(cpb, "description", args)
	return err
}

// Alias sets the alias for the property
func (cpb *classPropertyBuilder) Alias(alias string) error {
	args := map[string]interface{}{
		"alias": alias,
	}
	_, err := raw_objects.CallMethod(cpb, "alias", args)
	return err
}

// Type sets the type for the property
func (cpb *classPropertyBuilder) Type(fieldType Type) error {
	args := map[string]interface{}{
		"field_type": fieldType,
	}
	_, err := raw_objects.CallMethod(cpb, "type_", args)
	return err
}
package baml

import (
	"unsafe"

	"github.com/boundaryml/baml/engine/language_client_go/baml_go/raw_objects"
	"github.com/boundaryml/baml/engine/language_client_go/pkg/cffi"
)

// enumValueBuilder provides enum value construction functionality
type enumValueBuilder struct {
	*raw_objects.RawObject
}

func (evb *enumValueBuilder) ObjectType() cffi.CFFIObjectType {
	return cffi.CFFIObjectType_OBJECT_ENUM_VALUE_BUILDER
}

func newEnumValueBuilder(ptr int64, rt unsafe.Pointer) EnumValueBuilder {
	return &enumValueBuilder{raw_objects.FromPointer(ptr, rt)}
}

// Description sets the description for the enum value
func (evb *enumValueBuilder) Description(description string) error {
	args := map[string]interface{}{
		"description": description,
	}
	_, err := raw_objects.CallMethod(evb, "description", args)
	return err
}

// Alias sets the alias for the enum value
func (evb *enumValueBuilder) Alias(alias string) error {
	args := map[string]interface{}{
		"alias": alias,
	}
	_, err := raw_objects.CallMethod(evb, "alias", args)
	return err
}

// Skip marks the enum value to be skipped
func (evb *enumValueBuilder) Skip() error {
	_, err := raw_objects.CallMethod(evb, "skip", nil)
	return err
}
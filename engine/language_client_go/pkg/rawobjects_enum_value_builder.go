package baml

import (
	"unsafe"

	"github.com/boundaryml/baml/engine/language_client_go/baml_go/raw_objects"
	"github.com/boundaryml/baml/engine/language_client_go/pkg/cffi"
)

// enumValueBuilder provides enum value construction functionality
type enumValueBuilder struct {
	*raw_objects.RawObject
	llmRenderableObject
}

func (evb *enumValueBuilder) ObjectType() cffi.CFFIObjectType {
	return cffi.CFFIObjectType_OBJECT_ENUM_VALUE_BUILDER
}

func newEnumValueBuilder(ptr int64, rt unsafe.Pointer) EnumValueBuilder {
	bldr := enumValueBuilder{raw_objects.FromPointer(ptr, rt), llmRenderableObject{}}
	bldr.llmRenderableObject = llmRenderableObject{&bldr}
	return &bldr
}

func (evb *enumValueBuilder) Skip() (bool, error) {
	skip, err := raw_objects.CallMethod(evb, "skip", nil)
	if err != nil {
		return false, err
	}
	return skip.(bool), nil
}

// SetSkip marks the enum value to be skipped
func (evb *enumValueBuilder) SetSkip(skip bool) error {
	_, err := raw_objects.CallMethod(evb, "set_skip", map[string]any{
		"skip": skip,
	})
	return err
}
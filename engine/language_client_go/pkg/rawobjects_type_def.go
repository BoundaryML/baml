package baml

import (
	"fmt"
	"unsafe"

	"github.com/boundaryml/baml/engine/language_client_go/baml_go/raw_objects"
	"github.com/boundaryml/baml/engine/language_client_go/pkg/cffi"
)

// TypeDefWrapper wraps type definitions
type typeDef struct {
	*raw_objects.RawObject
}

func (t *typeDef) ObjectType() cffi.CFFIObjectType {
	return cffi.CFFIObjectType_OBJECT_TYPE
}


func newType(ptr int64, rt unsafe.Pointer) Type {
	return &typeDef{raw_objects.FromPointer(ptr, rt)}
}

// List wraps this type in a list type
func (t *typeDef) List() (Type, error) {
	result, err := raw_objects.CallMethod(t, "list", nil)
	if err != nil {
		return nil, err
	}

	request, ok := result.(Type)
	if !ok {
		return nil, fmt.Errorf("unexpected type for type: %T", result)
	}

	return request, nil
}

// Optional wraps this type in an optional type
func (t *typeDef) Optional() (Type, error) {
	result, err := raw_objects.CallMethod(t, "optional", nil)
	if err != nil {
		return nil, err
	}

	request, ok := result.(Type)
	if !ok {
		return nil, fmt.Errorf("unexpected type for type: %T", result)
	}

	return request, nil
}

// __display__ returns the string representation of this type (internal method)
func (t *typeDef) Print() string {
	result, err := raw_objects.CallMethod(t, "__display__", nil)
	if err != nil {
		return fmt.Sprintf("<Type: error getting repr: %v>", err)
	}

	repr, ok := result.(string)
	if !ok {
		return fmt.Sprintf("<Type: error getting repr: %T>", result)
	}

	return repr
}

// String implements the fmt.Stringer interface for native Go printing
func (d *typeDef) Format(f fmt.State, verb rune) {
	display := d.Print()
	fmt.Fprint(f, display)
}

func (t *typeDef) InternalBamlSerializer() {
}

func (t *typeDef) Encode() (*cffi.CFFIValueHolder, error) {
	return &cffi.CFFIValueHolder{
		Value: &cffi.CFFIValueHolder_ObjectValue{
			ObjectValue: &cffi.CFFIValueRawObject{
				Object: &cffi.CFFIValueRawObject_Type{
					Type: raw_objects.EncodeRawObject(t),
				},
			},
		},
		Type: nil,
	}, nil
}

func (t *typeDef) Type() (*cffi.CFFIFieldTypeHolder, error) {
	// not necessary here.
	return nil, nil
}
package baml

import (
	"fmt"
	"unsafe"

	"github.com/boundaryml/baml/engine/language_client_go/baml_go/raw_objects"
	"github.com/boundaryml/baml/engine/language_client_go/pkg/cffi"
)

// typeBuilder provides access to BAML type construction functionality
type typeBuilder struct {
	*raw_objects.RawObject
}

func (tb *typeBuilder) ObjectType() cffi.BamlObjectType {
	return cffi.BamlObjectType_OBJECT_TYPE_BUILDER
}

func newTypeBuilder(ptr int64, rt unsafe.Pointer) TypeBuilder {
	return &typeBuilder{raw_objects.FromPointer(ptr, rt)}
}

// Basic types
func (tb *typeBuilder) String() (Type, error) {
	result, err := raw_objects.CallMethod(tb, "string", nil)
	if err != nil {
		return nil, err
	}

	typ, ok := result.(Type)
	if !ok {
		return nil, fmt.Errorf("unexpected type for string type: %T", result)
	}

	return typ, nil
}

func (tb *typeBuilder) Int() (Type, error) {
	result, err := raw_objects.CallMethod(tb, "int", nil)
	if err != nil {
		return nil, err
	}

	typ, ok := result.(Type)
	if !ok {
		return nil, fmt.Errorf("unexpected type for int type: %T", result)
	}

	return typ, nil
}

func (tb *typeBuilder) Float() (Type, error) {
	result, err := raw_objects.CallMethod(tb, "float", nil)
	if err != nil {
		return nil, err
	}

	typ, ok := result.(Type)
	if !ok {
		return nil, fmt.Errorf("unexpected type for float type: %T", result)
	}

	return typ, nil
}

func (tb *typeBuilder) Bool() (Type, error) {
	result, err := raw_objects.CallMethod(tb, "bool", nil)
	if err != nil {
		return nil, err
	}

	typ, ok := result.(Type)
	if !ok {
		return nil, fmt.Errorf("unexpected type for bool type: %T", result)
	}

	return typ, nil
}

func (tb *typeBuilder) Null() (Type, error) {
	result, err := raw_objects.CallMethod(tb, "null", nil)
	if err != nil {
		return nil, err
	}

	typ, ok := result.(Type)
	if !ok {
		return nil, fmt.Errorf("unexpected type for null type: %T", result)
	}

	return typ, nil
}

// Literal types
func (tb *typeBuilder) LiteralString(value string) (Type, error) {
	args := map[string]interface{}{
		"value": value,
	}
	result, err := raw_objects.CallMethod(tb, "literal_string", args)
	if err != nil {
		return nil, err
	}

	typ, ok := result.(Type)
	if !ok {
		return nil, fmt.Errorf("unexpected type for literal string type: %T", result)
	}

	return typ, nil
}

func (tb *typeBuilder) LiteralInt(value int64) (Type, error) {
	args := map[string]interface{}{
		"value": value,
	}
	result, err := raw_objects.CallMethod(tb, "literal_int", args)
	if err != nil {
		return nil, err
	}

	typ, ok := result.(Type)
	if !ok {
		return nil, fmt.Errorf("unexpected type for literal int type: %T", result)
	}

	return typ, nil
}

func (tb *typeBuilder) LiteralBool(value bool) (Type, error) {
	args := map[string]interface{}{
		"value": value,
	}
	result, err := raw_objects.CallMethod(tb, "literal_bool", args)
	if err != nil {
		return nil, err
	}

	typ, ok := result.(Type)
	if !ok {
		return nil, fmt.Errorf("unexpected type for literal bool type: %T", result)
	}

	return typ, nil
}

// Composite types
func (tb *typeBuilder) Map(key Type, value Type) (Type, error) {
	args := map[string]interface{}{
		"key":   key,
		"value": value,
	}
	result, err := raw_objects.CallMethod(tb, "map", args)
	if err != nil {
		return nil, err
	}

	typ, ok := result.(Type)
	if !ok {
		return nil, fmt.Errorf("unexpected type for map type: %T", result)
	}

	return typ, nil
}

func (tb *typeBuilder) List(inner Type) (Type, error) {
	args := map[string]interface{}{
		"inner": inner,
	}
	result, err := raw_objects.CallMethod(tb, "list", args)
	if err != nil {
		return nil, err
	}

	typ, ok := result.(Type)
	if !ok {
		return nil, fmt.Errorf("unexpected type for list type: %T", result)
	}

	return typ, nil
}

func (tb *typeBuilder) Optional(inner Type) (Type, error) {
	args := map[string]interface{}{
		"inner": inner,
	}
	result, err := raw_objects.CallMethod(tb, "optional", args)
	if err != nil {
		return nil, err
	}

	typ, ok := result.(Type)
	if !ok {
		return nil, fmt.Errorf("unexpected type for optional type: %T", result)
	}

	return typ, nil
}

func (tb *typeBuilder) Union(types []Type) (Type, error) {
	args := map[string]interface{}{
		"types": types,
	}
	result, err := raw_objects.CallMethod(tb, "union", args)
	if err != nil {
		return nil, err
	}

	typ, ok := result.(Type)
	if !ok {
		return nil, fmt.Errorf("unexpected type for union type: %T", result)
	}

	return typ, nil
}

// BAML schema operations
func (tb *typeBuilder) AddBaml(baml string) error {
	args := map[string]interface{}{
		"baml": baml,
	}
	_, err := raw_objects.CallMethod(tb, "add_baml", args)
	return err
}

// Enum operations
func (tb *typeBuilder) AddEnum(name string) (EnumBuilder, error) {
	args := map[string]interface{}{
		"name": name,
	}
	result, err := raw_objects.CallMethod(tb, "add_enum", args)
	if err != nil {
		return nil, err
	}

	enumBuilder, ok := result.(EnumBuilder)
	if !ok {
		return nil, fmt.Errorf("unexpected type for enum builder: %T", result)
	}

	return enumBuilder, nil
}

func (tb *typeBuilder) Enum(name string) (EnumBuilder, error) {
	args := map[string]interface{}{
		"name": name,
	}
	result, err := raw_objects.CallMethod(tb, "enum_", args)
	if err != nil {
		return nil, err
	}

	enumBuilder, ok := result.(EnumBuilder)
	if !ok {
		return nil, fmt.Errorf("unexpected type for enum builder: %T", result)
	}

	return enumBuilder, nil
}

func (tb *typeBuilder) ListEnums() ([]EnumBuilder, error) {
	result, err := raw_objects.CallMethod(tb, "list_enums", nil)
	if err != nil {
		return nil, err
	}

	rawObjects, ok := result.([]raw_objects.RawPointer)
	if !ok {
		return nil, fmt.Errorf("unexpected type for enum builders: %T", result)
	}

	enumBuilders := make([]EnumBuilder, len(rawObjects))
	for i, rawObject := range rawObjects {
		enumBuilders[i] = rawObject.(EnumBuilder)
	}

	return enumBuilders, nil
}

// Class operations
func (tb *typeBuilder) AddClass(name string) (ClassBuilder, error) {
	args := map[string]interface{}{
		"name": name,
	}
	result, err := raw_objects.CallMethod(tb, "add_class", args)
	if err != nil {
		return nil, err
	}

	classBuilder, ok := result.(ClassBuilder)
	if !ok {
		return nil, fmt.Errorf("unexpected type for class builder: %T", result)
	}

	return classBuilder, nil
}

func (tb *typeBuilder) Class(name string) (ClassBuilder, error) {
	args := map[string]interface{}{
		"name": name,
	}
	result, err := raw_objects.CallMethod(tb, "class", args)
	if err != nil {
		return nil, err
	}

	classBuilder, ok := result.(ClassBuilder)
	if !ok {
		return nil, fmt.Errorf("unexpected type for class builder: %T", result)
	}

	return classBuilder, nil
}

func (tb *typeBuilder) ListClasses() ([]ClassBuilder, error) {
	result, err := raw_objects.CallMethod(tb, "list_classes", nil)
	if err != nil {
		return nil, err
	}

	rawObjects, ok := result.([]raw_objects.RawPointer)
	if !ok {
		return nil, fmt.Errorf("unexpected type for class builders: %T", result)
	}

	classBuilders := make([]ClassBuilder, len(rawObjects))
	for i, rawObject := range rawObjects {
		classBuilders[i] = rawObject.(ClassBuilder)
	}

	return classBuilders, nil
}

// __display__ returns the string representation of this type (internal method)
func (t *typeBuilder) Print() string {
	result, err := raw_objects.CallMethod(t, "__display__", nil)
	if err != nil {
		return fmt.Sprintf("<TypeBuilder: error getting repr: %v>", err)
	}

	repr, ok := result.(string)
	if !ok {
		return fmt.Sprintf("<TypeBuilder: error getting repr: %T>", result)
	}

	return repr
}

// String implements the fmt.Stringer interface for native Go printing
func (d *typeBuilder) Format(f fmt.State, verb rune) {
	display := d.Print()
	fmt.Fprint(f, display)
}

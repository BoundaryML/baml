package baml_go

import (
	"fmt"
	"math"
	"math/big"
	"strconv"

	"github.com/boundaryml/baml-go/internal/cffi"
	"google.golang.org/protobuf/proto"
)

// BAMLType is the opaque Go representation of a first-class BAML `type` value.
// Generated union codecs also use it to dispatch selected arms. Its protobuf
// representation remains private so generated code cannot depend on wire
// implementation details or parse diagnostic type strings.
type BAMLType struct {
	value      *cffi.BamlTy
	definition *cffi.BamlTyDef
	err        error
}

// BAMLTypeMetadata is the ordered metadata row accepted by reflected class
// fields and enum values. It is data, not another type handle.
type BAMLTypeMetadata struct {
	Type        BAMLType
	Alias       *string
	Description *string
	Docstring   *string
	Other       map[string]string
}

// MetadataOption applies one schema annotation. The generated reflect facade
// re-exports these options so the same spelling works for fields, enum values,
// and BAMLType.Meta.
type MetadataOption func(*BAMLTypeMetadata)

func WithAlias(value string) MetadataOption {
	return func(metadata *BAMLTypeMetadata) { metadata.Alias = &value }
}

func WithDescription(value string) MetadataOption {
	return func(metadata *BAMLTypeMetadata) { metadata.Description = &value }
}

func WithDocstring(value string) MetadataOption {
	return func(metadata *BAMLTypeMetadata) { metadata.Docstring = &value }
}

func WithOther(value map[string]string) MetadataOption {
	return func(metadata *BAMLTypeMetadata) {
		metadata.Other = make(map[string]string, len(value))
		for key, item := range value {
			metadata.Other[key] = item
		}
	}
}

// Meta pairs a type with schema metadata without mutating the opaque type.
func (value BAMLType) Meta(options ...MetadataOption) BAMLTypeMetadata {
	metadata := BAMLTypeMetadata{Type: value, Other: map[string]string{}}
	for _, option := range options {
		if option != nil {
			option(&metadata)
		}
	}
	return metadata
}

// Array and Optional are the only type-graph composition operations exposed
// to hosts. Definition tables are copied intact and only the root changes.
func (value BAMLType) Array() BAMLType {
	return value.wrapDefinition(func(root *cffi.BamlTy) *cffi.BamlTy {
		return &cffi.BamlTy{Ty: &cffi.BamlTy_List{List: &cffi.BamlTyList{Item: root}}}
	})
}

func (value BAMLType) Optional() BAMLType {
	return value.wrapDefinition(func(root *cffi.BamlTy) *cffi.BamlTy {
		return &cffi.BamlTy{Ty: &cffi.BamlTy_Optional{Optional: &cffi.BamlTyOptional{Inner: root}}}
	})
}

func (value BAMLType) wrapDefinition(wrap func(*cffi.BamlTy) *cffi.BamlTy) BAMLType {
	if value.err != nil {
		return value
	}
	definition, err := value.definitionCopy()
	if err != nil {
		return BAMLType{err: err}
	}
	definition.Root = wrap(definition.Root)
	return BAMLType{definition: definition}
}

func (value BAMLType) definitionCopy() (*cffi.BamlTyDef, error) {
	if value.err != nil {
		return nil, value.err
	}
	if value.definition != nil {
		return proto.Clone(value.definition).(*cffi.BamlTyDef), nil
	}
	if value.value == nil {
		return nil, fmt.Errorf("descriptor is missing its type")
	}
	return &cffi.BamlTyDef{Root: proto.Clone(value.value).(*cffi.BamlTy)}, nil
}

func (value BAMLType) root() *cffi.BamlTy {
	if value.definition != nil {
		return value.definition.Root
	}
	return value.value
}

// GobEncode and MarshalJSON deliberately reject persistence. A BAMLType is a
// process-local capability whose portable definition is only transported by
// the BAML wire protocol.
func (BAMLType) GobEncode() ([]byte, error) {
	return nil, fmt.Errorf("BAMLType values are runtime handles and cannot be serialized")
}

func (BAMLType) MarshalJSON() ([]byte, error) {
	return nil, fmt.Errorf("BAMLType values are runtime handles and cannot be serialized")
}

type resultOwner struct {
	keys []uint64
}

type PrimitiveType int

const (
	StringType PrimitiveType = iota + 1
	IntType
	FloatType
	BoolType
	NullType
	BytesType
	BigintType
)

func PrimitiveBAMLType(kind PrimitiveType) BAMLType {
	protoKind := cffi.BamlTyPrimitiveKind_BAML_TY_PRIMITIVE_UNSPECIFIED
	switch kind {
	case StringType:
		protoKind = cffi.BamlTyPrimitiveKind_BAML_TY_PRIMITIVE_STRING
	case IntType:
		protoKind = cffi.BamlTyPrimitiveKind_BAML_TY_PRIMITIVE_INT
	case FloatType:
		protoKind = cffi.BamlTyPrimitiveKind_BAML_TY_PRIMITIVE_FLOAT
	case BoolType:
		protoKind = cffi.BamlTyPrimitiveKind_BAML_TY_PRIMITIVE_BOOL
	case NullType:
		protoKind = cffi.BamlTyPrimitiveKind_BAML_TY_PRIMITIVE_NULL
	case BytesType:
		protoKind = cffi.BamlTyPrimitiveKind_BAML_TY_PRIMITIVE_BYTES
	case BigintType:
		protoKind = cffi.BamlTyPrimitiveKind_BAML_TY_PRIMITIVE_BIGINT
	}
	return BAMLType{value: &cffi.BamlTy{Ty: &cffi.BamlTy_Primitive{Primitive: &cffi.BamlTyPrimitive{Kind: protoKind}}}}
}

func ClassBAMLType(name string, typeArgs ...BAMLType) BAMLType {
	encoded := make([]*cffi.BamlTy, len(typeArgs))
	for index, argument := range typeArgs {
		encoded[index] = argument.root()
	}
	return BAMLType{value: &cffi.BamlTy{Ty: &cffi.BamlTy_ClassTy{ClassTy: &cffi.BamlTyClass{Name: name, TypeArgs: encoded}}}}
}

func EnumBAMLType(name string) BAMLType {
	return BAMLType{value: &cffi.BamlTy{Ty: &cffi.BamlTy_Enum{Enum: &cffi.BamlTyEnum{Name: name}}}}
}

// InterfaceBAMLType is the erased token emitted for an interface which is
// present in the public generated API.
func InterfaceBAMLType(name string) BAMLType {
	return BAMLType{value: &cffi.BamlTy{Ty: &cffi.BamlTy_Interface{Interface: &cffi.BamlTyInterface{Name: name}}}}
}

// EnumVariantBAMLType describes one narrowed enum variant while the Go value
// continues to use the owning generated enum type.
func EnumVariantBAMLType(name, variant string) BAMLType {
	return BAMLType{value: &cffi.BamlTy{Ty: &cffi.BamlTy_EnumVariant{EnumVariant: &cffi.BamlTyEnumVariant{
		Name: name, Variant: variant,
	}}}}
}

// TypeAliasBAMLType describes a named BAML type alias. Generated union
// codecs use this when an alias is itself a selected arm; the native runtime
// remains authoritative for resolving and validating the alias body.
func TypeAliasBAMLType(name string) BAMLType {
	return BAMLType{value: &cffi.BamlTy{Ty: &cffi.BamlTy_TypeAlias{TypeAlias: &cffi.BamlTyTypeAlias{Name: name}}}}
}

func ListBAMLType(item BAMLType) BAMLType {
	if item.definition != nil || item.err != nil {
		return item.Array()
	}
	return BAMLType{value: &cffi.BamlTy{Ty: &cffi.BamlTy_List{List: &cffi.BamlTyList{Item: item.value}}}}
}

func MapBAMLType(key, value BAMLType) BAMLType {
	return BAMLType{value: &cffi.BamlTy{Ty: &cffi.BamlTy_Map{Map: &cffi.BamlTyMap{Key: key.value, Value: value.value}}}}
}

func OptionalBAMLType(inner BAMLType) BAMLType {
	if inner.definition != nil || inner.err != nil {
		return inner.Optional()
	}
	return BAMLType{value: &cffi.BamlTy{Ty: &cffi.BamlTy_Optional{Optional: &cffi.BamlTyOptional{Inner: inner.value}}}}
}

func UnionBAMLType(options ...BAMLType) BAMLType {
	encoded := make([]*cffi.BamlTy, len(options))
	for index, option := range options {
		encoded[index] = option.value
	}
	return BAMLType{value: &cffi.BamlTy{Ty: &cffi.BamlTy_Union{Union: &cffi.BamlTyUnion{Options: encoded}}}}
}

func StringLiteralBAMLType(value string) BAMLType {
	return BAMLType{value: &cffi.BamlTy{Ty: &cffi.BamlTy_Literal{Literal: &cffi.BamlTyLiteral{Literal: &cffi.BamlTyLiteral_StringValue{StringValue: value}}}}}
}

func IntLiteralBAMLType(value int64) BAMLType {
	return BAMLType{value: &cffi.BamlTy{Ty: &cffi.BamlTy_Literal{Literal: &cffi.BamlTyLiteral{Literal: &cffi.BamlTyLiteral_IntValue{IntValue: value}}}}}
}

func BigintLiteralBAMLType(value string) BAMLType {
	return BAMLType{value: &cffi.BamlTy{Ty: &cffi.BamlTy_Literal{Literal: &cffi.BamlTyLiteral{Literal: &cffi.BamlTyLiteral_BigintValue{BigintValue: value}}}}}
}

func FloatLiteralBAMLType(value string) BAMLType {
	return BAMLType{value: &cffi.BamlTy{Ty: &cffi.BamlTy_Literal{Literal: &cffi.BamlTyLiteral{Literal: &cffi.BamlTyLiteral_FloatValue{FloatValue: value}}}}}
}

func BoolLiteralBAMLType(value bool) BAMLType {
	return BAMLType{value: &cffi.BamlTy{Ty: &cffi.BamlTy_Literal{Literal: &cffi.BamlTyLiteral{Literal: &cffi.BamlTyLiteral_BoolValue{BoolValue: value}}}}}
}

// MetaTypeBAMLType describes BAML's `type` metatype. A BAMLType value itself
// is a runtime instance of this metatype.
func MetaTypeBAMLType() BAMLType {
	return BAMLType{value: &cffi.BamlTy{Ty: &cffi.BamlTy_MetaType{MetaType: &cffi.BamlTyMetaType{}}}}
}

// RustTypeBAMLType describes BAML's opaque `$rust_type` leaf. Values of this
// type are native-owned handle-table entries and never expose their payload to
// Go.
func RustTypeBAMLType() BAMLType {
	return BAMLType{value: &cffi.BamlTy{Ty: &cffi.BamlTy_RustType{RustType: &cffi.BamlTyRustType{}}}}
}

// Equal compares complete BAML type descriptors. Optionality is significant
// at every level, while union member order is not.
func (value BAMLType) Equal(other BAMLType) bool {
	if value.definition != nil || other.definition != nil {
		left, leftErr := value.definitionCopy()
		right, rightErr := other.definitionCopy()
		return leftErr == nil && rightErr == nil && proto.Equal(left, right)
	}
	return equalBAMLType(value.value, other.value, false)
}

// MatchesUnionArm implements the native runtime's legacy selected-arm rule:
// a single Optional wrapper at the selected arm's root is ignored. Generated
// union decoders use this method; application code normally wants Equal.
func (value BAMLType) MatchesUnionArm(other BAMLType) bool {
	return equalBAMLType(value.root(), other.root(), true)
}

func validateBAMLTypeValue(value BAMLType) error {
	if value.err != nil {
		return value.err
	}
	if value.definition != nil {
		return validateBAMLTypeDefinition(value.definition)
	}
	return validateBAMLType(value.value, 0)
}

func validateBAMLTypeDefinition(definition *cffi.BamlTyDef) error {
	if definition == nil {
		return fmt.Errorf("definition is missing")
	}
	if err := validateBAMLType(definition.Root, 0); err != nil {
		return fmt.Errorf("definition root: %w", err)
	}
	classNames := make(map[string]struct{}, len(definition.Classes))
	for classIndex, class := range definition.Classes {
		if class == nil || class.Name == "" {
			return fmt.Errorf("class definition %d has no name", classIndex)
		}
		if _, duplicate := classNames[class.Name]; duplicate {
			return fmt.Errorf("duplicate class definition %q", class.Name)
		}
		classNames[class.Name] = struct{}{}
		fieldNames := make(map[string]struct{}, len(class.Fields))
		for fieldIndex, field := range class.Fields {
			if field == nil || field.Name == "" {
				return fmt.Errorf("class %q field %d has no name", class.Name, fieldIndex)
			}
			if _, duplicate := fieldNames[field.Name]; duplicate {
				return fmt.Errorf("class %q has duplicate field %q", class.Name, field.Name)
			}
			fieldNames[field.Name] = struct{}{}
			if err := validateBAMLType(field.Ty, 0); err != nil {
				return fmt.Errorf("class %q field %q: %w", class.Name, field.Name, err)
			}
		}
	}
	enumNames := make(map[string]struct{}, len(definition.Enums))
	for enumIndex, enum := range definition.Enums {
		if enum == nil || enum.Name == "" {
			return fmt.Errorf("enum definition %d has no name", enumIndex)
		}
		if _, duplicate := enumNames[enum.Name]; duplicate {
			return fmt.Errorf("duplicate enum definition %q", enum.Name)
		}
		enumNames[enum.Name] = struct{}{}
		variantNames := make(map[string]struct{}, len(enum.Variants))
		for variantIndex, variant := range enum.Variants {
			if variant == nil || variant.Name == "" {
				return fmt.Errorf("enum %q variant %d has no name", enum.Name, variantIndex)
			}
			if _, duplicate := variantNames[variant.Name]; duplicate {
				return fmt.Errorf("enum %q has duplicate variant %q", enum.Name, variant.Name)
			}
			variantNames[variant.Name] = struct{}{}
		}
	}
	for index, witness := range definition.Witnesses {
		if witness == nil || witness.Interface == "" {
			return fmt.Errorf("witness definition %d has no interface", index)
		}
		for argumentIndex, argument := range witness.InterfaceArgs {
			if err := validateBAMLType(argument, 0); err != nil {
				return fmt.Errorf("witness %q argument %d: %w", witness.Interface, argumentIndex, err)
			}
		}
	}
	return nil
}

func equalBAMLType(left, right *cffi.BamlTy, allowTopLevelOptional bool) bool {
	if left == nil || right == nil {
		return left == right
	}
	// RuntimeTy serializes a selected non-null arm from a flattened nullable
	// union as Optional<T> in a few legacy paths. Tolerate that wrapper only at
	// the selected arm's root. Recursive calls deliberately pass false so
	// list<int> remains distinct from list<int?>.
	if allowTopLevelOptional {
		leftInner, leftIsOptional := soleNonNullBAMLType(left)
		rightInner, rightIsOptional := soleNonNullBAMLType(right)
		switch {
		case leftIsOptional && rightIsOptional:
			return equalBAMLType(leftInner, rightInner, false)
		case leftIsOptional:
			return equalBAMLType(leftInner, right, false)
		case rightIsOptional:
			return equalBAMLType(left, rightInner, false)
		}
	}
	leftOptions, leftIsUnion := semanticUnionOptions(left)
	rightOptions, rightIsUnion := semanticUnionOptions(right)
	if leftIsUnion || rightIsUnion {
		if !leftIsUnion || !rightIsUnion || len(leftOptions) != len(rightOptions) {
			return false
		}
		matched := make([]bool, len(rightOptions))
		for _, leftOption := range leftOptions {
			found := false
			for index, rightOption := range rightOptions {
				if !matched[index] && equalBAMLType(leftOption, rightOption, false) {
					matched[index] = true
					found = true
					break
				}
			}
			if !found {
				return false
			}
		}
		return true
	}
	switch leftValue := left.Ty.(type) {
	case *cffi.BamlTy_List:
		rightValue, ok := right.Ty.(*cffi.BamlTy_List)
		return ok && leftValue.List != nil && rightValue.List != nil && equalBAMLType(leftValue.List.Item, rightValue.List.Item, false)
	case *cffi.BamlTy_Map:
		rightValue, ok := right.Ty.(*cffi.BamlTy_Map)
		return ok && leftValue.Map != nil && rightValue.Map != nil &&
			equalBAMLType(leftValue.Map.Key, rightValue.Map.Key, false) && equalBAMLType(leftValue.Map.Value, rightValue.Map.Value, false)
	default:
		// Optionality is semantically significant at every nested position:
		// list<int> must not compare equal to list<int?>. Selected union arms
		// arrive in canonical non-optional form, so no wrapper erasure is
		// needed here.
		return proto.Equal(left, right)
	}
}

func semanticUnionOptions(value *cffi.BamlTy) ([]*cffi.BamlTy, bool) {
	var raw []*cffi.BamlTy
	switch item := value.Ty.(type) {
	case *cffi.BamlTy_Optional:
		if item.Optional == nil || item.Optional.Inner == nil {
			return nil, false
		}
		raw = []*cffi.BamlTy{item.Optional.Inner, PrimitiveBAMLType(NullType).value}
	case *cffi.BamlTy_Union:
		if item.Union == nil {
			return nil, false
		}
		raw = item.Union.Options
	default:
		return nil, false
	}
	flat := make([]*cffi.BamlTy, 0, len(raw))
	for _, option := range raw {
		if option == nil {
			flat = append(flat, option)
			continue
		}
		if nested, ok := semanticUnionOptions(option); ok {
			flat = append(flat, nested...)
		} else {
			flat = append(flat, option)
		}
	}
	unique := flat[:0]
	for _, option := range flat {
		duplicate := false
		for _, existing := range unique {
			if equalBAMLType(option, existing, false) {
				duplicate = true
				break
			}
		}
		if !duplicate {
			unique = append(unique, option)
		}
	}
	return unique, true
}

func soleNonNullBAMLType(value *cffi.BamlTy) (*cffi.BamlTy, bool) {
	options, ok := semanticUnionOptions(value)
	if !ok {
		return nil, false
	}
	var nonNull *cffi.BamlTy
	seenNull := false
	for _, option := range options {
		if option == nil {
			return nil, false
		}
		primitive, isPrimitive := option.Ty.(*cffi.BamlTy_Primitive)
		if isPrimitive && primitive.Primitive != nil && primitive.Primitive.Kind == cffi.BamlTyPrimitiveKind_BAML_TY_PRIMITIVE_NULL {
			seenNull = true
			continue
		}
		if nonNull != nil {
			return nil, false
		}
		nonNull = option
	}
	return nonNull, seenNull && nonNull != nil
}

func validateBAMLType(value *cffi.BamlTy, depth int) error {
	if depth > 256 {
		return fmt.Errorf("descriptor nesting exceeds 256 levels")
	}
	if value == nil || value.Ty == nil {
		return fmt.Errorf("descriptor is missing its type")
	}
	validateMany := func(values []*cffi.BamlTy) error {
		for index, item := range values {
			if err := validateBAMLType(item, depth+1); err != nil {
				return fmt.Errorf("type argument %d: %w", index, err)
			}
		}
		return nil
	}
	switch item := value.Ty.(type) {
	case *cffi.BamlTy_Primitive:
		if item.Primitive == nil || item.Primitive.Kind == cffi.BamlTyPrimitiveKind_BAML_TY_PRIMITIVE_UNSPECIFIED {
			return fmt.Errorf("primitive descriptor has no kind")
		}
		if _, known := cffi.BamlTyPrimitiveKind_name[int32(item.Primitive.Kind)]; !known {
			return fmt.Errorf("primitive descriptor has unknown kind %d", item.Primitive.Kind)
		}
	case *cffi.BamlTy_ClassTy:
		if item.ClassTy == nil || item.ClassTy.Name == "" {
			return fmt.Errorf("class descriptor has no name")
		}
		return validateMany(item.ClassTy.TypeArgs)
	case *cffi.BamlTy_Enum:
		if item.Enum == nil || item.Enum.Name == "" {
			return fmt.Errorf("enum descriptor has no name")
		}
	case *cffi.BamlTy_List:
		if item.List == nil {
			return fmt.Errorf("list descriptor is missing")
		}
		return validateBAMLType(item.List.Item, depth+1)
	case *cffi.BamlTy_Map:
		if item.Map == nil {
			return fmt.Errorf("map descriptor is missing")
		}
		if err := validateBAMLType(item.Map.Key, depth+1); err != nil {
			return fmt.Errorf("map key: %w", err)
		}
		if err := validateBAMLType(item.Map.Value, depth+1); err != nil {
			return fmt.Errorf("map value: %w", err)
		}
	case *cffi.BamlTy_Optional:
		if item.Optional == nil {
			return fmt.Errorf("optional descriptor is missing")
		}
		return validateBAMLType(item.Optional.Inner, depth+1)
	case *cffi.BamlTy_Union:
		if item.Union == nil || len(item.Union.Options) == 0 {
			return fmt.Errorf("union descriptor has no options")
		}
		return validateMany(item.Union.Options)
	case *cffi.BamlTy_Literal:
		if item.Literal == nil || item.Literal.Literal == nil {
			return fmt.Errorf("literal descriptor has no value")
		}
		switch literal := item.Literal.Literal.(type) {
		case *cffi.BamlTyLiteral_BigintValue:
			if _, ok := new(big.Int).SetString(literal.BigintValue, 10); !ok {
				return fmt.Errorf("bigint literal descriptor is not decimal")
			}
		case *cffi.BamlTyLiteral_FloatValue:
			parsed, err := strconv.ParseFloat(literal.FloatValue, 64)
			if err != nil {
				numberError, rangeError := err.(*strconv.NumError)
				if !rangeError || numberError.Err != strconv.ErrRange || math.IsInf(parsed, 0) {
					return fmt.Errorf("float literal descriptor is not finite decimal source")
				}
			}
			if math.IsNaN(parsed) || math.IsInf(parsed, 0) {
				return fmt.Errorf("float literal descriptor is not finite decimal source")
			}
		}
	case *cffi.BamlTy_TypeAlias:
		if item.TypeAlias == nil || item.TypeAlias.Name == "" {
			return fmt.Errorf("type alias descriptor has no name")
		}
		if len(item.TypeAlias.TypeArgs) != 0 {
			return fmt.Errorf("type alias descriptor carries unsupported generic arguments")
		}
	case *cffi.BamlTy_Unknown:
		if item.Unknown == nil {
			return fmt.Errorf("unknown descriptor is missing")
		}
	case *cffi.BamlTy_Media:
		if item.Media == nil || item.Media.Kind == cffi.BamlTyMediaKind_BAML_TY_MEDIA_KIND_UNSPECIFIED {
			return fmt.Errorf("media descriptor has no kind")
		}
		if _, known := cffi.BamlTyMediaKind_name[int32(item.Media.Kind)]; !known {
			return fmt.Errorf("media descriptor has unknown kind %d", item.Media.Kind)
		}
	case *cffi.BamlTy_Interface:
		if item.Interface == nil || item.Interface.Name == "" {
			return fmt.Errorf("interface descriptor has no name")
		}
		if err := validateMany(item.Interface.TypeArgs); err != nil {
			return err
		}
		for index, binding := range item.Interface.Bindings {
			if binding == nil || binding.Name == "" {
				return fmt.Errorf("associated binding %d has no name", index)
			}
			if err := validateBAMLType(binding.Ty, depth+1); err != nil {
				return fmt.Errorf("associated binding %q: %w", binding.Name, err)
			}
		}
	case *cffi.BamlTy_EnumVariant:
		if item.EnumVariant == nil || item.EnumVariant.Name == "" || item.EnumVariant.Variant == "" {
			return fmt.Errorf("enum variant descriptor is incomplete")
		}
	case *cffi.BamlTy_Function:
		if item.Function == nil {
			return fmt.Errorf("function descriptor is missing")
		}
		for index, parameter := range item.Function.Params {
			if parameter == nil {
				return fmt.Errorf("function parameter %d is missing", index)
			}
			if err := validateBAMLType(parameter.Ty, depth+1); err != nil {
				return fmt.Errorf("function parameter %d: %w", index, err)
			}
			if parameter.Mode == cffi.BamlTyFunctionParamMode_BAML_TY_FUNCTION_PARAM_MODE_UNSPECIFIED {
				return fmt.Errorf("function parameter %d has no mode", index)
			}
			if _, known := cffi.BamlTyFunctionParamMode_name[int32(parameter.Mode)]; !known {
				return fmt.Errorf("function parameter %d has unknown mode %d", index, parameter.Mode)
			}
		}
		if err := validateBAMLType(item.Function.Ret, depth+1); err != nil {
			return fmt.Errorf("function return: %w", err)
		}
		if err := validateBAMLType(item.Function.Throws, depth+1); err != nil {
			return fmt.Errorf("function throws: %w", err)
		}
	case *cffi.BamlTy_Future:
		if item.Future == nil {
			return fmt.Errorf("future descriptor is missing")
		}
		if err := validateBAMLType(item.Future.Value, depth+1); err != nil {
			return fmt.Errorf("future value: %w", err)
		}
		if err := validateBAMLType(item.Future.Error, depth+1); err != nil {
			return fmt.Errorf("future error: %w", err)
		}
	case *cffi.BamlTy_RustType:
		if item.RustType == nil {
			return fmt.Errorf("rust type descriptor is missing")
		}
	case *cffi.BamlTy_MetaType:
		if item.MetaType == nil {
			return fmt.Errorf("metatype descriptor is missing")
		}
	case *cffi.BamlTy_Resource:
		if item.Resource == nil {
			return fmt.Errorf("resource descriptor is missing")
		}
	case *cffi.BamlTy_PromptAst:
		if item.PromptAst == nil {
			return fmt.Errorf("prompt AST descriptor is missing")
		}
	case *cffi.BamlTy_Void:
		if item.Void == nil {
			return fmt.Errorf("void descriptor is missing")
		}
	case *cffi.BamlTy_TypeVar:
		if item.TypeVar == nil || item.TypeVar.Name == "" {
			return fmt.Errorf("type variable descriptor has no name")
		}
	case *cffi.BamlTy_AssociatedTypeProjection:
		if item.AssociatedTypeProjection == nil || item.AssociatedTypeProjection.Member == "" {
			return fmt.Errorf("associated type projection is incomplete")
		}
		if err := validateBAMLType(item.AssociatedTypeProjection.Base, depth+1); err != nil {
			return fmt.Errorf("associated type projection base: %w", err)
		}
		if item.AssociatedTypeProjection.Interface == nil {
			return fmt.Errorf("associated type projection has no interface")
		}
		if _, ok := item.AssociatedTypeProjection.Interface.Ty.(*cffi.BamlTy_Interface); !ok {
			return fmt.Errorf("associated type projection interface is not an interface descriptor")
		}
		if err := validateBAMLType(item.AssociatedTypeProjection.Interface, depth+1); err != nil {
			return fmt.Errorf("associated type projection interface: %w", err)
		}
	case *cffi.BamlTy_Never:
		if item.Never == nil {
			return fmt.Errorf("never descriptor is missing")
		}
	default:
		return fmt.Errorf("unsupported descriptor variant %T", value.Ty)
	}
	return nil
}

// UnionVariant returns the typed selected arm and its payload without guessing
// from the payload shape.
func (value Value) UnionVariant() (BAMLType, Value, error) {
	if value.value == nil {
		return BAMLType{}, Value{}, fmt.Errorf("BAML value is uninitialized")
	}
	envelope, ok := value.value.Value.(*cffi.BamlOutboundValue_UnionVariantValue)
	if !ok || envelope.UnionVariantValue == nil {
		return BAMLType{}, Value{}, fmt.Errorf("expected BAML union variant, got %T", value.value.Value)
	}
	selected, payload, err := validateUnionVariant(envelope.UnionVariantValue)
	payload.owner = value.owner
	return selected, payload, err
}

// validateUnionVariant treats outbound union metadata as untrusted ABI input.
// The selected arm is derived exclusively from the canonical index into the
// full self type.
func validateUnionVariant(variant *cffi.BamlValueUnionVariant) (BAMLType, Value, error) {
	if variant == nil {
		return BAMLType{}, Value{}, fmt.Errorf("BAML union variant metadata is missing")
	}
	if variant.SelfType == nil || variant.SelfType.Ty == nil {
		return BAMLType{}, Value{}, fmt.Errorf("BAML union variant is missing self type metadata")
	}
	if variant.Value == nil {
		return BAMLType{}, Value{}, fmt.Errorf("BAML union variant has an empty value")
	}
	if variant.SelectedOptionIndex == nil {
		return BAMLType{}, Value{}, fmt.Errorf("BAML union variant is missing selected option index")
	}

	options, err := unionTypeOptions(variant.SelfType)
	if err != nil {
		return BAMLType{}, Value{}, err
	}
	rawIndex := *variant.SelectedOptionIndex
	if uint64(rawIndex) >= uint64(len(options)) {
		return BAMLType{}, Value{}, fmt.Errorf("BAML union variant selected option index %d is outside self type with %d options", rawIndex, len(options))
	}
	selectedType := options[int(rawIndex)]
	if err := validateBAMLType(selectedType, 0); err != nil {
		return BAMLType{}, Value{}, fmt.Errorf("BAML union variant selected option index %d has invalid type metadata: %w", rawIndex, err)
	}
	return BAMLType{value: selectedType}, Value{value: variant.Value}, nil
}

func unionTypeOptions(selfType *cffi.BamlTy) ([]*cffi.BamlTy, error) {
	switch union := selfType.Ty.(type) {
	case *cffi.BamlTy_Union:
		if union.Union == nil || len(union.Union.Options) == 0 {
			return nil, fmt.Errorf("BAML union variant self type has no options")
		}
		return union.Union.Options, nil
	case *cffi.BamlTy_Optional:
		if union.Optional == nil || union.Optional.Inner == nil || union.Optional.Inner.Ty == nil {
			return nil, fmt.Errorf("BAML union variant optional self type is missing its inner type")
		}
		// RuntimeTy's canonical nullable order is [inner, null]. This is also
		// the order used by selected_option_index in the Rust encoder.
		return []*cffi.BamlTy{
			union.Optional.Inner,
			PrimitiveBAMLType(NullType).value,
		}, nil
	default:
		return nil, fmt.Errorf("BAML union variant self type is not a union or optional type")
	}
}

func (value Value) IsNull() (bool, error) {
	return value.isNull()
}

func UnexpectedUnionVariant(name string, selected BAMLType) error {
	return fmt.Errorf("BAML returned selected arm %v outside generated union %s", selected.value, name)
}

func (value Value) isNull() (bool, error) {
	unwrapped, err := value.unwrapUnionVariants()
	if err != nil {
		return false, err
	}
	value = unwrapped
	if value.value == nil {
		return false, fmt.Errorf("BAML value is uninitialized")
	}
	// The CFFI ABI encodes BAML null as an absent oneof. The explicit
	// null_value arm is also accepted for forward compatibility.
	if value.value.Value == nil {
		return true, nil
	}
	_, ok := value.value.Value.(*cffi.BamlOutboundValue_NullValue)
	return ok, nil
}

func (value Value) String() (string, error) {
	unwrapped, err := value.unwrapUnionVariants()
	if err != nil {
		return "", err
	}
	value = unwrapped
	if value.value == nil {
		return "", fmt.Errorf("BAML value is uninitialized")
	}
	switch item := value.value.Value.(type) {
	case *cffi.BamlOutboundValue_StringValue:
		return item.StringValue, nil
	case *cffi.BamlOutboundValue_LiteralValue:
		if item.LiteralValue == nil {
			break
		}
		if literal, ok := item.LiteralValue.Literal.(*cffi.BamlLiteralValue_StringValue); ok {
			return literal.StringValue, nil
		}
	}
	return "", fmt.Errorf("expected BAML string, got %T", value.value.Value)
}

func (value Value) Int64() (int64, error) {
	unwrapped, err := value.unwrapUnionVariants()
	if err != nil {
		return 0, err
	}
	value = unwrapped
	if value.value == nil {
		return 0, fmt.Errorf("BAML value is uninitialized")
	}
	switch item := value.value.Value.(type) {
	case *cffi.BamlOutboundValue_IntValue:
		return item.IntValue, nil
	case *cffi.BamlOutboundValue_LiteralValue:
		if item.LiteralValue == nil {
			break
		}
		if literal, ok := item.LiteralValue.Literal.(*cffi.BamlLiteralValue_IntValue); ok {
			return literal.IntValue, nil
		}
	}
	return 0, fmt.Errorf("expected BAML int, got %T", value.value.Value)
}

func (value Value) BigInt() (*big.Int, error) {
	unwrapped, err := value.unwrapUnionVariants()
	if err != nil {
		return nil, err
	}
	value = unwrapped
	if value.value == nil {
		return nil, fmt.Errorf("BAML value is uninitialized")
	}
	var encoded string
	switch item := value.value.Value.(type) {
	case *cffi.BamlOutboundValue_BigintValue:
		encoded = item.BigintValue
	case *cffi.BamlOutboundValue_LiteralValue:
		if item.LiteralValue == nil {
			break
		}
		if literal, ok := item.LiteralValue.Literal.(*cffi.BamlLiteralValue_BigintValue); ok {
			encoded = literal.BigintValue
		}
	}
	if encoded == "" {
		return nil, fmt.Errorf("expected BAML bigint, got %T", value.value.Value)
	}
	decoded, ok := new(big.Int).SetString(encoded, 16)
	if !ok {
		return nil, fmt.Errorf("BAML returned invalid bigint %q", encoded)
	}
	return decoded, nil
}

func (value Value) Float64() (float64, error) {
	unwrapped, err := value.unwrapUnionVariants()
	if err != nil {
		return 0, err
	}
	value = unwrapped
	if value.value == nil {
		return 0, fmt.Errorf("BAML value is uninitialized")
	}
	switch item := value.value.Value.(type) {
	case *cffi.BamlOutboundValue_FloatValue:
		return item.FloatValue, nil
	case *cffi.BamlOutboundValue_LiteralValue:
		if item.LiteralValue == nil {
			break
		}
		literal, ok := item.LiteralValue.Literal.(*cffi.BamlLiteralValue_FloatValue)
		if !ok {
			break
		}
		decoded, err := strconv.ParseFloat(literal.FloatValue, 64)
		if err != nil {
			return 0, fmt.Errorf("BAML returned invalid float literal %q: %w", literal.FloatValue, err)
		}
		return decoded, nil
	}
	return 0, fmt.Errorf("expected BAML float, got %T", value.value.Value)
}

func (value Value) Bool() (bool, error) {
	unwrapped, err := value.unwrapUnionVariants()
	if err != nil {
		return false, err
	}
	value = unwrapped
	if value.value == nil {
		return false, fmt.Errorf("BAML value is uninitialized")
	}
	switch item := value.value.Value.(type) {
	case *cffi.BamlOutboundValue_BoolValue:
		return item.BoolValue, nil
	case *cffi.BamlOutboundValue_LiteralValue:
		if item.LiteralValue == nil {
			break
		}
		if literal, ok := item.LiteralValue.Literal.(*cffi.BamlLiteralValue_BoolValue); ok {
			return literal.BoolValue, nil
		}
	}
	return false, fmt.Errorf("expected BAML bool, got %T", value.value.Value)
}

func (value Value) Null() (Null, error) {
	unwrapped, err := value.unwrapUnionVariants()
	if err != nil {
		return Null{}, err
	}
	value = unwrapped
	if value.value == nil {
		return Null{}, fmt.Errorf("BAML value is uninitialized")
	}
	if value.value.Value == nil {
		return Null{}, nil
	}
	if _, ok := value.value.Value.(*cffi.BamlOutboundValue_NullValue); ok {
		return Null{}, nil
	}
	return Null{}, fmt.Errorf("expected BAML null, got %T", value.value.Value)
}

func (value Value) Uint8Array() ([]byte, error) {
	unwrapped, err := value.unwrapUnionVariants()
	if err != nil {
		return nil, err
	}
	value = unwrapped
	if value.value == nil {
		return nil, fmt.Errorf("BAML value is uninitialized")
	}
	item, ok := value.value.Value.(*cffi.BamlOutboundValue_Uint8ArrayValue)
	if !ok {
		return nil, fmt.Errorf("expected BAML uint8array, got %T", value.value.Value)
	}
	return append([]byte(nil), item.Uint8ArrayValue...), nil
}

// Type decodes a reflected BAML type value. The returned descriptor is owned
// entirely by Go and may be compared or sent back through another BAML call.
func (value Value) Type() (BAMLType, error) {
	unwrapped, err := value.unwrapUnionVariants()
	if err != nil {
		return BAMLType{}, err
	}
	value = unwrapped
	if value.value == nil {
		return BAMLType{}, fmt.Errorf("BAML value is uninitialized")
	}
	switch item := value.value.Value.(type) {
	case *cffi.BamlOutboundValue_TyDefValue:
		if err := validateBAMLTypeDefinition(item.TyDefValue); err != nil {
			return BAMLType{}, fmt.Errorf("invalid BAML type definition: %w", err)
		}
		return BAMLType{definition: proto.Clone(item.TyDefValue).(*cffi.BamlTyDef)}, nil
	case *cffi.BamlOutboundValue_TyValue:
		if err := validateBAMLType(item.TyValue, 0); err != nil {
			return BAMLType{}, fmt.Errorf("invalid BAML type value: %w", err)
		}
		return BAMLType{value: proto.Clone(item.TyValue).(*cffi.BamlTy)}, nil
	default:
		return BAMLType{}, fmt.Errorf("expected BAML type value, got %T", value.value.Value)
	}
}

// unwrapUnionVariants removes the ABI's descriptive union envelopes before a
// generated concrete decoder reads the chosen value. This is required even
// for unions normalized by the compiler to a single host-language type (for
// example `int | int`), while preserving general-union metadata for the
// future generated union representation at the boundary above Value.
func (value Value) unwrapUnionVariants() (Value, error) {
	for depth := 0; depth < 64; depth++ {
		if value.value == nil {
			return Value{}, fmt.Errorf("BAML value is uninitialized")
		}
		item, ok := value.value.Value.(*cffi.BamlOutboundValue_UnionVariantValue)
		if !ok {
			return value, nil
		}
		_, payload, err := validateUnionVariant(item.UnionVariantValue)
		if err != nil {
			return Value{}, err
		}
		payload.owner = value.owner
		value = payload
	}
	return Value{}, fmt.Errorf("BAML union variant nesting exceeds 64 levels")
}

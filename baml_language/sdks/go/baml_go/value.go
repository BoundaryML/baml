package baml_go

import (
	"fmt"
	"math/big"
	"strconv"

	"github.com/boundaryml/baml-go/internal/cffi"
	"google.golang.org/protobuf/proto"
)

// BAMLType is a typed wire descriptor used to dispatch closed union results.
// Its protobuf representation remains private so generated code cannot depend
// on wire implementation details or parse diagnostic type strings.
type BAMLType struct {
	value *cffi.BamlTy
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

func ClassBAMLType(name string) BAMLType {
	return BAMLType{value: &cffi.BamlTy{Ty: &cffi.BamlTy_ClassTy{ClassTy: &cffi.BamlTyClass{Name: name}}}}
}

func EnumBAMLType(name string) BAMLType {
	return BAMLType{value: &cffi.BamlTy{Ty: &cffi.BamlTy_Enum{Enum: &cffi.BamlTyEnum{Name: name}}}}
}

// TypeAliasBAMLType describes a named BAML type alias. Generated union
// codecs use this when an alias is itself a selected arm; the native runtime
// remains authoritative for resolving and validating the alias body.
func TypeAliasBAMLType(name string) BAMLType {
	return BAMLType{value: &cffi.BamlTy{Ty: &cffi.BamlTy_TypeAlias{TypeAlias: &cffi.BamlTyTypeAlias{Name: name}}}}
}

func ListBAMLType(item BAMLType) BAMLType {
	return BAMLType{value: &cffi.BamlTy{Ty: &cffi.BamlTy_List{List: &cffi.BamlTyList{Item: item.value}}}}
}

func MapBAMLType(key, value BAMLType) BAMLType {
	return BAMLType{value: &cffi.BamlTy{Ty: &cffi.BamlTy_Map{Map: &cffi.BamlTyMap{Key: key.value, Value: value.value}}}}
}

func OptionalBAMLType(inner BAMLType) BAMLType {
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

func (value BAMLType) Equal(other BAMLType) bool {
	return equalBAMLType(value.value, other.value, true)
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
		leftOptional, leftIsOptional := left.Ty.(*cffi.BamlTy_Optional)
		rightOptional, rightIsOptional := right.Ty.(*cffi.BamlTy_Optional)
		switch {
		case leftIsOptional && rightIsOptional && leftOptional.Optional != nil && rightOptional.Optional != nil:
			return equalBAMLType(leftOptional.Optional.Inner, rightOptional.Optional.Inner, false)
		case leftIsOptional && leftOptional.Optional != nil:
			return equalBAMLType(leftOptional.Optional.Inner, right, false)
		case rightIsOptional && rightOptional.Optional != nil:
			return equalBAMLType(left, rightOptional.Optional.Inner, false)
		}
	}
	switch leftValue := left.Ty.(type) {
	case *cffi.BamlTy_List:
		rightValue, ok := right.Ty.(*cffi.BamlTy_List)
		return ok && leftValue.List != nil && rightValue.List != nil && equalBAMLType(leftValue.List.Item, rightValue.List.Item, false)
	case *cffi.BamlTy_Map:
		rightValue, ok := right.Ty.(*cffi.BamlTy_Map)
		return ok && leftValue.Map != nil && rightValue.Map != nil &&
			equalBAMLType(leftValue.Map.Key, rightValue.Map.Key, false) && equalBAMLType(leftValue.Map.Value, rightValue.Map.Value, false)
	case *cffi.BamlTy_Union:
		rightValue, ok := right.Ty.(*cffi.BamlTy_Union)
		if !ok || leftValue.Union == nil || rightValue.Union == nil || len(leftValue.Union.Options) != len(rightValue.Union.Options) {
			return false
		}
		matched := make([]bool, len(rightValue.Union.Options))
		for _, leftOption := range leftValue.Union.Options {
			found := false
			for index, rightOption := range rightValue.Union.Options {
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
	default:
		// Optionality is semantically significant at every nested position:
		// list<int> must not compare equal to list<int?>. Selected union arms
		// arrive in canonical non-optional form, so no wrapper erasure is
		// needed here.
		return proto.Equal(left, right)
	}
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
// The Go bridge and native runtime require an exact product-version match, so
// self_type and selected_type are mandatory. selected_option_index remains
// optional solely for envelopes emitted during its protobuf rollout; when it
// is present, it must agree exactly with selected_type and self_type.
func validateUnionVariant(variant *cffi.BamlValueUnionVariant) (BAMLType, Value, error) {
	if variant == nil {
		return BAMLType{}, Value{}, fmt.Errorf("BAML union variant metadata is missing")
	}
	if variant.SelfType == nil || variant.SelfType.Ty == nil {
		return BAMLType{}, Value{}, fmt.Errorf("BAML union variant is missing self type metadata")
	}
	if variant.SelectedType == nil || variant.SelectedType.Ty == nil {
		return BAMLType{}, Value{}, fmt.Errorf("BAML union variant is missing selected type metadata")
	}
	if variant.Value == nil {
		return BAMLType{}, Value{}, fmt.Errorf("BAML union variant has an empty value")
	}

	options, err := unionTypeOptions(variant.SelfType)
	if err != nil {
		return BAMLType{}, Value{}, err
	}
	selectedIndex := -1
	for index, option := range options {
		if equalBAMLType(option, variant.SelectedType, false) {
			selectedIndex = index
			break
		}
	}
	if selectedIndex < 0 {
		return BAMLType{}, Value{}, fmt.Errorf("BAML union variant selected type is not a member of self type")
	}
	if variant.SelectedOptionIndex != nil {
		rawIndex := *variant.SelectedOptionIndex
		if uint64(rawIndex) >= uint64(len(options)) {
			return BAMLType{}, Value{}, fmt.Errorf("BAML union variant selected option index %d is outside self type with %d options", rawIndex, len(options))
		}
		index := int(rawIndex)
		if index != selectedIndex || !equalBAMLType(options[index], variant.SelectedType, false) {
			return BAMLType{}, Value{}, fmt.Errorf("BAML union variant selected option index %d disagrees with selected type at index %d", index, selectedIndex)
		}
	}
	return BAMLType{value: variant.SelectedType}, Value{value: variant.Value}, nil
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

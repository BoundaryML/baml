package baml_go

import (
	"fmt"
	"math/big"
	"strconv"

	"github.com/boundaryml/baml-go/internal/cffi"
)

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
		if item.UnionVariantValue == nil || item.UnionVariantValue.Value == nil {
			return Value{}, fmt.Errorf("BAML union variant has an empty value")
		}
		value = Value{value: item.UnionVariantValue.Value}
	}
	return Value{}, fmt.Errorf("BAML union variant nesting exceeds 64 levels")
}

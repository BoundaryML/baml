package baml_go

import "math/big"

// Optional encodes nil as BAML null and a non-nil pointer with encode.
func Optional[T any](value *T, encode func(T) Input) Input {
	if value == nil {
		return NullInput(Null{})
	}
	return encode(*value)
}

func OptionalEncoder[T any](encode func(T) Input) func(*T) Input {
	return func(value *T) Input { return Optional(value, encode) }
}

// OptionalBigInt is specialized because both required and optional bigint use
// *big.Int in Go. A nil required bigint is invalid; a nil optional bigint is
// BAML null.
func OptionalBigInt(value *big.Int) Input {
	if value == nil {
		return NullInput(Null{})
	}
	return BigInt(value)
}

// DecodeOptional returns nil for BAML null and otherwise decodes a value.
func DecodeOptional[T any](value Value, decode func(Value) (T, error)) (*T, error) {
	isNull, err := value.isNull()
	if err != nil || isNull {
		return nil, err
	}
	decoded, err := decode(value)
	if err != nil {
		return nil, err
	}
	return &decoded, nil
}

func OptionalDecoder[T any](decode func(Value) (T, error)) func(Value) (*T, error) {
	return func(value Value) (*T, error) { return DecodeOptional(value, decode) }
}

// DecodeOptionalBigInt preserves bigint's single-pointer Go representation.
func DecodeOptionalBigInt(value Value) (*big.Int, error) {
	isNull, err := value.isNull()
	if err != nil || isNull {
		return nil, err
	}
	return value.BigInt()
}

// DecodeField applies a generated decoder to one required class field.
func DecodeField[T any](class ClassValue, name string, decode func(Value) (T, error)) (T, error) {
	field, err := class.Field(name)
	if err != nil {
		var zero T
		return zero, err
	}
	decoded, err := decode(field)
	return decoded, classFieldError(class.name, name, err)
}

// DecodeOptionalField decodes one nullable class field. The field itself is
// still required on the wire; only its value may be null.
func DecodeOptionalField[T any](class ClassValue, name string, decode func(Value) (T, error)) (*T, error) {
	field, err := class.Field(name)
	if err != nil {
		return nil, err
	}
	decoded, err := DecodeOptional(field, decode)
	return decoded, classFieldError(class.name, name, err)
}

func DecodeOptionalBigIntField(class ClassValue, name string) (*big.Int, error) {
	field, err := class.Field(name)
	if err != nil {
		return nil, err
	}
	decoded, err := DecodeOptionalBigInt(field)
	return decoded, classFieldError(class.name, name, err)
}

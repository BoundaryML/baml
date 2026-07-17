package baml_go

import (
	"fmt"
	"math"
	"reflect"
	"strconv"

	"github.com/boundaryml/baml-go/internal/cffi"
)

const maxJSONDecodeDepth = 256

// JSON converts an ordinary Go JSON value into the generic ABI tree. Unlike
// Any, it deliberately rejects every BAML-only extension (bigints, bytes,
// classes, enums, unions, media, and non-finite floats) before native code is
// entered. The native runtime still performs the final assignability check.
func JSON(value any) Input {
	return encodeJSON(reflect.ValueOf(value), make(map[visit]bool), "$", 0)
}

func encodeJSON(value reflect.Value, active map[visit]bool, path string, depth int) Input {
	if depth > maxJSONDecodeDepth {
		return InvalidInput(fmt.Sprintf("encode baml.json.json at %s: nesting exceeds %d levels", path, maxJSONDecodeDepth))
	}
	if !value.IsValid() {
		return NullInput(Null{})
	}
	if value.Kind() == reflect.Interface {
		if value.IsNil() {
			return NullInput(Null{})
		}
		return encodeJSON(value.Elem(), active, path, depth+1)
	}
	if (value.Kind() == reflect.Pointer || value.Kind() == reflect.Map || value.Kind() == reflect.Slice) && value.IsNil() {
		return NullInput(Null{})
	}
	if value.CanInterface() {
		if _, ok := value.Interface().(InputMarshaler); ok {
			return InvalidInput(fmt.Sprintf("encode baml.json.json at %s: generated BAML values are not JSON", path))
		}
		if _, ok := value.Interface().(DynamicClass); ok {
			return InvalidInput(fmt.Sprintf("encode baml.json.json at %s: BAML classes are not JSON", path))
		}
		if _, ok := value.Interface().(DynamicEnum); ok {
			return InvalidInput(fmt.Sprintf("encode baml.json.json at %s: BAML enums are not JSON", path))
		}
	}

	switch value.Kind() {
	case reflect.Pointer:
		key := visit{typ: value.Type(), ptr: value.Pointer()}
		if active[key] {
			return InvalidInput(fmt.Sprintf("encode baml.json.json at %s: cyclic Go pointer cannot cross the BAML ABI", path))
		}
		active[key] = true
		defer delete(active, key)
		return encodeJSON(value.Elem(), active, path, depth+1)
	case reflect.Bool:
		return Bool(value.Bool())
	case reflect.String:
		return String(value.String())
	case reflect.Int, reflect.Int8, reflect.Int16, reflect.Int32, reflect.Int64:
		return Int64(value.Int())
	case reflect.Uint, reflect.Uint8, reflect.Uint16, reflect.Uint32, reflect.Uint64, reflect.Uintptr:
		integer := value.Uint()
		if integer > math.MaxInt64 {
			return InvalidInput(fmt.Sprintf("encode baml.json.json at %s: unsigned integer %d overflows BAML int", path, integer))
		}
		return Int64(int64(integer))
	case reflect.Float32, reflect.Float64:
		float := value.Float()
		if math.IsNaN(float) || math.IsInf(float, 0) {
			return InvalidInput(fmt.Sprintf("encode baml.json.json at %s: non-finite float %v is not JSON", path, float))
		}
		return Float64(float)
	case reflect.Slice:
		if value.Type().Elem().Kind() == reflect.Uint8 {
			return InvalidInput(fmt.Sprintf("encode baml.json.json at %s: byte slices are not JSON", path))
		}
		return encodeJSONList(value, active, path, depth)
	case reflect.Array:
		return encodeJSONList(value, active, path, depth)
	case reflect.Map:
		if value.Type().Key().Kind() != reflect.String {
			return InvalidInput(fmt.Sprintf("encode baml.json.json at %s: JSON objects require string keys, got %s", path, value.Type().Key()))
		}
		key := visit{typ: value.Type(), ptr: value.Pointer()}
		if active[key] {
			return InvalidInput(fmt.Sprintf("encode baml.json.json at %s: cyclic Go map cannot cross the BAML ABI", path))
		}
		active[key] = true
		defer delete(active, key)
		entries := make(map[string]Input, value.Len())
		iterator := value.MapRange()
		for iterator.Next() {
			entryKey := iterator.Key().String()
			encoded := encodeJSON(iterator.Value(), active, fmt.Sprintf("%s[%q]", path, entryKey), depth+1)
			if encoded.err != nil {
				return encoded
			}
			entries[entryKey] = encoded
		}
		return anyMap(entries, nil)
	default:
		return InvalidInput(fmt.Sprintf("encode baml.json.json at %s: unsupported Go value of type %s", path, value.Type()))
	}
}

func encodeJSONList(value reflect.Value, active map[visit]bool, path string, depth int) Input {
	if value.Kind() == reflect.Slice && value.Len() > 0 {
		key := visit{typ: value.Type(), ptr: value.Pointer()}
		if active[key] {
			return InvalidInput(fmt.Sprintf("encode baml.json.json at %s: cyclic Go slice cannot cross the BAML ABI", path))
		}
		active[key] = true
		defer delete(active, key)
	}
	items := make([]Input, value.Len())
	for index := range items {
		items[index] = encodeJSON(value.Index(index), active, fmt.Sprintf("%s[%d]", path, index), depth+1)
		if items[index].err != nil {
			return items[index]
		}
	}
	return anyList(items, nil)
}

// JSON decodes the exact recursive value algebra represented by
// baml.json.json into ordinary Go values: nil, bool, int64, float64, string,
// []any, and map[string]any. It treats the outbound ABI as untrusted and
// rejects values outside that algebra rather than silently widening them.
func (value Value) JSON() (any, error) {
	return decodeJSON(value, "$", 0)
}

func decodeJSON(value Value, path string, depth int) (any, error) {
	if depth > maxJSONDecodeDepth {
		return nil, fmt.Errorf("decode baml.json.json at %s: nesting exceeds %d levels", path, maxJSONDecodeDepth)
	}
	unwrapped, err := value.unwrapJSONUnionVariants(path)
	if err != nil {
		return nil, fmt.Errorf("decode baml.json.json at %s: %w", path, err)
	}
	value = unwrapped
	if value.value == nil {
		return nil, fmt.Errorf("decode baml.json.json at %s: BAML value is uninitialized", path)
	}

	switch item := value.value.Value.(type) {
	case nil:
		// bridge_ctypes encodes BAML null as an absent protobuf oneof.
		return nil, nil
	case *cffi.BamlOutboundValue_NullValue:
		// Also accept the explicit empty-message arm for compatibility with
		// producers that materialize BamlValueNull.
		if item.NullValue == nil {
			return nil, fmt.Errorf("decode baml.json.json at %s: null payload is empty", path)
		}
		return nil, nil
	case *cffi.BamlOutboundValue_BoolValue:
		return item.BoolValue, nil
	case *cffi.BamlOutboundValue_IntValue:
		return item.IntValue, nil
	case *cffi.BamlOutboundValue_FloatValue:
		if math.IsNaN(item.FloatValue) || math.IsInf(item.FloatValue, 0) {
			return nil, fmt.Errorf("decode baml.json.json at %s: non-finite float %v is not JSON", path, item.FloatValue)
		}
		return item.FloatValue, nil
	case *cffi.BamlOutboundValue_StringValue:
		return item.StringValue, nil
	case *cffi.BamlOutboundValue_LiteralValue:
		return decodeJSONLiteral(item.LiteralValue, path)
	case *cffi.BamlOutboundValue_ListValue:
		if item.ListValue == nil {
			return nil, fmt.Errorf("decode baml.json.json at %s: list payload is empty", path)
		}
		decoded := make([]any, len(item.ListValue.Items))
		for index, encoded := range item.ListValue.Items {
			if encoded == nil {
				return nil, fmt.Errorf("decode baml.json.json at %s[%d]: value is empty", path, index)
			}
			decoded[index], err = decodeJSON(Value{value: encoded, owner: value.owner}, fmt.Sprintf("%s[%d]", path, index), depth+1)
			if err != nil {
				return nil, err
			}
		}
		return decoded, nil
	case *cffi.BamlOutboundValue_MapValue:
		if item.MapValue == nil {
			return nil, fmt.Errorf("decode baml.json.json at %s: map payload is empty", path)
		}
		decoded := make(map[string]any, len(item.MapValue.Entries))
		for index, entry := range item.MapValue.Entries {
			if entry == nil {
				return nil, fmt.Errorf("decode baml.json.json at %s: map entry %d is empty", path, index)
			}
			if entry.Value == nil {
				return nil, fmt.Errorf("decode baml.json.json at %s[%q]: value is empty", path, entry.Key)
			}
			if _, duplicate := decoded[entry.Key]; duplicate {
				return nil, fmt.Errorf("decode baml.json.json at %s: duplicate map key %q", path, entry.Key)
			}
			decoded[entry.Key], err = decodeJSON(Value{value: entry.Value, owner: value.owner}, fmt.Sprintf("%s[%q]", path, entry.Key), depth+1)
			if err != nil {
				return nil, err
			}
		}
		return decoded, nil
	default:
		return nil, fmt.Errorf("decode baml.json.json at %s: non-JSON BAML wire value %T", path, value.value.Value)
	}
}

// unwrapJSONUnionVariants applies the ordinary union-envelope integrity
// checks and additionally proves that each selected arm is part of the JSON
// algebra and agrees with the payload's outer wire shape. The generic union
// unwrapping helper cannot perform this check because it has no declared
// destination type.
func (value Value) unwrapJSONUnionVariants(path string) (Value, error) {
	for depth := 0; depth < 64; depth++ {
		if value.value == nil {
			return Value{}, fmt.Errorf("BAML value is uninitialized")
		}
		envelope, ok := value.value.Value.(*cffi.BamlOutboundValue_UnionVariantValue)
		if !ok {
			return value, nil
		}
		selected, payload, err := validateUnionVariant(envelope.UnionVariantValue)
		if err != nil {
			return Value{}, err
		}
		if err := validateJSONSelectedPayload(selected.value, payload.value, path); err != nil {
			return Value{}, err
		}
		payload.owner = value.owner
		value = payload
	}
	return Value{}, fmt.Errorf("BAML union variant nesting exceeds 64 levels")
}

func validateJSONSelectedPayload(selected *cffi.BamlTy, payload *cffi.BamlOutboundValue, path string) error {
	if !isJSONBAMLType(selected, 0) {
		return fmt.Errorf("decode baml.json.json at %s: selected union arm is not JSON", path)
	}
	if payload == nil {
		return fmt.Errorf("decode baml.json.json at %s: selected union payload is empty", path)
	}
	if selected == nil || selected.Ty == nil {
		return fmt.Errorf("decode baml.json.json at %s: selected union type is empty", path)
	}

	// The canonical recursive alias admits every JSON wire shape. The normal
	// decoder below still validates the complete payload recursively.
	if alias, ok := selected.Ty.(*cffi.BamlTy_TypeAlias); ok {
		if alias.TypeAlias != nil && alias.TypeAlias.Name == "baml.json.json" {
			return nil
		}
	}

	switch typed := selected.Ty.(type) {
	case *cffi.BamlTy_Optional:
		if isJSONNullWire(payload) {
			return nil
		}
		if typed.Optional == nil {
			return fmt.Errorf("decode baml.json.json at %s: selected optional JSON type is empty", path)
		}
		return validateJSONSelectedPayload(typed.Optional.Inner, payload, path)
	case *cffi.BamlTy_Primitive:
		if typed.Primitive == nil || !jsonPrimitiveMatchesWire(typed.Primitive.Kind, payload) {
			return fmt.Errorf("decode baml.json.json at %s: selected JSON primitive disagrees with payload %T", path, payload.Value)
		}
	case *cffi.BamlTy_Literal:
		if typed.Literal == nil || !jsonLiteralTypeMatchesWire(typed.Literal, payload) {
			return fmt.Errorf("decode baml.json.json at %s: selected JSON literal disagrees with payload %T", path, payload.Value)
		}
	case *cffi.BamlTy_List:
		if typed.List == nil {
			return fmt.Errorf("decode baml.json.json at %s: selected JSON list type is empty", path)
		}
		list, ok := payload.Value.(*cffi.BamlOutboundValue_ListValue)
		if !ok {
			return fmt.Errorf("decode baml.json.json at %s: selected JSON list disagrees with payload %T", path, payload.Value)
		}
		if list.ListValue == nil {
			return fmt.Errorf("decode baml.json.json at %s: selected JSON list payload is empty", path)
		}
		for index, item := range list.ListValue.Items {
			if err := validateJSONSelectedPayload(typed.List.Item, item, fmt.Sprintf("%s[%d]", path, index)); err != nil {
				return err
			}
		}
	case *cffi.BamlTy_Map:
		if typed.Map == nil {
			return fmt.Errorf("decode baml.json.json at %s: selected JSON map type is empty", path)
		}
		object, ok := payload.Value.(*cffi.BamlOutboundValue_MapValue)
		if !ok {
			return fmt.Errorf("decode baml.json.json at %s: selected JSON map disagrees with payload %T", path, payload.Value)
		}
		if object.MapValue == nil {
			return fmt.Errorf("decode baml.json.json at %s: selected JSON map payload is empty", path)
		}
		for _, entry := range object.MapValue.Entries {
			if entry == nil {
				return fmt.Errorf("decode baml.json.json at %s: selected JSON map entry is empty", path)
			}
			if err := validateJSONSelectedPayload(typed.Map.Value, entry.Value, fmt.Sprintf("%s[%q]", path, entry.Key)); err != nil {
				return err
			}
		}
	case *cffi.BamlTy_Union:
		if typed.Union == nil || len(typed.Union.Options) == 0 {
			return fmt.Errorf("decode baml.json.json at %s: selected JSON union type is empty", path)
		}
		for _, option := range typed.Union.Options {
			if validateJSONSelectedPayload(option, payload, path) == nil {
				return nil
			}
		}
		return fmt.Errorf("decode baml.json.json at %s: selected JSON union disagrees with payload %T", path, payload.Value)
	default:
		return fmt.Errorf("decode baml.json.json at %s: selected union arm is not JSON", path)
	}
	return nil
}

func isJSONBAMLType(value *cffi.BamlTy, depth int) bool {
	if value == nil || value.Ty == nil || depth > 64 {
		return false
	}
	switch typed := value.Ty.(type) {
	case *cffi.BamlTy_TypeAlias:
		return typed.TypeAlias != nil && typed.TypeAlias.Name == "baml.json.json"
	case *cffi.BamlTy_Primitive:
		if typed.Primitive == nil {
			return false
		}
		switch typed.Primitive.Kind {
		case cffi.BamlTyPrimitiveKind_BAML_TY_PRIMITIVE_NULL,
			cffi.BamlTyPrimitiveKind_BAML_TY_PRIMITIVE_BOOL,
			cffi.BamlTyPrimitiveKind_BAML_TY_PRIMITIVE_INT,
			cffi.BamlTyPrimitiveKind_BAML_TY_PRIMITIVE_FLOAT,
			cffi.BamlTyPrimitiveKind_BAML_TY_PRIMITIVE_STRING:
			return true
		default:
			return false
		}
	case *cffi.BamlTy_Literal:
		return typed.Literal != nil && isJSONLiteralType(typed.Literal)
	case *cffi.BamlTy_List:
		return typed.List != nil && isJSONBAMLType(typed.List.Item, depth+1)
	case *cffi.BamlTy_Map:
		return typed.Map != nil && isJSONStringType(typed.Map.Key) && isJSONBAMLType(typed.Map.Value, depth+1)
	case *cffi.BamlTy_Optional:
		return typed.Optional != nil && isJSONBAMLType(typed.Optional.Inner, depth+1)
	case *cffi.BamlTy_Union:
		if typed.Union == nil || len(typed.Union.Options) == 0 {
			return false
		}
		for _, option := range typed.Union.Options {
			if !isJSONBAMLType(option, depth+1) {
				return false
			}
		}
		return true
	default:
		return false
	}
}

func isJSONStringType(value *cffi.BamlTy) bool {
	primitive, ok := value.GetTy().(*cffi.BamlTy_Primitive)
	return ok && primitive.Primitive != nil && primitive.Primitive.Kind == cffi.BamlTyPrimitiveKind_BAML_TY_PRIMITIVE_STRING
}

func isJSONLiteralType(value *cffi.BamlTyLiteral) bool {
	switch literal := value.Literal.(type) {
	case *cffi.BamlTyLiteral_StringValue, *cffi.BamlTyLiteral_IntValue, *cffi.BamlTyLiteral_BoolValue:
		return true
	case *cffi.BamlTyLiteral_FloatValue:
		parsed, err := strconv.ParseFloat(literal.FloatValue, 64)
		return err == nil && !math.IsNaN(parsed) && !math.IsInf(parsed, 0)
	default:
		return false
	}
}

func isJSONNullWire(value *cffi.BamlOutboundValue) bool {
	if value == nil || value.Value == nil {
		return value != nil
	}
	nullValue, ok := value.Value.(*cffi.BamlOutboundValue_NullValue)
	return ok && nullValue.NullValue != nil
}

func jsonPrimitiveMatchesWire(kind cffi.BamlTyPrimitiveKind, value *cffi.BamlOutboundValue) bool {
	switch kind {
	case cffi.BamlTyPrimitiveKind_BAML_TY_PRIMITIVE_NULL:
		return isJSONNullWire(value)
	case cffi.BamlTyPrimitiveKind_BAML_TY_PRIMITIVE_BOOL:
		_, ok := value.Value.(*cffi.BamlOutboundValue_BoolValue)
		return ok
	case cffi.BamlTyPrimitiveKind_BAML_TY_PRIMITIVE_INT:
		_, ok := value.Value.(*cffi.BamlOutboundValue_IntValue)
		return ok
	case cffi.BamlTyPrimitiveKind_BAML_TY_PRIMITIVE_FLOAT:
		_, ok := value.Value.(*cffi.BamlOutboundValue_FloatValue)
		return ok
	case cffi.BamlTyPrimitiveKind_BAML_TY_PRIMITIVE_STRING:
		_, ok := value.Value.(*cffi.BamlOutboundValue_StringValue)
		return ok
	default:
		return false
	}
}

func jsonLiteralTypeMatchesWire(selected *cffi.BamlTyLiteral, value *cffi.BamlOutboundValue) bool {
	wire, ok := value.Value.(*cffi.BamlOutboundValue_LiteralValue)
	if !ok || wire.LiteralValue == nil || selected == nil {
		return false
	}
	switch expected := selected.Literal.(type) {
	case *cffi.BamlTyLiteral_StringValue:
		actual, ok := wire.LiteralValue.Literal.(*cffi.BamlLiteralValue_StringValue)
		return ok && actual.StringValue == expected.StringValue
	case *cffi.BamlTyLiteral_IntValue:
		actual, ok := wire.LiteralValue.Literal.(*cffi.BamlLiteralValue_IntValue)
		return ok && actual.IntValue == expected.IntValue
	case *cffi.BamlTyLiteral_FloatValue:
		actual, ok := wire.LiteralValue.Literal.(*cffi.BamlLiteralValue_FloatValue)
		return ok && actual.FloatValue == expected.FloatValue
	case *cffi.BamlTyLiteral_BoolValue:
		actual, ok := wire.LiteralValue.Literal.(*cffi.BamlLiteralValue_BoolValue)
		return ok && actual.BoolValue == expected.BoolValue
	default:
		return false
	}
}

func decodeJSONLiteral(value *cffi.BamlLiteralValue, path string) (any, error) {
	if value == nil {
		return nil, fmt.Errorf("decode baml.json.json at %s: literal payload is empty", path)
	}
	switch literal := value.Literal.(type) {
	case *cffi.BamlLiteralValue_StringValue:
		return literal.StringValue, nil
	case *cffi.BamlLiteralValue_IntValue:
		return literal.IntValue, nil
	case *cffi.BamlLiteralValue_FloatValue:
		decoded, err := strconv.ParseFloat(literal.FloatValue, 64)
		if err != nil || math.IsNaN(decoded) || math.IsInf(decoded, 0) {
			return nil, fmt.Errorf("decode baml.json.json at %s: invalid JSON float literal %q", path, literal.FloatValue)
		}
		return decoded, nil
	case *cffi.BamlLiteralValue_BoolValue:
		return literal.BoolValue, nil
	default:
		return nil, fmt.Errorf("decode baml.json.json at %s: non-JSON BAML literal %T", path, value.Literal)
	}
}

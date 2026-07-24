package baml_go

import (
	"fmt"
	"reflect"
	"sort"

	"github.com/boundaryml/baml-go/internal/cffi"
)

// List encodes a present BAML list. A nil Go slice is a present empty list;
// nullable lists use Optional around ListEncoder to represent BAML null.
func List[T any](values []T, encode func(T) Input) Input {
	inputs := make([]Input, 0, len(values))
	for _, value := range values {
		inputs = append(inputs, encode(value))
	}
	return listInput(inputs, nil)
}

func listInput(inputs []Input, itemType *BAMLType) Input {
	prepare := func(transaction *inputTransaction) (*cffi.InboundValue, error) {
		if itemType != nil {
			if err := validateBAMLType(itemType.value, 0); err != nil {
				return nil, fmt.Errorf("list item type: %w", err)
			}
		}
		items := make([]*cffi.InboundValue, 0, len(inputs))
		for index, input := range inputs {
			encoded, err := input.encodeValue(transaction)
			if err != nil {
				return nil, fmt.Errorf("list item %d: %w", index, err)
			}
			items = append(items, encoded)
		}
		value := &cffi.InboundValue{
			Value: &cffi.InboundValue_ListValue{
				ListValue: &cffi.InboundListValue{Values: items},
			},
		}
		if itemType != nil {
			value.ValueType = ListBAMLType(*itemType).value
		}
		return value, nil
	}
	if inputsAreStatic(inputs) {
		value, err := prepare(nil)
		return Input{value: value, err: err}
	}
	return Input{deferred: &inputEncoder{encode: prepare}}
}

// ListEncoder adapts an element encoder into the shape used by nested and
// nullable generated codecs.
func ListEncoder[T any](encode func(T) Input) func([]T) Input {
	return func(values []T) Input {
		inputs := make([]Input, 0, len(values))
		for _, value := range values {
			inputs = append(inputs, encode(value))
		}
		var itemType *BAMLType
		if inferred, ok := reflectedBAMLType(reflect.TypeOf((*T)(nil)).Elem()); ok {
			itemType = &inferred
		}
		return listInput(inputs, itemType)
	}
}

// ListEncoderWithType is the generated-code form of ListEncoder. Generated
// APIs know the exact BAML item type even when Go reflection cannot distinguish
// two wire types with the same surface (notably bigint and bigint?).
func ListEncoderWithType[T any](itemType BAMLType, encode func(T) Input) func([]T) Input {
	return func(values []T) Input {
		inputs := make([]Input, 0, len(values))
		for _, value := range values {
			inputs = append(inputs, encode(value))
		}
		return listInput(inputs, &itemType)
	}
}

// Map encodes a present BAML map with string keys. Entries are sorted so the
// protobuf representation is deterministic. A nil Go map is a present empty
// map; nullable maps use Optional around MapEncoder.
func Map[T any](values map[string]T, encode func(T) Input) Input {
	keys := make([]string, 0, len(values))
	for key := range values {
		keys = append(keys, key)
	}
	sort.Strings(keys)

	inputs := make([]Input, 0, len(keys))
	for _, key := range keys {
		inputs = append(inputs, encode(values[key]))
	}
	return mapInput(keys, inputs, nil)
}

func mapInput(keys []string, inputs []Input, valueType *BAMLType) Input {
	prepare := func(transaction *inputTransaction) (*cffi.InboundValue, error) {
		if valueType != nil {
			if err := validateBAMLType(valueType.value, 0); err != nil {
				return nil, fmt.Errorf("map value type: %w", err)
			}
		}
		entries := make([]*cffi.InboundMapEntry, 0, len(keys))
		for index, key := range keys {
			encoded, err := inputs[index].encodeValue(transaction)
			if err != nil {
				return nil, fmt.Errorf("map entry %q: %w", key, err)
			}
			entries = append(entries, &cffi.InboundMapEntry{
				Key:   &cffi.InboundMapEntry_StringKey{StringKey: key},
				Value: encoded,
			})
		}
		value := &cffi.InboundValue{
			Value: &cffi.InboundValue_MapValue{
				MapValue: &cffi.InboundMapValue{Entries: entries},
			},
		}
		if valueType != nil {
			value.ValueType = MapBAMLType(PrimitiveBAMLType(StringType), *valueType).value
		}
		return value, nil
	}
	if inputsAreStatic(inputs) {
		value, err := prepare(nil)
		return Input{value: value, err: err}
	}
	return Input{deferred: &inputEncoder{encode: prepare}}
}

func inputsAreStatic(inputs []Input) bool {
	for _, input := range inputs {
		if input.deferred != nil {
			return false
		}
	}
	return true
}

// MapEncoder adapts a value encoder into the shape used by nested and
// nullable generated codecs.
func MapEncoder[T any](encode func(T) Input) func(map[string]T) Input {
	return func(values map[string]T) Input {
		keys := make([]string, 0, len(values))
		for key := range values {
			keys = append(keys, key)
		}
		sort.Strings(keys)
		inputs := make([]Input, 0, len(keys))
		for _, key := range keys {
			inputs = append(inputs, encode(values[key]))
		}
		var valueType *BAMLType
		if inferred, ok := reflectedBAMLType(reflect.TypeOf((*T)(nil)).Elem()); ok {
			valueType = &inferred
		}
		return mapInput(keys, inputs, valueType)
	}
}

// MapEncoderWithType is the generated-code form of MapEncoder. It carries the
// exact BAML value type independently of Go's reflected map element type.
func MapEncoderWithType[T any](valueType BAMLType, encode func(T) Input) func(map[string]T) Input {
	return func(values map[string]T) Input {
		keys := make([]string, 0, len(values))
		for key := range values {
			keys = append(keys, key)
		}
		sort.Strings(keys)
		inputs := make([]Input, 0, len(keys))
		for _, key := range keys {
			inputs = append(inputs, encode(values[key]))
		}
		return mapInput(keys, inputs, &valueType)
	}
}

func DecodeList[T any](value Value, decode func(Value) (T, error)) ([]T, error) {
	unwrapped, err := value.unwrapUnionVariants()
	if err != nil {
		return nil, err
	}
	value = unwrapped
	if value.value == nil {
		return nil, fmt.Errorf("BAML value is uninitialized")
	}
	item, ok := value.value.Value.(*cffi.BamlOutboundValue_ListValue)
	if !ok || item.ListValue == nil {
		return nil, fmt.Errorf("expected BAML list, got %T", value.value.Value)
	}

	decoded := make([]T, len(item.ListValue.Items))
	for index, encoded := range item.ListValue.Items {
		if encoded == nil {
			return nil, fmt.Errorf("BAML list item %d is empty", index)
		}
		item, err := decode(Value{value: encoded, owner: value.owner})
		if err != nil {
			return nil, fmt.Errorf("BAML list item %d: %w", index, err)
		}
		decoded[index] = item
	}
	return decoded, nil
}

func ListDecoder[T any](decode func(Value) (T, error)) func(Value) ([]T, error) {
	return func(value Value) ([]T, error) { return DecodeList(value, decode) }
}

func DecodeMap[T any](value Value, decode func(Value) (T, error)) (map[string]T, error) {
	unwrapped, err := value.unwrapUnionVariants()
	if err != nil {
		return nil, err
	}
	value = unwrapped
	if value.value == nil {
		return nil, fmt.Errorf("BAML value is uninitialized")
	}
	item, ok := value.value.Value.(*cffi.BamlOutboundValue_MapValue)
	if !ok || item.MapValue == nil {
		return nil, fmt.Errorf("expected BAML map, got %T", value.value.Value)
	}

	decoded := make(map[string]T, len(item.MapValue.Entries))
	for index, entry := range item.MapValue.Entries {
		if entry == nil {
			return nil, fmt.Errorf("BAML map entry %d is empty", index)
		}
		if entry.Value == nil {
			return nil, fmt.Errorf("BAML map entry %q has an empty value", entry.Key)
		}
		if _, duplicate := decoded[entry.Key]; duplicate {
			return nil, fmt.Errorf("BAML map returned duplicate key %q", entry.Key)
		}
		mapValue, err := decode(Value{value: entry.Value, owner: value.owner})
		if err != nil {
			return nil, fmt.Errorf("BAML map entry %q: %w", entry.Key, err)
		}
		decoded[entry.Key] = mapValue
	}
	return decoded, nil
}

func MapDecoder[T any](decode func(Value) (T, error)) func(Value) (map[string]T, error) {
	return func(value Value) (map[string]T, error) { return DecodeMap(value, decode) }
}

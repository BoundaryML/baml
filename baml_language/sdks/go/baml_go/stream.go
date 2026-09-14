package baml_go

import (
	"context"
	"fmt"

	"github.com/boundaryml/baml-go/internal/cffi"
)

// Stream is an opaque ai.stream.Stream capability. Next reports done=true for
// the terminal ai.stream.Done sentinel; Final returns the settled output.
type Stream[TPartial, TFinal any] struct {
	key           uint64
	owner         *resultOwner
	decodePartial func(Value) (TPartial, error)
	decodeFinal   func(Value) (TFinal, error)
}

// DecodeStream is the generated-code decoder for a flat Stream projection.
func DecodeStream[TPartial, TFinal any](
	decodePartial func(Value) (TPartial, error),
	decodeFinal func(Value) (TFinal, error),
) func(Value) (Stream[TPartial, TFinal], error) {
	return func(value Value) (Stream[TPartial, TFinal], error) {
		unwrapped, err := value.unwrapUnionVariants()
		if err != nil {
			return Stream[TPartial, TFinal]{}, err
		}
		handle := unwrapped.value.GetHandleValue()
		if handle == nil || handle.GetKey() == 0 || handle.GetHandleType() != cffi.BamlHandleType_ADT_TAGGED_HEAP_HANDLE {
			return Stream[TPartial, TFinal]{}, fmt.Errorf("expected BAML Stream handle, got %T", unwrapped.value.GetValue())
		}
		return Stream[TPartial, TFinal]{
			key:           handle.GetKey(),
			owner:         unwrapped.owner,
			decodePartial: decodePartial,
			decodeFinal:   decodeFinal,
		}, nil
	}
}

func (stream Stream[TPartial, TFinal]) BAMLInput() Input {
	if stream.key == 0 || stream.owner == nil {
		return InvalidInput("uninitialized BAML Stream")
	}
	return liveHandleInput(stream.key, cffi.BamlHandleType_ADT_TAGGED_HEAP_HANDLE, stream.owner)
}

// Next yields one partial. done is true only for ai.stream.Done.
func (stream Stream[TPartial, TFinal]) Next(ctx context.Context) (partial TPartial, done bool, err error) {
	value, err := Call(ctx, "ai.stream.Stream.next", map[string]Input{"self": stream.BAMLInput()})
	if err != nil {
		return partial, false, err
	}
	unwrapped, err := value.unwrapUnionVariants()
	if err != nil {
		return partial, false, err
	}
	if class := unwrapped.value.GetClassValue(); class != nil && class.GetName() == "ai.stream.Done" {
		return partial, true, nil
	}
	partial, err = stream.decodePartial(value)
	return partial, false, err
}

// Final returns the settled stream output.
func (stream Stream[TPartial, TFinal]) Final(ctx context.Context) (TFinal, error) {
	value, err := Call(ctx, "ai.stream.Stream.final", map[string]Input{"self": stream.BAMLInput()})
	if err != nil {
		var zero TFinal
		return zero, err
	}
	return stream.decodeFinal(value)
}

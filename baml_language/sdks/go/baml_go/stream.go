package baml_go

import (
	"context"
	"fmt"

	"github.com/boundaryml/baml-go/internal/cffi"
)

// Stream is an opaque ai.stream.Stream[T] capability. A partial and the settled
// value share the one type: a partial is T parsed from the text received so
// far. Next reports done=true for the terminal ai.stream.Done sentinel; Final
// returns the settled value.
type Stream[T any] struct {
	key    uint64
	owner  *resultOwner
	decode func(Value) (T, error)
}

// DecodeStream is the generated-code decoder for a flat Stream projection.
func DecodeStream[T any](decode func(Value) (T, error)) func(Value) (Stream[T], error) {
	return func(value Value) (Stream[T], error) {
		unwrapped, err := value.unwrapUnionVariants()
		if err != nil {
			return Stream[T]{}, err
		}
		handle := unwrapped.value.GetHandleValue()
		if handle == nil || handle.GetKey() == 0 || handle.GetHandleType() != cffi.BamlHandleType_ADT_TAGGED_HEAP_HANDLE {
			return Stream[T]{}, fmt.Errorf("expected BAML Stream handle, got %T", unwrapped.value.GetValue())
		}
		return Stream[T]{
			key:    handle.GetKey(),
			owner:  unwrapped.owner,
			decode: decode,
		}, nil
	}
}

func (stream Stream[T]) BAMLInput() Input {
	if stream.key == 0 || stream.owner == nil {
		return InvalidInput("uninitialized BAML Stream")
	}
	return liveHandleInput(stream.key, cffi.BamlHandleType_ADT_TAGGED_HEAP_HANDLE, stream.owner)
}

// Next yields one partial. done is true only for ai.stream.Done.
func (stream Stream[T]) Next(ctx context.Context) (partial T, done bool, err error) {
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
	partial, err = stream.decode(value)
	return partial, false, err
}

// Final returns the settled stream value.
func (stream Stream[T]) Final(ctx context.Context) (T, error) {
	value, err := Call(ctx, "ai.stream.Stream.final", map[string]Input{"self": stream.BAMLInput()})
	if err != nil {
		var zero T
		return zero, err
	}
	return stream.decode(value)
}

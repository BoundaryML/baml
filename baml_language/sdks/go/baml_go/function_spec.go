package baml_go

import (
	"context"
	"fmt"
	"runtime"

	"github.com/boundaryml/baml-go/internal/cffi"
)

// FunctionSpec is an opaque, bound ai.FunctionSpec capability. The type
// parameter retains the authored output type while the engine remains the sole
// owner of the recipe.
type FunctionSpec[TOut any] struct {
	key       uint64
	owner     *resultOwner
	decodeOut func(Value) (TOut, error)
}

// EventCallback is the Go callback surface for stream lifecycle events. Event
// is an open, evolving BAML union, so the callback receives the canonical Value
// instead of a lossy reflected Go shape.
type EventCallback func(Value)

// EventCallbackInput adapts an EventCallback to the ordinary host-callable ABI.
func EventCallbackInput(callback EventCallback) Input {
	if callback == nil {
		return NullInput(Null{})
	}
	return HostCallable(func(arguments HostCallArguments) (Input, error) {
		if arguments.RequiredCount() != 1 {
			return InvalidInput("invalid stream event callback arity"), HostCallableArityError(1, arguments.RequiredCount())
		}
		if arguments.OptionalCount() != 0 {
			return InvalidInput("unexpected stream event callback argument"), HostCallableOptionalCountError(0, arguments.OptionalCount())
		}
		callback(arguments.Required(0))
		return NullInput(Null{}), nil
	})
}

// DecodeFunctionSpec is the generated-code decoder for a Spec operation.
func DecodeFunctionSpec[TOut any](
	decodeOut func(Value) (TOut, error),
) func(Value) (FunctionSpec[TOut], error) {
	return func(value Value) (FunctionSpec[TOut], error) {
		unwrapped, err := value.unwrapUnionVariants()
		if err != nil {
			return FunctionSpec[TOut]{}, err
		}
		handle := unwrapped.value.GetHandleValue()
		if handle == nil || handle.GetKey() == 0 || handle.GetHandleType() != cffi.BamlHandleType_ADT_FUNCTION_SPEC {
			return FunctionSpec[TOut]{}, fmt.Errorf("expected BAML FunctionSpec handle, got %T", unwrapped.value.GetValue())
		}
		return FunctionSpec[TOut]{
			key:       handle.GetKey(),
			owner:     unwrapped.owner,
			decodeOut: decodeOut,
		}, nil
	}
}

// BAMLInput allows a FunctionSpec to be passed back to its owning runtime.
func (spec FunctionSpec[TOut]) BAMLInput() Input {
	if spec.key == 0 || spec.owner == nil {
		return InvalidInput("uninitialized BAML FunctionSpec")
	}
	return liveHandleInput(spec.key, cffi.BamlHandleType_ADT_FUNCTION_SPEC, spec.owner)
}

// Call executes the bound recipe and decodes its final output. Options may
// override the client or attach an event callback through the canonical spec
// method without synthesizing another function binding.
func (spec FunctionSpec[TOut]) Call(ctx context.Context, options ...CallOption) (TOut, error) {
	arguments := map[string]Input{"self": spec.BAMLInput()}
	ApplyCallOptions(arguments, nil, options...)
	value, err := Call(ctx, "ai.FunctionSpec.call", arguments)
	if err != nil {
		var zero TOut
		return zero, err
	}
	return spec.decodeOut(value)
}

// Parse parses an existing model reply against this spec's output type.
func (spec FunctionSpec[TOut]) Parse(ctx context.Context, json string) (TOut, error) {
	value, err := Call(ctx, "ai.FunctionSpec.parse", map[string]Input{
		"self": spec.BAMLInput(),
		"json": String(json),
	})
	if err != nil {
		var zero TOut
		return zero, err
	}
	return spec.decodeOut(value)
}

// Prompt renders the portable provider-neutral prompt for this recipe.
func (spec FunctionSpec[TOut]) Prompt(ctx context.Context) (Prompt, error) {
	value, err := Call(ctx, "ai.FunctionSpec.prompt", map[string]Input{"self": spec.BAMLInput()})
	if err != nil {
		return Prompt{}, err
	}
	return value.Prompt()
}

// BuildRequest builds the provider request as an opaque BAML value. Generated
// request models may decode this value explicitly when they expose that type.
func (spec FunctionSpec[TOut]) BuildRequest(ctx context.Context, options ...CallOption) (Value, error) {
	arguments := map[string]Input{"self": spec.BAMLInput()}
	ApplyCallOptions(arguments, nil, options...)
	return Call(ctx, "ai.FunctionSpec.build_request", arguments)
}

// Name returns the authored function identity carried by this spec.
func (spec FunctionSpec[TOut]) Name(ctx context.Context) (string, error) {
	value, err := Call(ctx, "ai.FunctionSpec.name", map[string]Input{"self": spec.BAMLInput()})
	if err != nil {
		return "", err
	}
	return value.String()
}

// Arguments returns the authored arguments bound into this spec.
func (spec FunctionSpec[TOut]) Arguments(ctx context.Context) (map[string]any, error) {
	value, err := Call(ctx, "ai.FunctionSpec.arguments", map[string]Input{"self": spec.BAMLInput()})
	if err != nil {
		return nil, err
	}
	decoded, err := decodeDynamicValue(value, "ai.FunctionSpec.arguments", 0)
	if err != nil {
		return nil, err
	}
	arguments, ok := decoded.(map[string]any)
	if !ok {
		return nil, fmt.Errorf("ai.FunctionSpec.arguments returned Go type %T", decoded)
	}
	return arguments, nil
}

// OutputType returns the realized final-output type carried by this spec.
func (spec FunctionSpec[TOut]) OutputType(ctx context.Context) (BAMLType, error) {
	value, err := Call(ctx, "ai.FunctionSpec.output_type", map[string]Input{"self": spec.BAMLInput()})
	if err != nil {
		return BAMLType{}, err
	}
	return value.Type()
}

// Tools returns the spec's toolbox as an opaque portable BAML value. A typed
// Toolbox facade can decode this later without changing the FunctionSpec API.
func (spec FunctionSpec[TOut]) Tools(ctx context.Context) (Value, error) {
	return Call(ctx, "ai.FunctionSpec.tools", map[string]Input{"self": spec.BAMLInput()})
}

// ClientID returns the identifier of the spec's bound default client.
func (spec FunctionSpec[TOut]) ClientID(ctx context.Context) (string, error) {
	value, err := Call(ctx, "ai.FunctionSpec.client_id", map[string]Input{"self": spec.BAMLInput()})
	if err != nil {
		return "", err
	}
	return value.String()
}

func liveHandleInput(key uint64, handleType cffi.BamlHandleType, owner *resultOwner) Input {
	return Input{deferred: &inputEncoder{encode: func(transaction *inputTransaction) (*cffi.InboundValue, error) {
		if owner != nil && owner.runtimeKey != 0 {
			if transaction.runtimeKey != 0 && transaction.runtimeKey != owner.runtimeKey {
				return nil, fmt.Errorf("BAML handles belong to different runtimes")
			}
			transaction.runtimeKey = owner.runtimeKey
		}
		cloned, err := cloneInboundHandle(key)
		runtime.KeepAlive(owner)
		if err != nil {
			return nil, fmt.Errorf("clone BAML capability handle for input: %w", err)
		}
		transaction.own(cloned)
		return &cffi.InboundValue{Value: &cffi.InboundValue_Handle{Handle: &cffi.BamlHandle{
			Key: cloned, HandleType: handleType,
		}}}, nil
	}}}
}

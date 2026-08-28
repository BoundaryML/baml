package baml_go

import (
	"context"
	"errors"
	"fmt"
	"runtime"

	"github.com/boundaryml/baml-go/internal/cffi"
)

// Function is an owned BAML closure returned to Go. It retains the captured
// BAML environment and may be invoked repeatedly.
type Function struct {
	key        uint64
	parameters []string
	owner      *resultOwner
}

// Function decodes a returned FUNCTION_REF and transfers its result ownership
// into an invokable Go value.
func (value Value) Function() (Function, error) {
	if value.value == nil {
		return Function{}, errors.New("decode BAML function: empty value")
	}
	handle := value.value.GetHandleValue()
	if handle == nil || handle.GetHandleType() != cffi.BamlHandleType_FUNCTION_REF || handle.GetKey() == 0 {
		return Function{}, fmt.Errorf("decode BAML function: expected FUNCTION_REF, received %T", value.value.GetValue())
	}
	functionType := handle.GetTy().GetFunction()
	if functionType == nil {
		return Function{}, errors.New("decode BAML function: handle is missing its function type")
	}
	parameters := make([]string, len(functionType.GetParams()))
	for index, parameter := range functionType.GetParams() {
		if parameter == nil || parameter.GetName() == "" {
			return Function{}, fmt.Errorf("decode BAML function: parameter %d has no name", index)
		}
		parameters[index] = parameter.GetName()
	}
	return Function{key: handle.GetKey(), parameters: parameters, owner: value.owner}, nil
}

// ParameterNames returns the BAML parameter names in invocation order.
func (function Function) ParameterNames() []string {
	return append([]string(nil), function.parameters...)
}

// Call invokes the closure through the native handle table.
func (function Function) Call(ctx context.Context, args map[string]Input) (Value, error) {
	return function.CallOperation(ctx, FunctionOperationDirect, args)
}

// CallOperation invokes a semantic projection of this first-class function.
func (function Function) CallOperation(ctx context.Context, operation FunctionOperation, args map[string]Input) (Value, error) {
	if function.key == 0 || function.owner == nil {
		return Value{}, errors.New("call BAML function: invalid or released function handle")
	}
	value, err := callHandleOperation(ctx, function.key, operation, args)
	runtime.KeepAlive(function.owner)
	return value, err
}

// CallPositional invokes the closure with required arguments in declaration
// order and supplied optional arguments by name.
func (function Function) CallPositional(
	ctx context.Context,
	required []Input,
	optional map[string]Input,
) (Value, error) {
	if len(required) > len(function.parameters) {
		return Value{}, fmt.Errorf(
			"call BAML function: received %d positional arguments for %d parameters",
			len(required),
			len(function.parameters),
		)
	}
	arguments := make(map[string]Input, len(required)+len(optional))
	for index, value := range required {
		arguments[function.parameters[index]] = value
	}
	for name, value := range optional {
		arguments[name] = value
	}
	return function.Call(ctx, arguments)
}

package baml

import (
	"fmt"

	"github.com/boundaryml/baml/engine/language_client_go/baml_go/raw_objects"
	"github.com/boundaryml/baml/engine/language_client_go/pkg/cffi"
)

type functionLog struct {
	*raw_objects.RawObject
}

func newFunctionLog(ptr int64) FunctionLog {
	return &functionLog{raw_objects.FromPointer(ptr)}
}

func (f *functionLog) ObjectType() cffi.CFFIObjectType {
	return cffi.CFFIObjectType_OBJECT_FUNCTION_LOG
}

func (f *functionLog) pointer() int64 {
	return f.RawObject.Pointer()
}

func (f *functionLog) Id() (string, error) {
	result, err := raw_objects.CallMethod(f, "id", nil)
	if err != nil {
		return "", fmt.Errorf("failed to get ID: %w", err)
	}

	id, ok := result.(string)
	if !ok {
		return "", fmt.Errorf("unexpected type for ID: %T", result)
	}

	return id, nil
}

func (f *functionLog) FunctionName() (string, error) {
	result, err := raw_objects.CallMethod(f, "function_name", nil)
	if err != nil {
		return "", fmt.Errorf("failed to get function name: %w", err)
	}

	name, ok := result.(string)
	if !ok {
		return "", fmt.Errorf("unexpected type for function name: %T", result)
	}

	return name, nil
}

func (f *functionLog) LogType() (string, error) {
	result, err := raw_objects.CallMethod(f, "log_type", nil)
	if err != nil {
		return "", fmt.Errorf("failed to get log type: %w", err)
	}

	logType, ok := result.(string)
	if !ok {
		return "", fmt.Errorf("unexpected type for log type: %T", result)
	}

	return logType, nil
}

func (f *functionLog) Timing() (Timing, error) {
	result, err := raw_objects.CallMethod(f, "timing", nil)
	if err != nil {
		return nil, fmt.Errorf("failed to get timing: %w", err)
	}

	timing, ok := result.(Timing)
	if !ok {
		return nil, fmt.Errorf("unexpected type for timing: %T", result)
	}

	return timing, nil
}

func (f *functionLog) Usage() (Usage, error) {
	result, err := raw_objects.CallMethod(f, "usage", nil)
	if err != nil {
		return nil, fmt.Errorf("failed to get usage: %w", err)
	}

	usage, ok := result.(Usage)
	if !ok {
		return nil, fmt.Errorf("unexpected type for usage: %T", result)
	}

	return usage, nil
}

func (f *functionLog) RawLLMResponse() (string, error) {
	result, err := raw_objects.CallMethod(f, "raw_llm_response", nil)
	if err != nil {
		return "", fmt.Errorf("failed to get raw LLM response: %w", err)
	}

	response, ok := result.(string)
	if !ok {
		return "", fmt.Errorf("unexpected type for raw LLM response: %T", result)
	}

	return response, nil
}

func (f *functionLog) CallsCount() (int, error) {
	result, err := raw_objects.CallMethod(f, "calls_count", nil)
	if err != nil {
		return 0, fmt.Errorf("failed to get calls count: %w", err)
	}

	count, ok := result.(int)
	if !ok {
		return 0, fmt.Errorf("unexpected type for calls count: %T", result)
	}

	return count, nil
}

func (f *functionLog) Calls() ([]LLMCall, error) {
	result, err := raw_objects.CallMethod(f, "calls", nil)
	if err != nil {
		return nil, fmt.Errorf("failed to get calls: %w", err)
	}

	result_cast := result.([]raw_objects.RawPointer)
	calls := make([]LLMCall, len(result_cast))

	for i, call := range result_cast {
		calls[i] = call.(LLMCall)
	}

	return calls, nil
}

func (f *functionLog) SelectedCall() (LLMCall, error) {
	result, err := raw_objects.CallMethod(f, "selected_call", nil)
	if err != nil {
		return nil, fmt.Errorf("failed to get selected call: %w", err)
	}

	call, ok := result.(LLMCall)
	if !ok {
		return nil, fmt.Errorf("unexpected type for selected call: %T", result)
	}

	return call, nil
}

func (f *functionLog) Tags() (map[string]any, error) {
	result, err := raw_objects.CallMethod(f, "tags", nil)
	if err != nil {
		return nil, fmt.Errorf("failed to get tags: %w", err)
	}

	tags, ok := result.(map[string]any)
	if !ok {
		return nil, fmt.Errorf("unexpected type for tags: %T", result)
	}

	return tags, nil
}

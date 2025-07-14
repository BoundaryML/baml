package raw_objects

import (
	"fmt"

	"github.com/boundaryml/baml/engine/language_client_go/pkg/cffi"
)

type functionLog struct {
	*rawObject
}

func newFunctionLog(ptr int64) FunctionLog {
	return &functionLog{&rawObject{ptr: ptr}}
}

func (f *functionLog) objectType() cffi.CFFIObjectType {
	return cffi.CFFIObjectType_OBJECT_FUNCTION_LOG
}

func (f *functionLog) pointer() int64 {
	return f.rawObject.pointer()
}

func (f *functionLog) ID() (string, error) {
	result, err := callMethod(f, "id", nil)
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
	result, err := callMethod(f, "function_name", nil)
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
	result, err := callMethod(f, "log_type", nil)
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
	result, err := callMethod(f, "timing", nil)
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
	result, err := callMethod(f, "usage", nil)
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
	result, err := callMethod(f, "raw_llm_response", nil)
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
	result, err := callMethod(f, "calls_count", nil)
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
	result, err := callMethod(f, "calls", nil)
	if err != nil {
		return nil, fmt.Errorf("failed to get calls: %w", err)
	}

	calls, ok := result.([]LLMCall)
	if !ok {
		return nil, fmt.Errorf("unexpected type for calls: %T", result)
	}

	return calls, nil
}

func (f *functionLog) SelectedCall() (LLMCall, error) {
	result, err := callMethod(f, "selected_call", nil)
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
	result, err := callMethod(f, "tags", nil)
	if err != nil {
		return nil, fmt.Errorf("failed to get tags: %w", err)
	}

	tags, ok := result.(map[string]any)
	if !ok {
		return nil, fmt.Errorf("unexpected type for tags: %T", result)
	}

	return tags, nil
}

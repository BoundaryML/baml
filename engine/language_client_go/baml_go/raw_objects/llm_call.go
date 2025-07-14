package raw_objects

import (
	"fmt"

	"github.com/boundaryml/baml/engine/language_client_go/pkg/cffi"
)

type llmCall struct {
	*rawObject
}

func newLLMCall(ptr int64) LLMCall {
	return &llmCall{&rawObject{ptr: ptr}}
}

func (l *llmCall) objectType() cffi.CFFIObjectType {
	return cffi.CFFIObjectType_OBJECT_LLM_CALL
}

func (l *llmCall) pointer() int64 {
	return l.rawObject.pointer()
}

func (l *llmCall) ClientName() (string, error) {
	result, err := callMethod(l, "client_name", nil)
	if err != nil {
		return "", fmt.Errorf("failed to get client name: %w", err)
	}

	name, ok := result.(string)
	if !ok {
		return "", fmt.Errorf("unexpected type for client name: %T", result)
	}

	return name, nil
}

func (l *llmCall) Provider() (string, error) {
	result, err := callMethod(l, "provider", nil)
	if err != nil {
		return "", fmt.Errorf("failed to get provider: %w", err)
	}

	provider, ok := result.(string)
	if !ok {
		return "", fmt.Errorf("unexpected type for provider: %T", result)
	}

	return provider, nil
}

func (l *llmCall) HttpRequest() (HttpRequest, error) {
	result, err := callMethod(l, "http_request", nil)
	if err != nil {
		return nil, fmt.Errorf("failed to get http request: %w", err)
	}

	request, ok := result.(HttpRequest)
	if !ok {
		return nil, fmt.Errorf("unexpected type for http request: %T", result)
	}

	return request, nil
}

func (l *llmCall) HttpResponse() (HttpResponse, error) {
	result, err := callMethod(l, "http_response", nil)
	if err != nil {
		return nil, fmt.Errorf("failed to get http response: %w", err)
	}

	if result == nil {
		return nil, nil
	}

	response, ok := result.(HttpResponse)
	if !ok {
		return nil, fmt.Errorf("unexpected type for http response: %T", result)
	}

	return response, nil
}

func (l *llmCall) Usage() (Usage, error) {
	result, err := callMethod(l, "usage", nil)
	if err != nil {
		return nil, fmt.Errorf("failed to get usage: %w", err)
	}

	if result == nil {
		return nil, nil
	}

	usage, ok := result.(Usage)
	if !ok {
		return nil, fmt.Errorf("unexpected type for usage: %T", result)
	}

	return usage, nil
}

func (l *llmCall) Selected() (bool, error) {
	result, err := callMethod(l, "selected", nil)
	if err != nil {
		return false, fmt.Errorf("failed to get selected: %w", err)
	}

	selected, ok := result.(bool)
	if !ok {
		return false, fmt.Errorf("unexpected type for selected: %T", result)
	}

	return selected, nil
}

func (l *llmCall) Timing() (Timing, error) {
	result, err := callMethod(l, "timing", nil)
	if err != nil {
		return nil, fmt.Errorf("failed to get timing: %w", err)
	}

	timing, ok := result.(Timing)
	if !ok {
		return nil, fmt.Errorf("unexpected type for timing: %T", result)
	}

	return timing, nil
}

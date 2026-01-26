package baml

import (
	"fmt"
	"unsafe"

	"github.com/boundaryml/baml/engine/language_client_go/baml_go/raw_objects"
	"github.com/boundaryml/baml/engine/language_client_go/pkg/cffi"
)

type httpRequest struct {
	*raw_objects.RawObject
}

func newHttpRequest(ptr int64, rt unsafe.Pointer) HTTPRequest {
	return &httpRequest{raw_objects.FromPointer(ptr, rt)}
}

func (h *httpRequest) ObjectType() cffi.BamlObjectType {
	return cffi.BamlObjectType_OBJECT_HTTP_REQUEST
}

func (h *httpRequest) pointer() int64 {
	return h.RawObject.Pointer()
}

func (h *httpRequest) RequestId() (string, error) {
	result, err := raw_objects.CallMethod(h, "id", nil)
	if err != nil {
		return "", fmt.Errorf("failed to get ID: %w", err)
	}

	id, ok := result.(string)
	if !ok {
		return "", fmt.Errorf("unexpected type for ID: %T", result)
	}

	return id, nil
}

func (h *httpRequest) Url() (string, error) {
	result, err := raw_objects.CallMethod(h, "url", nil)
	if err != nil {
		return "", fmt.Errorf("failed to get URL: %w", err)
	}

	url, ok := result.(string)
	if !ok {
		return "", fmt.Errorf("unexpected type for URL: %T", result)
	}

	return url, nil
}

func (h *httpRequest) Method() (string, error) {
	result, err := raw_objects.CallMethod(h, "method", nil)
	if err != nil {
		return "", fmt.Errorf("failed to get method: %w", err)
	}

	method, ok := result.(string)
	if !ok {
		return "", fmt.Errorf("unexpected type for method: %T", result)
	}

	return method, nil
}

func (h *httpRequest) Headers() (map[string]string, error) {
	result, err := raw_objects.CallMethod(h, "headers", nil)
	if err != nil {
		return nil, fmt.Errorf("failed to get headers: %w", err)
	}

	headers, ok := result.(map[string]string)
	if !ok {
		return nil, fmt.Errorf("unexpected type for headers: %T", result)
	}

	return headers, nil
}

func (h *httpRequest) Body() (HTTPBody, error) {
	result, err := raw_objects.CallMethod(h, "body", nil)
	if err != nil {
		return nil, fmt.Errorf("failed to get body: %w", err)
	}

	body, ok := result.(HTTPBody)
	if !ok {
		return nil, fmt.Errorf("unexpected type for body: %T", result)
	}

	return body, nil
}

package baml

import (
	"fmt"
	"unsafe"

	"github.com/boundaryml/baml/engine/language_client_go/baml_go/raw_objects"
	"github.com/boundaryml/baml/engine/language_client_go/pkg/cffi"
)

type httpResponse struct {
	*raw_objects.RawObject
}

func newHttpResponse(ptr int64, rt unsafe.Pointer) HTTPResponse {
	return &httpResponse{raw_objects.FromPointer(ptr, rt)}
}

func (h *httpResponse) ObjectType() cffi.CFFIObjectType {
	return cffi.CFFIObjectType_OBJECT_HTTP_RESPONSE
}

func (h *httpResponse) pointer() int64 {
	return h.RawObject.Pointer()
}

func (h *httpResponse) RequestId() (string, error) {
	result, err := raw_objects.CallMethod(h, "id", nil)
	if err != nil {
		return "", fmt.Errorf("failed to get id: %w", err)
	}

	id, ok := result.(string)
	if !ok {
		return "", fmt.Errorf("unexpected type for id: %T", result)
	}

	return id, nil
}

func (h *httpResponse) Status() (int64, error) {
	result, err := raw_objects.CallMethod(h, "status", nil)
	if err != nil {
		return 0, fmt.Errorf("failed to get status: %w", err)
	}

	status, ok := result.(int64)
	if !ok {
		return 0, fmt.Errorf("unexpected type for status: %T", result)
	}

	return status, nil
}

func (h *httpResponse) Headers() (map[string]string, error) {
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

func (h *httpResponse) Body() (HTTPBody, error) {
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

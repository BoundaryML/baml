package baml

import (
	"fmt"
	"unsafe"

	"github.com/boundaryml/baml/engine/language_client_go/baml_go/raw_objects"
	"github.com/boundaryml/baml/engine/language_client_go/pkg/cffi"
)

type sseResponse struct {
	*raw_objects.RawObject
}

func newSSEResponse(ptr int64, rt unsafe.Pointer) SSEResponse {
	return &sseResponse{raw_objects.FromPointer(ptr, rt)}
}

func (s *sseResponse) ObjectType() cffi.CFFIObjectType {
	return cffi.CFFIObjectType_OBJECT_SSE_RESPONSE
}

func (s *sseResponse) pointer() int64 {
	return s.RawObject.Pointer()
}

func (s *sseResponse) Text() (string, error) {
	result, err := raw_objects.CallMethod(s, "text", nil)
	if err != nil {
		return "", fmt.Errorf("failed to get text: %w", err)
	}

	text, ok := result.(string)
	if !ok {
		return "", fmt.Errorf("unexpected type for text: %T", result)
	}

	return text, nil
}

func (s *sseResponse) JSON() (any, error) {
	result, err := raw_objects.CallMethod(s, "json", nil)
	if err != nil {
		return nil, fmt.Errorf("failed to get JSON: %w", err)
	}

	return result, nil
}

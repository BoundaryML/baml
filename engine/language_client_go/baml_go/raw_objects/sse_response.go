package raw_objects

import (
	"fmt"

	"github.com/boundaryml/baml/engine/language_client_go/pkg/cffi"
)

type sseResponse struct {
	*rawObject
}

func newSSEResponse(ptr int64) SSEResponse {
	return &sseResponse{&rawObject{ptr: ptr}}
}

func (s *sseResponse) objectType() cffi.CFFIObjectType {
	return cffi.CFFIObjectType_OBJECT_SSE_RESPONSE
}

func (s *sseResponse) pointer() int64 {
	return s.rawObject.pointer()
}

func (s *sseResponse) Text() (string, error) {
	result, err := callMethod(s, "text", nil)
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
	result, err := callMethod(s, "json", nil)
	if err != nil {
		return nil, fmt.Errorf("failed to get JSON: %w", err)
	}

	return result, nil
}

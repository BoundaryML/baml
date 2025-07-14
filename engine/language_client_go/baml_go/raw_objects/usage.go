package raw_objects

import (
	"fmt"

	"github.com/boundaryml/baml/engine/language_client_go/pkg/cffi"
)

type usage struct {
	*rawObject
}

func newUsage(ptr int64) Usage {
	return &usage{&rawObject{ptr: ptr}}
}

func (u *usage) objectType() cffi.CFFIObjectType {
	return cffi.CFFIObjectType_OBJECT_USAGE
}

func (u *usage) pointer() int64 {
	return u.rawObject.pointer()
}

func (u *usage) InputTokens() (int64, error) {
	result, err := callMethod(u, "input_tokens", nil)
	if err != nil {
		return 0, fmt.Errorf("failed to get input tokens: %w", err)
	}

	tokens, ok := result.(int64)
	if !ok {
		return 0, fmt.Errorf("unexpected type for input tokens: %T", result)
	}

	return tokens, nil
}

func (u *usage) OutputTokens() (int64, error) {
	result, err := callMethod(u, "output_tokens", nil)
	if err != nil {
		return 0, fmt.Errorf("failed to get output tokens: %w", err)
	}

	tokens, ok := result.(int64)
	if !ok {
		return 0, fmt.Errorf("unexpected type for output tokens: %T", result)
	}

	return tokens, nil
}

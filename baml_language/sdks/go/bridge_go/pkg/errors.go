package pkg

import "strings"

// BamlError is the base error type for all BAML errors.
type BamlError struct {
	Message string
}

func (e *BamlError) Error() string { return e.Message }

// BamlInvalidArgumentError indicates invalid input.
type BamlInvalidArgumentError struct{ BamlError }

// BamlClientError indicates an engine/runtime error.
type BamlClientError struct{ BamlError }

// BamlCancelledError indicates the operation was cancelled.
type BamlCancelledError struct{ BamlError }

// ParseBamlError parses a prefixed error string from Rust into a typed Go error.
func ParseBamlError(msg string) error {
	if strings.HasPrefix(msg, "BamlError: BamlInvalidArgumentError: ") {
		return &BamlInvalidArgumentError{BamlError{Message: msg}}
	}
	if strings.HasPrefix(msg, "BamlError: BamlCancelledError: ") {
		return &BamlCancelledError{BamlError{Message: msg}}
	}
	if strings.HasPrefix(msg, "BamlError: BamlClientError: ") {
		return &BamlClientError{BamlError{Message: msg}}
	}
	return &BamlError{Message: msg}
}

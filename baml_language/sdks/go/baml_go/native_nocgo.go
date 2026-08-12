//go:build !cgo

package baml_go

import "fmt"

func unsupportedNativeOperation() error {
	return fmt.Errorf("BAML native runtime requires cgo; rebuild with CGO_ENABLED=1")
}

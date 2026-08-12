//go:build cgo && !darwin && !linux && !windows

package baml_go

import (
	"fmt"
	"runtime"
)

func unsupportedNativeOperation() error {
	return fmt.Errorf("BAML native runtime loader is not implemented for %s/%s", runtime.GOOS, runtime.GOARCH)
}

//go:build !cgo || (!darwin && !linux && !windows)

package baml_go

import (
	"fmt"
	"runtime"
)

func nativeOpen(string) (string, error) {
	if !cgoEnabled {
		return "", fmt.Errorf("BAML native runtime requires cgo; rebuild with CGO_ENABLED=1")
	}
	return "", fmt.Errorf("BAML native runtime loader is not implemented for %s/%s", runtime.GOOS, runtime.GOARCH)
}

func nativeCloseAfterLoadFailure() {}

func nativeRegisterBridge(string) error { return unsupportedNativeOperation() }

func nativeRuntimeTarget() (string, error) {
	return "", unsupportedNativeOperation()
}

func nativeInitialize([]byte) error     { return unsupportedNativeOperation() }
func nativeRegisterCallback()           {}
func nativeNewFunctionCall() uint64     { return 0 }
func nativeCall(string, []byte, uint32) {}
func nativeCancel(uint64) int32         { return 1 }

func unsupportedNativeOperation() error {
	return fmt.Errorf("BAML native runtime loader is not implemented for %s/%s", runtime.GOOS, runtime.GOARCH)
}

const cgoEnabled = false

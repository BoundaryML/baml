//go:build !cgo || (!darwin && !linux && !windows)

package baml_go

import "fmt"
import "github.com/boundaryml/baml-go/internal/cffi"

func nativeOpen(string) (string, error) {
	return "", unsupportedNativeOperation()
}

func nativeCloseAfterLoadFailure() {}

func nativeRegisterBridge(string, string, string) error { return unsupportedNativeOperation() }

func nativeRuntimeTarget() (string, error) {
	return "", unsupportedNativeOperation()
}

func nativeInitialize([]byte, string) error       { return unsupportedNativeOperation() }
func nativeRegisterCallback()                     {}
func nativeRegisterUnhandledSpawnErrorCallback()  {}
func nativeShutdown() error                       { return unsupportedNativeOperation() }
func nativeNewFunctionCall() uint64               { return 0 }
func nativeReleaseFunctionCall(id uint64)         {}
func nativeCall([]byte, uint32)                   {}
func nativeCancel(uint64) int32                   { return 1 }
func nativeCompleteHostCall(uint32, bool, []byte) {}
func nativeHandleClone(uint64) (uint64, error) {
	return 0, unsupportedNativeOperation()
}
func nativeHandleRelease(uint64) {}
func nativeMediaConstruct(mediaConstructor, cffi.MediaTypeEnum, string, *string) (uint64, cffi.BamlHandleType, error) {
	return 0, cffi.BamlHandleType_HANDLE_UNSPECIFIED, unsupportedNativeOperation()
}
func nativeMediaAccess(mediaAccessor, uint64, cffi.BamlHandleType) (*string, error) {
	return nil, unsupportedNativeOperation()
}

func nativeRegisterProgram(uint64, []byte, string) error { return unsupportedNativeOperation() }
func nativeCallKeyed(uint64, []byte, uint32)             {}

func nativeCreateRuntime(bytes []byte) (uint64, error) { return 0, fmt.Errorf("unsupported platform") }
func nativeUnregisterRuntime(key uint64) error         { return fmt.Errorf("unsupported platform") }

//go:build !cgo || (!darwin && !linux && !windows)

package baml_go

func nativeOpen(string) (string, error) {
	return "", unsupportedNativeOperation()
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

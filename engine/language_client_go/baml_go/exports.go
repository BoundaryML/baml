package baml_go

import (
	"fmt"
	"unsafe"
)

/*
#cgo CFLAGS: -I${SRCDIR}
#cgo CFLAGS: -O3 -g
#include <baml_cffi_wrapper.h>
#include <stdlib.h>
#include <stdint.h>
#include <string.h>
*/
import "C"

func CreateBamlRuntime(rootPath string, srcFilesJson string, envVarsJson string) (unsafe.Pointer, error) {
	cRootPath := C.CString(rootPath)
	defer C.free(unsafe.Pointer(cRootPath))

	cSrcFilesJson := C.CString(srcFilesJson)
	defer C.free(unsafe.Pointer(cSrcFilesJson))

	cEnvVarsJson := C.CString(envVarsJson)
	defer C.free(unsafe.Pointer(cEnvVarsJson))

	runtime := C.WrapCreateBamlRuntime(cRootPath, cSrcFilesJson, cEnvVarsJson)
	if runtime == nil {
		return nil, fmt.Errorf("failed to create BAML runtime")
	}
	return runtime, nil
}

func DestroyBamlRuntime(runtime unsafe.Pointer) error {
	C.WrapDestroyBamlRuntime(runtime)
	return nil
}

func BamlVersion() string {
	return C.GoString(C.WrapVersion())
}

func InvokeRuntimeCli(args []string) (int, error) {
	arg_c_strings := make([]*C.char, len(args)+1)
	for i, arg := range args {
		arg_c_strings[i] = C.CString(arg)
	}
	
	defer func() {
		for i := 0; i < len(args); i++ {
			C.free(unsafe.Pointer(arg_c_strings[i]))
		}
	}()

	result := C.WrapInvokeRuntimeCli((**C.char)(unsafe.Pointer(&arg_c_strings[0])))

	return int(result), nil
}

func RegisterCallbacks(callbackFn unsafe.Pointer, errorFn unsafe.Pointer, onTickFn unsafe.Pointer) error {
	C.WrapRegisterCallbacks((C.CallbackFn)(callbackFn), (C.CallbackFn)(errorFn), (C.OnTickCallbackFn)(onTickFn))
	return nil
}

func CallFunctionFromC(runtime unsafe.Pointer, functionName string, encodedArgs []byte, id uint32) (unsafe.Pointer, error) {
	cFunctionName := C.CString(functionName)
	defer C.free(unsafe.Pointer(cFunctionName))

	cEncodedArgs := (*C.char)(unsafe.Pointer(&encodedArgs[0]))

	result := C.WrapCallFunctionFromC(runtime, cFunctionName, cEncodedArgs, C.uintptr_t(len(encodedArgs)), C.uint32_t(id))

	return result, nil
}

func CallFunctionStreamFromC(runtime unsafe.Pointer, functionName string, encodedArgs []byte, id uint32) (unsafe.Pointer, error) {
	cFunctionName := C.CString(functionName)
	defer C.free(unsafe.Pointer(cFunctionName))

	cEncodedArgs := (*C.char)(unsafe.Pointer(&encodedArgs[0]))

	result := C.WrapCallFunctionStreamFromC(runtime, cFunctionName, cEncodedArgs, C.uintptr_t(len(encodedArgs)), C.uint32_t(id))

	return result, nil
}

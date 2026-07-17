//go:build (darwin || linux) && cgo

package baml_go

/*
#cgo linux LDFLAGS: -ldl
#cgo CFLAGS: -I${SRCDIR}/internal/cffi/include

#include "baml_cffi.h"
#include <dlfcn.h>
#include <stdio.h>
#include <stdlib.h>

#if defined(__linux__)
#include <features.h>
#endif

static int baml_is_musl(void) {
#if defined(__linux__) && !defined(__GLIBC__)
	return 1;
#else
	return 0;
#endif
}

extern void bamlGoResultCallback(uint32_t call_id, int8_t *content, size_t length);

static void baml_go_result_callback(uint32_t call_id, const int8_t *content, size_t length) {
	bamlGoResultCallback(call_id, (int8_t *)content, length);
}

static void *baml_library_handle = NULL;
static const BamlApiV1 *baml_api = NULL;
static char baml_loader_error[512];

static const char *baml_open_library(const char *path) {
	if (baml_api != NULL) {
		return NULL;
	}

	dlerror();
	void *handle = dlopen(path, RTLD_NOW | RTLD_LOCAL);
	if (handle == NULL) {
		const char *error = dlerror();
		snprintf(baml_loader_error, sizeof(baml_loader_error), "dlopen: %s", error == NULL ? "unknown error" : error);
		return baml_loader_error;
	}

	dlerror();
	void *symbol = dlsym(handle, "baml_get_api_v1");
	const char *symbol_error = dlerror();
	if (symbol_error != NULL || symbol == NULL) {
		snprintf(baml_loader_error, sizeof(baml_loader_error), "dlsym(baml_get_api_v1): %s", symbol_error == NULL ? "symbol not found" : symbol_error);
		dlclose(handle);
		return baml_loader_error;
	}

	const BamlApiV1 *api = ((BamlGetApiV1Fn)symbol)();
	if (api == NULL) {
		snprintf(baml_loader_error, sizeof(baml_loader_error), "baml_get_api_v1 returned NULL");
		dlclose(handle);
		return baml_loader_error;
	}
	if (!baml_api_v1_is_compatible(api)) {
		if (api->abi_version != BAML_API_V1_ABI_VERSION) {
			snprintf(baml_loader_error, sizeof(baml_loader_error), "unsupported BAML ABI version %u (expected %u)", api->abi_version, BAML_API_V1_ABI_VERSION);
		} else {
			snprintf(baml_loader_error, sizeof(baml_loader_error), "truncated BAML ABI v1 table: got %zu bytes, need at least %zu", api->struct_size, (size_t)BAML_API_V1_MIN_SIZE);
		}
		dlclose(handle);
		return baml_loader_error;
	}
	if (api->version == NULL || api->initialize_runtime_from_bytecode == NULL ||
		api->free_buffer == NULL || api->register_callback == NULL ||
		api->call_function == NULL || api->new_function_call == NULL ||
		api->cancel_function_call == NULL ||
		api->register_host_dispatch_callback == NULL ||
		api->register_host_release_callback == NULL ||
		api->complete_host_call == NULL || api->handle_clone == NULL ||
		api->handle_release == NULL || api->media_from_url == NULL ||
		api->media_from_file == NULL || api->media_from_base64 == NULL ||
		api->media_url == NULL || api->media_file == NULL ||
		api->media_base64 == NULL || api->media_mime_type == NULL ||
		api->register_bridge == NULL) {
		snprintf(baml_loader_error, sizeof(baml_loader_error), "BAML ABI v1 table contains a NULL required function");
		dlclose(handle);
		return baml_loader_error;
	}

	baml_library_handle = handle;
	baml_api = api;
	return NULL;
}

static void baml_close_library_after_load_failure(void) {
	if (baml_library_handle != NULL) {
		dlclose(baml_library_handle);
	}
	baml_library_handle = NULL;
	baml_api = NULL;
}

static BamlBuffer baml_version(void) { return baml_api->version(); }
static BamlBuffer baml_register_go_bridge(const uint8_t *sdk_version, size_t length) {
	const BamlBridgeInfoV1 info = {
		.struct_size = sizeof(BamlBridgeInfoV1),
		.language = BAML_BRIDGE_LANGUAGE_GO,
		.sdk_version = sdk_version,
		.sdk_version_len = length,
	};
	return baml_api->register_bridge(&info);
}
static BamlBuffer baml_initialize(const uint8_t *bytecode, size_t length) {
	return baml_api->initialize_runtime_from_bytecode(bytecode, length);
}
static void baml_free_buffer(BamlBuffer buffer) { baml_api->free_buffer(buffer); }
static void baml_register_go_callback(void) { baml_api->register_callback(baml_go_result_callback); }
static uint64_t baml_new_function_call(void) { return baml_api->new_function_call(); }
static void baml_call_function(const char *name, const uint8_t *args, size_t length, uint32_t callback_id) {
	baml_api->call_function(name, args, length, callback_id);
}
static int32_t baml_cancel_function_call(uint64_t call_id) { return baml_api->cancel_function_call(call_id); }
*/
import "C"

import (
	"fmt"
	"runtime"
	"unsafe"
)

func nativeOpen(path string) (string, error) {
	cPath := C.CString(path)
	defer C.free(unsafe.Pointer(cPath))
	if message := C.baml_open_library(cPath); message != nil {
		return "", fmt.Errorf("load BAML runtime %q: %s", path, C.GoString(message))
	}
	buffer := C.baml_version()
	if buffer.ptr == nil || buffer.len == 0 {
		if buffer.ptr != nil {
			C.baml_free_buffer(buffer)
		}
		C.baml_close_library_after_load_failure()
		return "", fmt.Errorf("load BAML runtime %q: runtime returned an empty version", path)
	}
	version := C.GoStringN((*C.char)(unsafe.Pointer(buffer.ptr)), C.int(buffer.len))
	C.baml_free_buffer(buffer)
	return version, nil
}

func nativeCloseAfterLoadFailure() { C.baml_close_library_after_load_failure() }

func nativeRegisterBridge(sdkVersion string) error {
	bytes := []byte(sdkVersion)
	var pointer *C.uint8_t
	if len(bytes) != 0 {
		pointer = (*C.uint8_t)(unsafe.Pointer(&bytes[0]))
	}
	buffer := C.baml_register_go_bridge(pointer, C.size_t(len(bytes)))
	defer C.baml_free_buffer(buffer)
	if buffer.len == 0 {
		return nil
	}
	message := C.GoBytes(unsafe.Pointer(buffer.ptr), C.int(buffer.len))
	return fmt.Errorf("%s", message)
}

func nativeInitialize(bytecode []byte) error {
	var pointer *C.uint8_t
	if len(bytecode) != 0 {
		pointer = (*C.uint8_t)(unsafe.Pointer(&bytecode[0]))
	}
	buffer := C.baml_initialize(pointer, C.size_t(len(bytecode)))
	defer C.baml_free_buffer(buffer)
	if buffer.len == 0 {
		return nil
	}
	message := C.GoBytes(unsafe.Pointer(buffer.ptr), C.int(buffer.len))
	return fmt.Errorf("initialize BAML runtime: %s", message)
}

func nativeRegisterCallback() { C.baml_register_go_callback() }

func nativeNewFunctionCall() uint64 { return uint64(C.baml_new_function_call()) }

func nativeCall(function string, encoded []byte, callbackID uint32) {
	functionName := C.CString(function)
	defer C.free(unsafe.Pointer(functionName))
	var encodedPointer *C.uint8_t
	if len(encoded) != 0 {
		encodedPointer = (*C.uint8_t)(unsafe.Pointer(&encoded[0]))
	}
	C.baml_call_function(functionName, encodedPointer, C.size_t(len(encoded)), C.uint32_t(callbackID))
}

func nativeCancel(callID uint64) int32 {
	return int32(C.baml_cancel_function_call(C.uint64_t(callID)))
}

func nativeRuntimeTarget() (string, error) {
	switch runtime.GOOS {
	case "darwin":
		switch runtime.GOARCH {
		case "arm64":
			return "aarch64-apple-darwin", nil
		case "amd64":
			return "x86_64-apple-darwin", nil
		}
	case "linux":
		libc := "gnu"
		if C.baml_is_musl() != 0 {
			libc = "musl"
		}
		switch runtime.GOARCH {
		case "arm64":
			return "aarch64-unknown-linux-" + libc, nil
		case "amd64":
			return "x86_64-unknown-linux-" + libc, nil
		}
	}
	return "", fmt.Errorf("BAML has no released native runtime target for %s/%s", runtime.GOOS, runtime.GOARCH)
}

//export bamlGoResultCallback
func bamlGoResultCallback(callID C.uint32_t, content *C.int8_t, length C.size_t) {
	value, ok := pendingCalls.LoadAndDelete(uint32(callID))
	if !ok {
		return
	}
	call := value.(*pendingCall)
	payload := C.GoBytes(unsafe.Pointer(content), C.int(length))
	call.result <- payload
}

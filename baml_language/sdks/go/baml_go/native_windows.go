//go:build windows && cgo

package baml_go

/*
#cgo CFLAGS: -I${SRCDIR}/internal/cffi/include

#include "baml_cffi.h"
#include <windows.h>
#include <stdio.h>
#include <stdlib.h>

extern void bamlGoResultCallback(uint32_t call_id, int8_t *content, size_t length);
extern void bamlGoUnhandledSpawnErrorCallback(int8_t *content, size_t length, int32_t cancelled);
extern void bamlGoHostDispatch(uint64_t host_value_key, uint32_t call_id, uint8_t *args, size_t length);
extern void bamlGoHostRelease(uint64_t host_value_key);

static void baml_go_result_callback(uint32_t call_id, const int8_t *content, size_t length) {
	bamlGoResultCallback(call_id, (int8_t *)content, length);
}
static void baml_go_host_dispatch(uint64_t host_value_key, uint32_t call_id, const uint8_t *args, size_t length) {
	bamlGoHostDispatch(host_value_key, call_id, (uint8_t *)args, length);
}
static void baml_go_host_release(uint64_t host_value_key) { bamlGoHostRelease(host_value_key); }

static void baml_go_unhandled_spawn_error_callback(const int8_t *content, size_t length, int32_t cancelled) {
	bamlGoUnhandledSpawnErrorCallback((int8_t *)content, length, cancelled);
}

static HMODULE baml_library_handle = NULL;
static const BamlApiV1 *baml_api = NULL;
static char baml_loader_error[512];

static const char *baml_open_library(const wchar_t *path) {
	if (baml_api != NULL) return NULL;
	HMODULE handle = LoadLibraryW(path);
	if (handle == NULL) {
		snprintf(baml_loader_error, sizeof(baml_loader_error), "LoadLibraryW failed with Windows error %lu", (unsigned long)GetLastError());
		return baml_loader_error;
	}
	FARPROC symbol = GetProcAddress(handle, "baml_get_api_v1");
	if (symbol == NULL) {
		snprintf(baml_loader_error, sizeof(baml_loader_error), "GetProcAddress(baml_get_api_v1) failed with Windows error %lu", (unsigned long)GetLastError());
		FreeLibrary(handle);
		return baml_loader_error;
	}
	const BamlApiV1 *api = ((BamlGetApiV1Fn)symbol)();
	if (api == NULL) {
		snprintf(baml_loader_error, sizeof(baml_loader_error), "baml_get_api_v1 returned NULL");
		FreeLibrary(handle);
		return baml_loader_error;
	}
	if (!baml_api_v1_is_compatible(api)) {
		if (api->abi_version != BAML_API_V1_ABI_VERSION) {
			snprintf(baml_loader_error, sizeof(baml_loader_error), "unsupported BAML ABI version %u (expected %u)", api->abi_version, BAML_API_V1_ABI_VERSION);
		} else {
			snprintf(baml_loader_error, sizeof(baml_loader_error), "truncated BAML ABI v1 table: got %zu bytes, need at least %zu", api->struct_size, (size_t)BAML_API_V1_MIN_SIZE);
		}
		FreeLibrary(handle);
		return baml_loader_error;
	}
	const size_t required_size = offsetof(BamlApiV1, release_function_call) + sizeof(api->release_function_call);
	if (api->struct_size < required_size) {
		snprintf(baml_loader_error, sizeof(baml_loader_error), "truncated BAML ABI v1 table: got %zu bytes, need at least %zu", api->struct_size, required_size);
		FreeLibrary(handle);
		return baml_loader_error;
	}
	if (api->version == NULL || api->initialize_runtime_from_bytecode == NULL ||
		api->free_buffer == NULL || api->register_callback == NULL ||
		api->call_function == NULL || api->new_function_call == NULL ||
		api->cancel_function_call == NULL || api->release_function_call == NULL ||
		api->register_host_dispatch_callback == NULL ||
		api->register_host_release_callback == NULL ||
		api->complete_host_call == NULL || api->handle_clone == NULL ||
		api->handle_release == NULL || api->media_from_url == NULL ||
		api->media_from_file == NULL || api->media_from_base64 == NULL ||
		api->media_url == NULL || api->media_file == NULL ||
		api->media_base64 == NULL || api->media_mime_type == NULL ||
		api->register_bridge == NULL ||
		api->register_unhandled_spawn_error_callback == NULL ||
		api->shutdown_runtime == NULL ||
		api->initialize_runtime_from_bytecode_with_metadata == NULL || api->create_runtime == NULL || api->unregister_runtime == NULL || api->register_program == NULL || api->call_function_for_runtime == NULL) {
		snprintf(baml_loader_error, sizeof(baml_loader_error), "BAML ABI v1 table contains a NULL required function");
		FreeLibrary(handle);
		return baml_loader_error;
	}
	baml_library_handle = handle;
	baml_api = api;
	return NULL;
}

static void baml_close_library_after_load_failure(void) {
	if (baml_library_handle != NULL) FreeLibrary(baml_library_handle);
	baml_library_handle = NULL;
	baml_api = NULL;
}

static BamlBuffer baml_version(void) { return baml_api->version(); }
static BamlBuffer baml_register_go_bridge(const uint8_t *runtime_name, size_t runtime_name_length, const uint8_t *sdk_version, size_t length, const uint8_t *runtime_version, size_t runtime_version_length) {
	const BamlBridgeInfoV1 info = {
		.struct_size = sizeof(BamlBridgeInfoV1),
		.language = BAML_BRIDGE_LANGUAGE_GO,
		.sdk_version = sdk_version,
		.sdk_version_len = length,
		.bridge_runtime_name = runtime_name,
		.bridge_runtime_name_len = runtime_name_length,
		.bridge_runtime_version = runtime_version,
		.bridge_runtime_version_len = runtime_version_length,
	};
	return baml_api->register_bridge(&info);
}
static BamlBuffer baml_initialize(const uint8_t *bytecode, size_t length) { return baml_api->initialize_runtime_from_bytecode(bytecode, length); }
static BamlBuffer baml_initialize_with_metadata(const uint8_t *bytecode, size_t length, const char *baml_toml) { return baml_api->initialize_runtime_from_bytecode_with_metadata(bytecode, length, baml_toml); }
static void baml_free_buffer(BamlBuffer buffer) { baml_api->free_buffer(buffer); }
static void baml_register_go_callback(void) {
	baml_api->register_callback(baml_go_result_callback);
	baml_api->register_host_dispatch_callback(baml_go_host_dispatch);
	baml_api->register_host_release_callback(baml_go_host_release);
}
static void baml_register_go_unhandled_spawn_error_callback(void) { baml_api->register_unhandled_spawn_error_callback(baml_go_unhandled_spawn_error_callback); }
static BamlBuffer baml_shutdown(void) { return baml_api->shutdown_runtime(); }
static uint64_t baml_new_function_call(void) { return baml_api->new_function_call(); }
static void baml_release_function_call(uint64_t id) { baml_api->release_function_call(id); }
static void baml_call_function(const uint8_t *args, size_t length, uint32_t callback_id) { baml_api->call_function(args, length, callback_id); }
static int32_t baml_cancel_function_call(uint64_t call_id) { return baml_api->cancel_function_call(call_id); }
static void baml_complete_host_call_go(uint32_t call_id, int32_t is_error, const uint8_t *content, size_t length) { baml_api->complete_host_call(call_id, is_error, (const int8_t *)content, length); }
static uint32_t baml_handle_clone_go(uint64_t key, uint64_t *out_key) { return baml_api == NULL ? BAML_CFFI_STATUS_INTERNAL_ERROR : baml_api->handle_clone(key, out_key); }
static uint32_t baml_handle_release_go(uint64_t key) { return baml_api == NULL ? BAML_CFFI_STATUS_INTERNAL_ERROR : baml_api->handle_release(key); }
static uint32_t baml_media_from_url_go(int32_t kind, const char *value, const char *mime_type, uint64_t *out_key, int32_t *out_handle_type) { return baml_api == NULL ? BAML_CFFI_STATUS_INTERNAL_ERROR : baml_api->media_from_url(kind, value, mime_type, out_key, out_handle_type); }
static uint32_t baml_media_from_file_go(int32_t kind, const char *value, const char *mime_type, uint64_t *out_key, int32_t *out_handle_type) { return baml_api == NULL ? BAML_CFFI_STATUS_INTERNAL_ERROR : baml_api->media_from_file(kind, value, mime_type, out_key, out_handle_type); }
static uint32_t baml_media_from_base64_go(int32_t kind, const char *value, const char *mime_type, uint64_t *out_key, int32_t *out_handle_type) { return baml_api == NULL ? BAML_CFFI_STATUS_INTERNAL_ERROR : baml_api->media_from_base64(kind, value, mime_type, out_key, out_handle_type); }
static uint32_t baml_media_url_go(uint64_t key, int32_t handle_type, BamlBuffer *out) { return baml_api == NULL ? BAML_CFFI_STATUS_INTERNAL_ERROR : baml_api->media_url(key, handle_type, out); }
static uint32_t baml_media_file_go(uint64_t key, int32_t handle_type, BamlBuffer *out) { return baml_api == NULL ? BAML_CFFI_STATUS_INTERNAL_ERROR : baml_api->media_file(key, handle_type, out); }
static uint32_t baml_media_base64_go(uint64_t key, int32_t handle_type, BamlBuffer *out) { return baml_api == NULL ? BAML_CFFI_STATUS_INTERNAL_ERROR : baml_api->media_base64(key, handle_type, out); }
static uint32_t baml_media_mime_type_go(uint64_t key, int32_t handle_type, BamlBuffer *out) { return baml_api == NULL ? BAML_CFFI_STATUS_INTERNAL_ERROR : baml_api->media_mime_type(key, handle_type, out); }

static BamlBuffer baml_create_runtime(const uint8_t *bytes, size_t length, uint64_t *key) { return baml_api->create_runtime(bytes, length, key); }
static BamlBuffer baml_unregister_runtime(uint64_t key) { return baml_api->unregister_runtime(key); }
static BamlBuffer baml_register_program(uint64_t key, const uint8_t *bytes, size_t length, const char *metadata) {
  return baml_api->register_program(key, bytes, length, metadata);
}
static void baml_call_keyed(uint64_t key, const uint8_t *bytes, size_t length, uint32_t callback) {
  baml_api->call_function_for_runtime(key, bytes, length, callback);
}
*/
import "C"

import (
	"fmt"
	"runtime"
	"syscall"
	"unsafe"

	"github.com/boundaryml/baml-go/internal/cffi"
)

func nativeOpen(path string) (string, error) {
	windowsPath, err := syscall.UTF16PtrFromString(path)
	if err != nil {
		return "", fmt.Errorf("encode BAML runtime path: %w", err)
	}
	if message := C.baml_open_library((*C.wchar_t)(unsafe.Pointer(windowsPath))); message != nil {
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

func nativeRegisterBridge(runtimeName string, sdkVersion string, runtimeVersion string) error {
	nameBytes := []byte(runtimeName)
	bytes := []byte(sdkVersion)
	runtimeBytes := []byte(runtimeVersion)
	var namePointer *C.uint8_t
	var pointer *C.uint8_t
	var runtimePointer *C.uint8_t
	if len(nameBytes) != 0 {
		namePointer = (*C.uint8_t)(unsafe.Pointer(&nameBytes[0]))
	}
	if len(bytes) != 0 {
		pointer = (*C.uint8_t)(unsafe.Pointer(&bytes[0]))
	}
	if len(runtimeBytes) != 0 {
		runtimePointer = (*C.uint8_t)(unsafe.Pointer(&runtimeBytes[0]))
	}
	buffer := C.baml_register_go_bridge(namePointer, C.size_t(len(nameBytes)), pointer, C.size_t(len(bytes)), runtimePointer, C.size_t(len(runtimeBytes)))
	defer C.baml_free_buffer(buffer)
	if buffer.len == 0 {
		return nil
	}
	message := C.GoBytes(unsafe.Pointer(buffer.ptr), C.int(buffer.len))
	return fmt.Errorf("%s", message)
}

func nativeInitialize(bytecode []byte, embeddedBamlToml string) error {
	var pointer *C.uint8_t
	if len(bytecode) != 0 {
		pointer = (*C.uint8_t)(unsafe.Pointer(&bytecode[0]))
	}
	var buffer C.BamlBuffer
	if embeddedBamlToml == "" {
		buffer = C.baml_initialize(pointer, C.size_t(len(bytecode)))
	} else {
		manifest := C.CString(embeddedBamlToml)
		defer C.free(unsafe.Pointer(manifest))
		buffer = C.baml_initialize_with_metadata(pointer, C.size_t(len(bytecode)), manifest)
	}
	defer C.baml_free_buffer(buffer)
	if buffer.len == 0 {
		return nil
	}
	message := C.GoBytes(unsafe.Pointer(buffer.ptr), C.int(buffer.len))
	return fmt.Errorf("%s", message)
}

func nativeRegisterUnhandledSpawnErrorCallback() {
	C.baml_register_go_unhandled_spawn_error_callback()
}

func nativeShutdown() error {
	buffer := C.baml_shutdown()
	defer C.baml_free_buffer(buffer)
	if buffer.len == 0 {
		return nil
	}
	return fmt.Errorf("shutdown BAML runtime: %s", C.GoBytes(unsafe.Pointer(buffer.ptr), C.int(buffer.len)))
}

func nativeRegisterCallback()             { C.baml_register_go_callback() }
func nativeNewFunctionCall() uint64       { return uint64(C.baml_new_function_call()) }
func nativeReleaseFunctionCall(id uint64) { C.baml_release_function_call(C.uint64_t(id)) }

func nativeCall(encoded []byte, callbackID uint32) {
	var encodedPointer *C.uint8_t
	if len(encoded) != 0 {
		encodedPointer = (*C.uint8_t)(unsafe.Pointer(&encoded[0]))
	}
	C.baml_call_function(encodedPointer, C.size_t(len(encoded)), C.uint32_t(callbackID))
}

func nativeCancel(callID uint64) int32 {
	return int32(C.baml_cancel_function_call(C.uint64_t(callID)))
}

func nativeCompleteHostCall(callID uint32, isError bool, content []byte) {
	var pointer *C.uint8_t
	if len(content) != 0 {
		pointer = (*C.uint8_t)(unsafe.Pointer(&content[0]))
	}
	flag := C.int32_t(0)
	if isError {
		flag = 1
	}
	C.baml_complete_host_call_go(C.uint32_t(callID), flag, pointer, C.size_t(len(content)))
}

func nativeHandleClone(key uint64) (uint64, error) {
	var cloned C.uint64_t
	status := uint32(C.baml_handle_clone_go(C.uint64_t(key), &cloned))
	if err := nativeHandleStatus("clone BAML handle", status); err != nil {
		return 0, err
	}
	if cloned == 0 {
		return 0, fmt.Errorf("clone BAML handle: runtime returned a zero handle")
	}
	return uint64(cloned), nil
}

func nativeHandleRelease(key uint64) { _ = C.baml_handle_release_go(C.uint64_t(key)) }

func nativeMediaConstruct(operation mediaConstructor, kind cffi.MediaTypeEnum, value string, mimeType *string) (uint64, cffi.BamlHandleType, error) {
	cValue := C.CString(value)
	defer C.free(unsafe.Pointer(cValue))
	var cMimeType *C.char
	if mimeType != nil {
		cMimeType = C.CString(*mimeType)
		defer C.free(unsafe.Pointer(cMimeType))
	}
	var key C.uint64_t
	var handleType C.int32_t
	var status C.uint32_t
	switch operation {
	case mediaFromURL:
		status = C.baml_media_from_url_go(C.int32_t(kind), cValue, cMimeType, &key, &handleType)
	case mediaFromFile:
		status = C.baml_media_from_file_go(C.int32_t(kind), cValue, cMimeType, &key, &handleType)
	case mediaFromBase64:
		status = C.baml_media_from_base64_go(C.int32_t(kind), cValue, cMimeType, &key, &handleType)
	default:
		return 0, cffi.BamlHandleType_HANDLE_UNSPECIFIED, fmt.Errorf("unknown BAML media constructor %d", operation)
	}
	if err := nativeHandleStatus(operation.String(), uint32(status)); err != nil {
		return 0, cffi.BamlHandleType_HANDLE_UNSPECIFIED, err
	}
	return uint64(key), cffi.BamlHandleType(handleType), nil
}

func nativeMediaAccess(operation mediaAccessor, key uint64, handleType cffi.BamlHandleType) (*string, error) {
	var buffer C.BamlBuffer
	var status C.uint32_t
	switch operation {
	case mediaURL:
		status = C.baml_media_url_go(C.uint64_t(key), C.int32_t(handleType), &buffer)
	case mediaFile:
		status = C.baml_media_file_go(C.uint64_t(key), C.int32_t(handleType), &buffer)
	case mediaBase64:
		status = C.baml_media_base64_go(C.uint64_t(key), C.int32_t(handleType), &buffer)
	case mediaMIMEType:
		status = C.baml_media_mime_type_go(C.uint64_t(key), C.int32_t(handleType), &buffer)
	default:
		return nil, fmt.Errorf("unknown BAML media accessor %d", operation)
	}
	if err := nativeHandleStatus(operation.String(), uint32(status)); err != nil {
		return nil, err
	}
	defer C.baml_free_buffer(buffer)
	hasPointer := buffer.ptr != nil
	if err := validateNativeMediaBuffer(hasPointer, uint64(buffer.len)); err != nil {
		return nil, fmt.Errorf("%s: %w", operation, err)
	}
	if !hasPointer {
		return nil, nil
	}
	bytes := C.GoBytes(unsafe.Pointer(buffer.ptr), C.int(buffer.len))
	value := string(bytes)
	return &value, nil
}

func nativeRuntimeTarget() (string, error) {
	switch runtime.GOARCH {
	case "amd64":
		return "x86_64-pc-windows-msvc", nil
	case "arm64":
		return "aarch64-pc-windows-msvc", nil
	default:
		return "", fmt.Errorf("BAML has no released native runtime target for windows/%s", runtime.GOARCH)
	}
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

//export bamlGoUnhandledSpawnErrorCallback
func bamlGoUnhandledSpawnErrorCallback(content *C.int8_t, length C.size_t, cancelled C.int32_t) {
	payload := C.GoBytes(unsafe.Pointer(content), C.int(length))
	go reportUnhandledSpawnError(payload, cancelled != 0)
}

//export bamlGoHostDispatch
func bamlGoHostDispatch(hostValueKey C.uint64_t, callID C.uint32_t, content *C.uint8_t, length C.size_t) {
	if uint64(length) > uint64(^uint32(0)>>1) {
		go completeHostCallFailure(uint32(callID), fmt.Errorf("BAML host-call payload is too large: %d bytes", uint64(length)), "")
		return
	}
	payload := C.GoBytes(unsafe.Pointer(content), C.int(length))
	dispatchHostCall(uint64(hostValueKey), uint32(callID), payload)
}

//export bamlGoHostRelease
func bamlGoHostRelease(hostValueKey C.uint64_t) {
	unregisterHostValue(uint64(hostValueKey))
}

func nativeRegisterProgram(key uint64, bytes []byte, metadata string) error {
	var pointer *C.uint8_t
	if len(bytes) > 0 {
		pointer = (*C.uint8_t)(unsafe.Pointer(&bytes[0]))
	}
	var manifest *C.char
	if metadata != "" {
		manifest = C.CString(metadata)
		defer C.free(unsafe.Pointer(manifest))
	}
	status := C.baml_register_program(C.uint64_t(key), pointer, C.size_t(len(bytes)), manifest)
	defer C.baml_free_buffer(status)
	if status.len == 0 {
		return nil
	}
	return fmt.Errorf("%s", C.GoBytes(unsafe.Pointer(status.ptr), C.int(status.len)))
}
func nativeCallKeyed(key uint64, bytes []byte, callbackID uint32) {
	var pointer *C.uint8_t
	if len(bytes) > 0 {
		pointer = (*C.uint8_t)(unsafe.Pointer(&bytes[0]))
	}
	C.baml_call_keyed(C.uint64_t(key), pointer, C.size_t(len(bytes)), C.uint32_t(callbackID))
}

func nativeCreateRuntime(bytes []byte) (uint64, error) {
	var pointer *C.uint8_t
	if len(bytes) > 0 {
		pointer = (*C.uint8_t)(unsafe.Pointer(&bytes[0]))
	}
	var key C.uint64_t
	status := C.baml_create_runtime(pointer, C.size_t(len(bytes)), &key)
	defer C.baml_free_buffer(status)
	if status.len != 0 {
		return 0, fmt.Errorf("%s", C.GoBytes(unsafe.Pointer(status.ptr), C.int(status.len)))
	}
	return uint64(key), nil
}
func nativeUnregisterRuntime(key uint64) error {
	status := C.baml_unregister_runtime(C.uint64_t(key))
	defer C.baml_free_buffer(status)
	if status.len != 0 {
		return fmt.Errorf("%s", C.GoBytes(unsafe.Pointer(status.ptr), C.int(status.len)))
	}
	return nil
}

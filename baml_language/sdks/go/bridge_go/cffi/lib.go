package cffi

/*
#cgo LDFLAGS: -ldl
#include "bridge.h"
*/
import "C"
import (
	"fmt"
	"log"
	"os"
	"path/filepath"
	"runtime"
	"unsafe"
)

var libHandle unsafe.Pointer

// Init loads the bridge_go shared library and resolves all symbols.
// Call this from TestMain or package init.
func Init(libraryPath string) error {
	cPath := C.CString(libraryPath)
	defer C.free(unsafe.Pointer(cPath))

	handle := C.dlopen(cPath, C.RTLD_LAZY|C.RTLD_LOCAL)
	if handle == nil {
		errStr := C.GoString(C.dlerror())
		return fmt.Errorf("dlopen(%s): %s", libraryPath, errStr)
	}
	symbols := map[string]func(unsafe.Pointer){
		"version":              func(p unsafe.Pointer) { C.setVersionFn(p) },
		"create_baml_runtime":  func(p unsafe.Pointer) { C.setCreateBamlRuntimeFn(p) },
		"destroy_baml_runtime": func(p unsafe.Pointer) { C.setDestroyBamlRuntimeFn(p) },
		"free_buffer":          func(p unsafe.Pointer) { C.setFreeBufferFn(p) },
		"call_function":        func(p unsafe.Pointer) { C.setCallFunctionFn(p) },
		"register_callback":   func(p unsafe.Pointer) { C.setRegisterCallbackFn(p) },
		"cancel_function_call": func(p unsafe.Pointer) { C.setCancelFunctionCallFn(p) },
		"clone_handle":         func(p unsafe.Pointer) { C.setCloneHandleFn(p) },
		"release_handle":       func(p unsafe.Pointer) { C.setReleaseHandleFn(p) },
		"flush_events":         func(p unsafe.Pointer) { C.setFlushEventsFn(p) },
	}
	for name, setter := range symbols {
		sym, err := resolveSymbol(handle, name)
		if err != nil {
			C.dlclose(handle)
			return fmt.Errorf("resolving %s: %w", name, err)
		}
		setter(sym)
	}
	libHandle = handle
	return nil
}

func resolveSymbol(handle unsafe.Pointer, name string) (unsafe.Pointer, error) {
	cName := C.CString(name)
	defer C.free(unsafe.Pointer(cName))
	C.dlerror() // clear
	sym := C.dlsym(handle, cName)
	if sym == nil {
		errStr := C.GoString(C.dlerror())
		return nil, fmt.Errorf("dlsym: %s", errStr)
	}
	return sym, nil
}

// FindLibrary locates the bridge_go shared library relative to the source tree.
func FindLibrary() (string, error) {
	// Walk up from this source file to find the cargo target directory
	_, thisFile, _, _ := runtime.Caller(0)
	crateDir := filepath.Dir(filepath.Dir(thisFile)) // cffi/ -> bridge_go/
	// Try common cargo output locations
	var libFile string
	switch runtime.GOOS {
	case "darwin":
		libFile = "libbridge_cffi.dylib"
	case "windows":
		libFile = "bridge_cffi.dll"
	default:
		libFile = "libbridge_cffi.so"
	}
	candidates := []string{
		filepath.Join(crateDir, "..", "..", "target", "debug", libFile),
		filepath.Join(crateDir, "..", "..", "target", "release", libFile),
	}
	for _, p := range candidates {
		abs, _ := filepath.Abs(p)
		if _, err := os.Stat(abs); err == nil {
			return abs, nil
		}
	}
	return "", fmt.Errorf("bridge_cffi shared library not found; tried: %v", candidates)
}

// Version returns the BAML engine version string.
func Version() string {
	buf := C.wrapVersion()
	if buf.ptr == nil || buf.len == 0 {
		return ""
	}
	s := C.GoStringN((*C.char)(unsafe.Pointer(buf.ptr)), C.int(buf.len))
	C.wrapFreeBuffer(buf.ptr, buf.len)
	return s
}

// CreateBamlRuntime initializes the global BAML runtime from source files.
// srcFilesJSON is a JSON-encoded map[string]string of filename -> content.
// Returns an opaque runtime pointer (sentinel, not a real pointer).
func CreateBamlRuntime(rootPath, srcFilesJSON string) (unsafe.Pointer, error) {
	cRootPath := C.CString(rootPath)
	defer C.free(unsafe.Pointer(cRootPath))
	cSrcFiles := C.CString(srcFilesJSON)
	defer C.free(unsafe.Pointer(cSrcFiles))

	ptr := C.wrapCreateBamlRuntime(cRootPath, cSrcFiles)
	if ptr == nil {
		// The Rust side logs the actual error to stderr via ffi_safe_ptr but
		// only returns a null pointer — there is no mechanism to retrieve
		// the error message. This matches engine/language_client_go behavior.
		return nil, fmt.Errorf("create_baml_runtime failed (returned null)")
	}
	return ptr, nil
}

// DestroyBamlRuntime is a no-op -- the runtime is global.
func DestroyBamlRuntime(ptr unsafe.Pointer) {
	C.wrapDestroyBamlRuntime(ptr)
}

// RegisterCallback registers a single Go callback function pointer with Rust.
// cb must be a C-callable function pointer (//export function cast to unsafe.Pointer).
func RegisterCallback(cb unsafe.Pointer) {
	C.wrapRegisterCallback((C.CallbackFn)(cb))
}

// CallFunction dispatches an async function call to Rust.
// Results and errors are delivered via the registered callback.
func CallFunction(functionName string, encodedArgs []byte, id uint32) {
	cName := C.CString(functionName)
	defer C.free(unsafe.Pointer(cName))

	var cArgs *C.uint8_t
	if len(encodedArgs) > 0 {
		cArgs = (*C.uint8_t)(unsafe.Pointer(&encodedArgs[0]))
	}

	C.wrapCallFunction((*C.char)(unsafe.Pointer(cName)), cArgs, C.size_t(len(encodedArgs)), C.uint32_t(id))
}

// CancelFunctionCall deliberately does not return an error. It's a bridge_go
// internal detail; when users interact with the Baml<->Go bridge, they cancel
// function calls using ctx.Cancel(), which we translate into a CancelFunctionCall
// invocation. If CancelFunctionCall were to return an error, it would have to be
// returned through ctx.Cancel(), but since there's no way to return an error
// through this path, there's no reason for CancelFunctionCall to return an error.
func CancelFunctionCall(id uint32) {
	if rc := C.wrapCancelFunctionCall(C.uint32_t(id)); rc != 0 {
		log.Printf("bridge_go: cancel_function_call failed for id=%d (rc=%d)", id, rc)
	}
}

// CloneHandle clones an opaque handle, returning a new key.
func CloneHandle(key uint64) uint64 {
	return uint64(C.wrapCloneHandle(C.uint64_t(key)))
}

// ReleaseHandle releases an opaque handle.
func ReleaseHandle(key uint64) {
	C.wrapReleaseHandle(C.uint64_t(key))
}

// FlushEvents flushes the event sink.
func FlushEvents() {
	C.wrapFlushEvents()
}

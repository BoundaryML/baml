//go:build darwin || linux

package baml_go

/*
#cgo CFLAGS: -I${SRCDIR}
#cgo CFLAGS: -O3 -g
#cgo LDFLAGS: -ldl
#include <dlfcn.h>
#include <baml_cffi_wrapper.h>
#include <stdlib.h>
#include <string.h>
#include <stdint.h>
*/
import "C"
import (
	"fmt"
	"strings"
	"unsafe"
)

// loadLibrary loads the shared library using dlopen
func loadLibrary(path string) (unsafe.Pointer, error) {
	cPath := C.CString(path)
	defer C.free(unsafe.Pointer(cPath))

	handle := C.dlopen(cPath, C.RTLD_LAZY|C.RTLD_LOCAL)
	if handle == nil {
		dlErrStr := C.GoString(C.dlerror())
		dlopenErr := fmt.Errorf("dlopen error for %s: %s", path, dlErrStr)
		if strings.Contains(dlErrStr, "mach-o, but wrong architecture") || strings.Contains(dlErrStr, "wrong ELF class") {
			dlopenErr = fmt.Errorf("%w (architecture mismatch)", dlopenErr)
		} else if strings.Contains(dlErrStr, "cannot open shared object file") {
			if strings.Contains(dlErrStr, "Permission denied") {
				dlopenErr = fmt.Errorf("%w (permission denied)", dlopenErr)
			} else {
				dlopenErr = fmt.Errorf("%w (file not found or inaccessible)", dlopenErr)
			}
		} else if strings.Contains(dlErrStr, "image not found") || strings.Contains(dlErrStr, "no such file or directory") {
			dlopenErr = fmt.Errorf("%w (library or dependency not found)", dlopenErr)
		}
		return nil, dlopenErr
	}
	return handle, nil
}

// getSymbol retrieves a symbol from the loaded library
func getSymbol(handle unsafe.Pointer, name string) (unsafe.Pointer, error) {
	if handle == nil {
		return nil, fmt.Errorf("library handle is nil when looking up symbol '%s'", name)
	}

	cName := C.CString(name)
	defer C.free(unsafe.Pointer(cName))

	C.dlerror() // Clear any existing error
	symbol := C.dlsym(handle, cName)
	errStr := C.GoString(C.dlerror())

	if symbol == nil {
		errMsg := fmt.Sprintf("dlsym error for %s", name)
		if errStr != "" {
			errMsg += fmt.Sprintf(": %s", errStr)
		} else {
			errMsg += ": symbol not found"
		}
		return nil, fmt.Errorf("%s", errMsg)
	}
	return symbol, nil
}

// closeLibrary closes the loaded library
func closeLibrary(handle unsafe.Pointer) error {
	if handle == nil {
		return nil
	}
	if C.dlclose(handle) != 0 {
		errStr := C.GoString(C.dlerror())
		if errStr != "" {
			return fmt.Errorf("dlclose failed: %s", errStr)
		}
		return fmt.Errorf("dlclose failed")
	}
	return nil
}

// platformInit performs any platform-specific initialization
func platformInit() error {
	// Unix doesn't need special initialization
	return nil
}
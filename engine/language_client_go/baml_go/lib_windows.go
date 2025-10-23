//go:build windows

package baml_go

/*
#cgo CFLAGS: -I${SRCDIR}
#cgo CFLAGS: -O3 -g
#include <baml_cffi_wrapper.h>
#include <stdlib.h>
#include <string.h>
#include <stdint.h>
*/
import "C"
import (
	"fmt"
	"syscall"
	"unsafe"
)

var (
	kernel32 = syscall.NewLazyDLL("kernel32.dll")

	procLoadLibraryW   = kernel32.NewProc("LoadLibraryW")
	procGetProcAddress = kernel32.NewProc("GetProcAddress")
	procFreeLibrary    = kernel32.NewProc("FreeLibrary")
	procGetLastError   = kernel32.NewProc("GetLastError")
)

// loadLibrary loads the shared library using LoadLibrary
func loadLibrary(path string) (unsafe.Pointer, error) {
	pathPtr, err := syscall.UTF16PtrFromString(path)
	if err != nil {
		return nil, fmt.Errorf("invalid library path: %w", err)
	}

	handle, _, err := procLoadLibraryW.Call(uintptr(unsafe.Pointer(pathPtr)))
	if handle == 0 {
		lastErr, _, _ := procGetLastError.Call()
		return nil, fmt.Errorf("LoadLibrary failed for %s: error code %d: %w", path, lastErr, err)
	}

	return unsafe.Pointer(handle), nil
}

// getSymbol retrieves a symbol from the loaded library
func getSymbol(handle unsafe.Pointer, name string) (unsafe.Pointer, error) {
	namePtr, err := syscall.BytePtrFromString(name)
	if err != nil {
		return nil, fmt.Errorf("invalid symbol name: %w", err)
	}

	proc, _, err := procGetProcAddress.Call(
		uintptr(handle),
		uintptr(unsafe.Pointer(namePtr)),
	)

	if proc == 0 {
		lastErr, _, _ := procGetLastError.Call()
		return nil, fmt.Errorf("GetProcAddress failed for %s: error code %d: %w", name, lastErr, err)
	}

	return unsafe.Pointer(proc), nil
}

// closeLibrary closes the loaded library
func closeLibrary(handle unsafe.Pointer) error {
	if handle == nil {
		return nil
	}
	ret, _, err := procFreeLibrary.Call(uintptr(handle))
	if ret == 0 {
		return fmt.Errorf("FreeLibrary failed: %w", err)
	}
	return nil
}

// platformInit performs any platform-specific initialization
func platformInit() error {
	// Windows doesn't need special initialization
	return nil
}
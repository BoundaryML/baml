package pkg

/*
#include <stdint.h>

// Forward-declare the exported Go function so we can take its address.
// Defined (via //export) in callbacks.go.
extern void baml_callback(uint32_t id, int32_t is_error, const int8_t *content, size_t length);
*/
import "C"
import (
	"unsafe"

	"bridge_go/cffi"
)

// InitCallbacks registers the Go callback function pointer with Rust.
// Must be called after cffi.Init and before any function calls.
func InitCallbacks() {
	cffi.RegisterCallback(unsafe.Pointer(C.baml_callback))
}

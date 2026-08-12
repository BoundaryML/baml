package pkg

/*
#include <stdint.h>
#include <stdlib.h>

// Forward-declare the exported Go functions so we can take their addresses.
// `baml_callback` is defined (via //export) in callbacks.go. `bamlHostDispatch`
// and `bamlHostRelease` are defined (via //export) in host_value.go.
extern void baml_callback(uint32_t id, const int8_t *content, size_t length);
extern void bamlHostDispatch(uint64_t host_value_key, uint32_t call_id, const uint8_t *args, size_t length);
extern void bamlHostRelease(uint64_t host_value_key);
*/
import "C"
import (
	"unsafe"

	"bridge_go/cffi"
)

// InitCallbacks registers the Go callback function pointers with Rust. Must
// be called after `cffi.Init` and before any function calls.
//
// Registers three callbacks:
//   - baml_callback: completion for `call_function` (existing).
//   - bamlHostDispatch: BAML invokes a host-value callable.
//   - bamlHostRelease: last Rust clone of a host-value `Arc` dropped.
func InitCallbacks() {
	cffi.RegisterCallback(unsafe.Pointer(C.baml_callback))
	cffi.RegisterHostDispatchCallback(unsafe.Pointer(C.bamlHostDispatch))
	cffi.RegisterHostReleaseCallback(unsafe.Pointer(C.bamlHostRelease))
}

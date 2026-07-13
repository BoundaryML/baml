package baml_go

/*
#include <stdint.h>
#include <stddef.h>
*/
import "C"

import "unsafe"

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

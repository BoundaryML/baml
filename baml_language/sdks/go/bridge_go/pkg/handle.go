package pkg

import "bridge_go/cffi"

// BamlHandle wraps an opaque handle key from the Rust HANDLE_TABLE.
type BamlHandle struct {
	Key        uint64
	HandleType int32
}

// Validate checks that the key exists and that the stored type matches
// HandleType unless HandleType is HANDLE_UNSPECIFIED.
func (h BamlHandle) Validate() error {
	return cffi.HandleValidate(h.Key, h.HandleType)
}

// Clone creates a new handle pointing to the same underlying value.
func (h BamlHandle) Clone() (BamlHandle, error) {
	newKey, handleType, err := cffi.HandleClone(h.Key, h.HandleType)
	if err != nil {
		return BamlHandle{}, err
	}
	return BamlHandle{Key: newKey, HandleType: handleType}, nil
}

// Release releases the handle, allowing Rust to free the underlying value.
func (h BamlHandle) Release() error {
	return cffi.HandleRelease(h.Key, h.HandleType)
}

// FlushEvents flushes the BAML event sink.
func FlushEvents() {
	cffi.FlushEvents()
}

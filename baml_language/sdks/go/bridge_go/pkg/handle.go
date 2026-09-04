package pkg

import "bridge_go/cffi"

// BamlHandle wraps an opaque handle key from the Rust HANDLE_TABLE.
type BamlHandle struct {
	Key        uint64
	HandleType int32
}

// Clone creates a new handle pointing to the same underlying value.
func (h BamlHandle) Clone() (BamlHandle, error) {
	newKey, err := cffi.CloneHandle(h.Key)
	if err != nil {
		return BamlHandle{}, err
	}
	return BamlHandle{Key: newKey, HandleType: h.HandleType}, nil
}

// Release releases the handle, allowing Rust to free the underlying value.
func (h BamlHandle) Release() error {
	return cffi.ReleaseHandle(h.Key)
}

// FlushEvents flushes the BAML event sink.
func FlushEvents() {
	cffi.FlushEvents()
}

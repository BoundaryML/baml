package pkg

import "bridge_go/cffi"

// BamlHandle wraps an opaque handle key from the Rust HANDLE_TABLE.
type BamlHandle struct {
	Key        uint64
	HandleType int32
}

// Host-value handle types: the engine's references to this host's own
// objects (registered callables and opaque values). They are numbered by the
// host registry, not the engine's handle table, and own nothing there.
const (
	handleTypeHostValueCallable int32 = 15
	handleTypeHostValueOpaque   int32 = 16
)

// IsHostValue reports whether this handle is the engine's reference to a host
// object, as opposed to this host's reference to an engine object.
func (h BamlHandle) IsHostValue() bool {
	return h.HandleType == handleTypeHostValueCallable || h.HandleType == handleTypeHostValueOpaque
}

// Clone creates a new handle pointing to the same underlying value. A host
// value's key goes as it is: nothing is cloned in the engine's table.
func (h BamlHandle) Clone() (BamlHandle, error) {
	if h.IsHostValue() {
		return h, nil
	}
	newKey, err := cffi.CloneHandle(h.Key)
	if err != nil {
		return BamlHandle{}, err
	}
	return BamlHandle{Key: newKey, HandleType: h.HandleType}, nil
}

// Release releases the handle, allowing Rust to free the underlying value. A
// host value is released through the host registry, never here.
func (h BamlHandle) Release() error {
	if h.IsHostValue() {
		return nil
	}
	return cffi.ReleaseHandle(h.Key)
}

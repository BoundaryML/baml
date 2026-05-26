package tests

import (
	"testing"

	"bridge_go/pkg"
)

func TestCloneAndReleaseHandle(t *testing.T) {
	// Call a function that returns a value containing a handle.
	// For basic types, there's no handle to clone, so we test the
	// clone_handle/release_handle paths with a synthetic key.
	// Key 0 means "invalid" — clone should return 0.
	h := pkg.BamlHandle{Key: 0, HandleType: 0}
	cloned := h.Clone()
	if cloned.Key != 0 {
		t.Fatalf("expected clone of invalid handle to return 0, got %d", cloned.Key)
	}

	// Release should not panic even for invalid keys.
	h.Release()
	cloned.Release()
}

func TestFlushEvents(t *testing.T) {
	// Should not panic even without a runtime or event sink.
	pkg.FlushEvents()
}

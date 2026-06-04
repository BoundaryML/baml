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
	if err := h.Validate(); err == nil {
		t.Fatal("expected invalid handle validate to fail")
	}

	if _, err := h.Clone(); err == nil {
		t.Fatal("expected invalid handle clone to fail")
	}

	if err := h.Release(); err == nil {
		t.Fatal("expected invalid handle release to fail")
	}
}

func TestFlushEvents(t *testing.T) {
	// Should not panic even without a runtime or event sink.
	pkg.FlushEvents()
}

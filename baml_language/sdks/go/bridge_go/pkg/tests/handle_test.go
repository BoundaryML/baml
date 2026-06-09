package tests

import (
	"testing"

	"bridge_go/cffi"
	"bridge_go/pkg"
)

func TestCloneAndReleaseHandle(t *testing.T) {
	key, handleType, err := cffi.TestonlySeedFunctionRef(42)
	if err != nil {
		t.Fatalf("seed function ref: %v", err)
	}

	h := pkg.BamlHandle{Key: key, HandleType: handleType}
	cloned, err := h.Clone()
	if err != nil {
		t.Fatalf("clone handle: %v", err)
	}
	if cloned.Key == 0 || cloned.Key == h.Key {
		t.Fatalf("expected clone to produce a fresh non-zero key, got %d from %d", cloned.Key, h.Key)
	}

	if err := h.Release(); err != nil {
		t.Fatalf("release original: %v", err)
	}
	if err := cloned.Release(); err != nil {
		t.Fatalf("release clone: %v", err)
	}
}

func TestInvalidHandleErrors(t *testing.T) {
	h := pkg.BamlHandle{Key: 0, HandleType: 0}
	if _, err := h.Clone(); err == nil {
		t.Fatal("expected clone of invalid handle to return an error")
	}
	if err := h.Release(); err == nil {
		t.Fatal("expected release of invalid handle to return an error")
	}
}

func TestFlushEvents(t *testing.T) {
	// Should not panic even without a runtime or event sink.
	pkg.FlushEvents()
}

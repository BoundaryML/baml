package baml_go

import (
	"bytes"
	"errors"
	"os"
	"path/filepath"
	"runtime"
	"testing"
)

func TestVendoredCFFIHeaderMatchesCanonicalHeader(t *testing.T) {
	t.Helper()
	_, filename, _, ok := runtime.Caller(0)
	if !ok {
		t.Fatal("locate ABI header test source")
	}

	packageDir := filepath.Dir(filename)
	canonicalPath := filepath.Clean(filepath.Join(
		packageDir,
		"..", "..", "..", "crates", "bridge_cffi", "include", "baml_cffi.h",
	))
	canonical, err := os.ReadFile(canonicalPath)
	if errors.Is(err, os.ErrNotExist) {
		t.Skip("canonical CFFI header is unavailable outside the BAML repository")
	}
	if err != nil {
		t.Fatalf("read canonical CFFI header: %v", err)
	}

	vendoredPath := filepath.Join(packageDir, "internal", "cffi", "include", "baml_cffi.h")
	vendored, err := os.ReadFile(vendoredPath)
	if err != nil {
		t.Fatalf("read vendored CFFI header: %v", err)
	}
	if !bytes.Equal(vendored, canonical) {
		t.Fatalf(
			"vendored CFFI header differs from %s; copy the regenerated canonical header into %s",
			canonicalPath,
			vendoredPath,
		)
	}
}

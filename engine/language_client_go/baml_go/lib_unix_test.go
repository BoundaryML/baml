//go:build darwin || linux

package baml_go

import (
	"runtime"
	"strings"
	"testing"
)

func TestUnixPlatformSupport(t *testing.T) {
	if !isSupportedPlatform() {
		t.Errorf("Unix platform should be supported, got GOOS=%s GOARCH=%s",
			runtime.GOOS, runtime.GOARCH)
	}
}

func TestUnixLibraryNaming(t *testing.T) {
	filename, err := getTargetLibFilename()
	if err != nil {
		t.Fatalf("Failed to get target filename: %v", err)
	}

	// Verify "lib" prefix on Unix
	if !strings.HasPrefix(filename, "lib") {
		t.Errorf("Unix library should have 'lib' prefix, got %s", filename)
	}

	// Check extension based on OS
	switch runtime.GOOS {
	case "darwin":
		if !strings.HasSuffix(filename, ".dylib") {
			t.Errorf("macOS library should have .dylib extension, got %s", filename)
		}
	case "linux":
		if !strings.HasSuffix(filename, ".so") {
			t.Errorf("Linux library should have .so extension, got %s", filename)
		}
	}
}
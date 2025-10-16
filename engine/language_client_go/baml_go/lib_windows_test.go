//go:build windows

package baml_go

import (
	"runtime"
	"strings"
	"testing"
)

func TestWindowsPlatformSupport(t *testing.T) {
	if !isSupportedPlatform() {
		t.Errorf("Windows platform should be supported, got GOOS=%s GOARCH=%s",
			runtime.GOOS, runtime.GOARCH)
	}
}

func TestWindowsDLLNaming(t *testing.T) {
	filename, err := getTargetLibFilename()
	if err != nil {
		t.Fatalf("Failed to get target filename: %v", err)
	}

	expected := map[string]string{
		"amd64": "baml_cffi-x86_64-pc-windows-msvc.dll",
		"arm64": "baml_cffi-aarch64-pc-windows-msvc.dll",
	}

	if want, ok := expected[runtime.GOARCH]; ok {
		if filename != want {
			t.Errorf("Expected filename %s, got %s", want, filename)
		}
	} else {
		t.Errorf("Unexpected architecture: %s", runtime.GOARCH)
	}

	// Verify no "lib" prefix on Windows
	if strings.HasPrefix(filename, "lib") {
		t.Errorf("Windows DLL should not have 'lib' prefix, got %s", filename)
	}
}

func TestWindowsLibraryLoading(t *testing.T) {
	// This will test the actual loading in CI where DLL is available
	// It's expected to fail locally if DLL is not available
	err := GetInitError()
	if err != nil {
		// Check if it's a known error about missing library
		if strings.Contains(err.Error(), "could not find BAML library") ||
		   strings.Contains(err.Error(), "LoadLibrary failed") {
			t.Logf("Library loading failed (expected if DLL not available): %v", err)
		} else {
			// Unexpected error type
			t.Errorf("Unexpected initialization error: %v", err)
		}
	} else {
		// If no error, verify the library handle was set
		if bamlLibHandle == nil {
			t.Error("No initialization error but library handle is nil")
		}
		t.Log("BAML library loaded successfully on Windows")
	}
}

func TestWindowsCacheDirectory(t *testing.T) {
	cacheDir, err := getCacheDir()
	if err != nil {
		t.Fatalf("Failed to get cache directory: %v", err)
	}

	// On Windows, cache should be in LOCALAPPDATA
	if !strings.Contains(cacheDir, "AppData\\Local") && !strings.Contains(cacheDir, "AppData/Local") {
		t.Errorf("Expected cache directory in AppData/Local, got %s", cacheDir)
	}

	if !strings.Contains(cacheDir, "baml") {
		t.Errorf("Expected cache directory to contain 'baml', got %s", cacheDir)
	}
}
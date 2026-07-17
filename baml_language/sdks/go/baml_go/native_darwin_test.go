//go:build darwin && cgo

package baml_go

import (
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"
)

func TestNativeOpenRejectsMissingFunctionTable(t *testing.T) {
	library := compileTestDylib(t, `
int unrelated_symbol(void) { return 1; }
`)
	if _, err := nativeOpen(library); err == nil || !strings.Contains(err.Error(), "baml_get_api_v1") {
		t.Fatalf("got error %v, want missing baml_get_api_v1", err)
	}
}

func TestNativeOpenRejectsWrongABIVersion(t *testing.T) {
	library := compileTestDylib(t, `
#include <stdint.h>
#include <stddef.h>

typedef struct {
    uint32_t abi_version;
    size_t struct_size;
} BadBamlApi;

static const BadBamlApi bad_api = { 99, sizeof(BadBamlApi) };

__attribute__((visibility("default")))
const BadBamlApi *baml_get_api_v1(void) { return &bad_api; }
`)
	if _, err := nativeOpen(library); err == nil || !strings.Contains(err.Error(), "unsupported BAML ABI version 99") {
		t.Fatalf("got error %v, want ABI version rejection", err)
	}
}

func TestNativeOpenRejectsTruncatedFunctionTable(t *testing.T) {
	library := compileTestDylib(t, `
#include <stdint.h>
#include <stddef.h>

typedef struct {
    uint32_t abi_version;
    size_t struct_size;
} TruncatedBamlApi;

static const TruncatedBamlApi truncated_api = { 1, sizeof(TruncatedBamlApi) };

__attribute__((visibility("default")))
const TruncatedBamlApi *baml_get_api_v1(void) { return &truncated_api; }
`)
	if _, err := nativeOpen(library); err == nil || !strings.Contains(err.Error(), "truncated BAML ABI v1 table") {
		t.Fatalf("got error %v, want truncated table rejection", err)
	}
}

func compileTestDylib(t *testing.T, source string) string {
	t.Helper()
	directory := t.TempDir()
	sourcePath := filepath.Join(directory, "fixture.c")
	libraryPath := filepath.Join(directory, "fixture.dylib")
	if err := os.WriteFile(sourcePath, []byte(source), 0o600); err != nil {
		t.Fatal(err)
	}
	compiler := os.Getenv("CC")
	if compiler == "" {
		compiler = "cc"
	}
	output, err := exec.Command(compiler, "-dynamiclib", "-o", libraryPath, sourcePath).CombinedOutput()
	if err != nil {
		t.Fatalf("compile test dylib: %v\n%s", err, output)
	}
	return libraryPath
}

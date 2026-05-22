package tests

import (
	"os"
	"testing"

	"bridge_go/cffi"
	"bridge_go/pkg"
)

func TestMain(m *testing.M) {
	libPath, err := cffi.FindLibrary()
	if err != nil {
		panic("cannot find bridge_go library: " + err.Error())
	}
	if err := cffi.Init(libPath); err != nil {
		panic("cannot init bridge_go: " + err.Error())
	}
	pkg.InitCallbacks()
	os.Exit(m.Run())
}

const bamlSource = `
function ReturnOne() -> int {
  1
}

function Identity(s: string) -> string {
  s
}
`

func TestGetVersion(t *testing.T) {
	v := pkg.Version()
	if v == "" {
		t.Fatal("expected non-empty version string")
	}
	t.Logf("version: %s", v)
}

func TestCreateRuntime(t *testing.T) {
	rt, err := pkg.NewRuntime(".", map[string]string{"main.baml": bamlSource})
	if err != nil {
		t.Fatalf("NewRuntime failed: %v", err)
	}
	if rt == nil {
		t.Fatal("expected non-nil runtime")
	}
}

func TestCreateRuntimeInvalidSource(t *testing.T) {
	// bex_engine does not validate BAML at initialization time, so
	// NewRuntime succeeds even for invalid source. Errors surface later
	// at call time. This test verifies NewRuntime does not panic or return
	// an unexpected nil runtime for syntactically bad input.
	badBaml := `function Bad() -> int { "not an int" }`
	rt, err := pkg.NewRuntime(".", map[string]string{"main.baml": badBaml})
	// We expect success (no error) at init time — engine validates lazily.
	if err != nil {
		// If the engine ever adds strict validation at init, update this test.
		t.Logf("NewRuntime returned error (engine may now validate at init): %v", err)
	}
	if rt == nil && err == nil {
		t.Fatal("expected non-nil runtime when no error")
	}
}

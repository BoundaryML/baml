package sdk_test

import (
	"os"
	"strings"
	"testing"

	"baml.local/sdk/docs_sdk"
	"baml.local/sdk/docs_sdk/reflect"
)

// The harness also generates this fixture with `package_name = "docs_sdk"`
// and `sdk_import_path = "baml.local/sdk/docs_sdk"`. Compiling this file
// proves the renamed root package and its subpackages build and import one
// another under the configured name.
//
// SDK_PARITY_LINT(skip): Go spells the package name in generated code
func Test_go_package_name_names_the_root_package(t *testing.T) {
	_ = docs_sdk.DocsDoc{}
	_ = docs_sdk.DocsPriorityHIGH
	_ = docs_sdk.BamlOptions{}
	_ = reflect.WithAlias

	contents, err := os.ReadFile("docs_sdk/invocation.go")
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(string(contents), "\npackage docs_sdk\n") {
		t.Error("invocation.go does not declare the configured package")
	}
	if strings.Contains(string(contents), "baml_sdk") {
		t.Error("the renamed SDK still names baml_sdk")
	}
}

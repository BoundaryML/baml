package sdk_test

import (
	"os"
	"strings"
	"testing"
)

func Test_go_codegen_function_doc_comment(t *testing.T) {
	contents, err := os.ReadFile("baml_sdk/functions.go")
	if err != nil {
		t.Fatal(err)
	}
	wants := []string{
		"// ChainChainInner Innermost step — throws, so the error unwinds up through middle + outer.",
		"// BAML failures are returned as Go errors containing the current runtime trace text; structured BAML error values are not exposed yet.\nfunc ChainChainInner",
	}
	for _, want := range wants {
		if !strings.Contains(string(contents), want) {
			t.Errorf("missing attached function doc comment %q", want)
		}
	}
}

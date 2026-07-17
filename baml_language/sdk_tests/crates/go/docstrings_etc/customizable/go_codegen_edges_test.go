package sdk_test

import (
	"os"
	"strings"
	"testing"
)

func TestGoCodegenFunctionDocComment(t *testing.T) {
	contents, err := os.ReadFile("baml_sdk/functions.go")
	if err != nil {
		t.Fatal(err)
	}
	want := "// ChainChainInner Innermost step — throws, so the error unwinds up through middle + outer.\nfunc ChainChainInner"
	if !strings.Contains(string(contents), want) {
		t.Errorf("missing attached function doc comment %q", want)
	}
}

package sdk_test

import (
	"os"
	"strings"
	"testing"

	"baml.local/sdk/baml_sdk"
)

func Test_imports(t *testing.T) {
	_ = baml_sdk.DocsDoc{}
	_ = baml_sdk.DocsNote{}
	_ = baml_sdk.DocsPriorityHIGH
	_ = baml_sdk.DocsSentimentHAPPY
}

// Go exposes documentation as source comments rather than runtime metadata.
func Test_class_doc_summary_and_attributes(t *testing.T) {
	source := readGeneratedTypes(t)
	for _, want := range []string{
		"// DocsDoc A document with a title and an optional body.",
		"// Title Title shown in lists and search results.",
		"// Body Free-form body text.",
	} {
		if !strings.Contains(source, want) {
			t.Errorf("missing %q", want)
		}
	}
}

func Test_undocumented_field_has_no_doc_artifact(t *testing.T) {
	source := readGeneratedTypes(t)
	for _, want := range []string{
		"// DocsNote A multi-line summary.",
		"// Continuation line of the summary, preserved verbatim in the",
		"// rendered block-form docstring.",
		"// Id Stable identifier — surfaces in URLs.",
	} {
		if !strings.Contains(source, want) {
			t.Errorf("missing %q", want)
		}
	}
	if strings.Contains(source, "// Text ") {
		t.Error("undocumented field has a doc comment")
	}
}

func Test_enum_doc_summary_and_members(t *testing.T) {
	source := readGeneratedTypes(t)
	for _, want := range []string{
		"// DocsSentiment Sentiment labels surfaced by the model.",
		"// DocsSentimentHAPPY Smiling face.",
		"// DocsSentimentSAD Frowning face.",
	} {
		if !strings.Contains(source, want) {
			t.Errorf("missing %q", want)
		}
	}
	if strings.Contains(source, "// DocsSentimentNEUTRAL ") {
		t.Error("undocumented variant has a doc comment")
	}
}

func Test_enum_summary_only_omits_member_comments(t *testing.T) {
	source := readGeneratedTypes(t)
	compactSource := strings.NewReplacer(" ", "", "\t", "").Replace(source)
	if !strings.Contains(source, "// DocsPriority Pin the \"summary only, no member rollup\" case: this enum has a") {
		t.Error("priority summary missing")
	}
	for _, variant := range []string{"HIGH", "MEDIUM", "LOW"} {
		if strings.Contains(source, "// DocsPriority"+variant+" ") {
			t.Errorf("%s unexpectedly documented", variant)
		}
		if !strings.Contains(compactSource, "DocsPriority"+variant+"DocsPriority=\""+variant+"\"") {
			t.Errorf("%s missing", variant)
		}
	}
}

func Test_no_inline_field_or_variant_doc_artifacts(t *testing.T) {
	source := readGeneratedTypes(t)
	if strings.Contains(source, "// Title shown in lists") {
		t.Error("field doc lost its generated field identifier")
	}
	if strings.Contains(source, "// Smiling face") {
		t.Error("variant doc lost its generated variant identifier")
	}
}

func readGeneratedTypes(t *testing.T) string {
	t.Helper()
	contents, err := os.ReadFile("baml_sdk/types.go")
	if err != nil {
		t.Fatal(err)
	}
	return string(contents)
}

package baml_go

import "testing"

func TestCanonicalGoModuleVersion(t *testing.T) {
	for input, expected := range map[string]string{
		"v0.14.1":                        "0.14.1",
		"v0.14.2-nightly.20260715.a":     "0.14.2-nightly.20260715.a",
		"":                               "",
		"(devel)":                        "",
		"v0.0.0-20260715000000-deadbeef": "",
	} {
		if actual := canonicalGoModuleVersion(input); actual != expected {
			t.Errorf("canonicalGoModuleVersion(%q) = %q, want %q", input, actual, expected)
		}
	}
}

package sdk_test

import (
	"baml.local/sdk/baml_sdk"
	"testing"
)

func Test_root_imports_cleanly(t *testing.T) { _ = baml_sdk.Foo{} }

func Test_supported_namespaces_reachable(t *testing.T) {
	_ = baml_sdk.PrimitivesPrimitives{}
	_ = baml_sdk.EnumsEnums{}
	_ = baml_sdk.LiteralsLiterals{}
	_ = baml_sdk.ClassRefsOuter{}
	_ = baml_sdk.AliasesStringList{}
	_ = baml_sdk.OptionalResume{}
	_ = baml_sdk.MapsResume{}
	_ = baml_sdk.RecursionA{}
	_ = baml_sdk.ForwardRefsOther{}
	_ = baml_sdk.GenericsWrapper[int64]{}
	_ = baml_sdk.ComplexModelsComplexProfile{}
	// Recursive aliases and host-created opaque handles remain deferred.
}

func Test_root_foo_reachable(t *testing.T)             { _ = baml_sdk.Foo{} }
func Test_lorem_resume_reachable(t *testing.T)         { _ = baml_sdk.LoremResume{} }
func Test_deep_namespace_thing_reachable(t *testing.T) { _ = baml_sdk.ABThing{} }

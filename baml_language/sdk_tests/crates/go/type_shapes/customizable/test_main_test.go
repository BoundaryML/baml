package sdk_test

import (
	"baml.local/sdk/baml_sdk"
	"testing"
)

func TestRootImportsCleanly(t *testing.T) { _ = baml_sdk.Foo{} }

func TestSupportedNamespacesReachable(t *testing.T) {
	_ = baml_sdk.PrimitivesPrimitives{}
	_ = baml_sdk.EnumsEnums{}
	_ = baml_sdk.LiteralsLiterals{}
	_ = baml_sdk.ClassRefsOuter{}
	_ = baml_sdk.AliasesStringList{}
	_ = baml_sdk.OptionalResume{}
	_ = baml_sdk.MapsResume{}
	_ = baml_sdk.RecursionA{}
	_ = baml_sdk.ForwardRefsOther{}
	// Media, general unions, and generic declarations remain deferred.
}

func TestRootFooReachable(t *testing.T)            { _ = baml_sdk.Foo{} }
func TestLoremResumeReachable(t *testing.T)        { _ = baml_sdk.LoremResume{} }
func TestDeepNamespaceThingReachable(t *testing.T) { _ = baml_sdk.ABThing{} }

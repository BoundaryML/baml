package sdk_test

// A generic class the runtime keeps to itself (`baml.sap._ParseCache<T>`): an
// instance crosses as a reference at its type arguments, so its methods bind
// `T` from the receiver, and two references to one engine instance are Equal.

import (
	"context"
	"testing"

	"baml.local/sdk/baml_sdk"
	"baml.local/sdk/baml_sdk/baml"
	baml_go "github.com/boundaryml/baml-go"
)

func Test_generic_references_method_binds_the_class_type_argument_from_its_receiver(t *testing.T) {
	ctx := context.Background()
	ints, err := baml.SapNewParseCache[int64](ctx)
	if err != nil {
		t.Fatalf("new int cache: %v", err)
	}
	number, err := ints.ParseFinal(ctx, "41")
	if err != nil || number != 41 {
		t.Fatalf("int cache parse = %d, %v", number, err)
	}
	// The same text read at another type argument: the binding is the
	// receiver's, not a default.
	texts, err := baml.SapNewParseCache[string](ctx)
	if err != nil {
		t.Fatalf("new string cache: %v", err)
	}
	text, err := texts.ParseFinal(ctx, "41")
	if err != nil || text != "41" {
		t.Fatalf("string cache parse = %q, %v", text, err)
	}
}

func Test_generic_references_reference_carries_its_type_arguments(t *testing.T) {
	ctx := context.Background()
	value, err := baml_sdk.Invoke(
		ctx,
		baml_sdk.NamedTarget("baml.sap._new_parse_cache"),
		baml_sdk.Arguments{},
		baml_sdk.TypeBindings{"T": baml_go.TypeOf[string]()},
	)
	if err != nil {
		t.Fatalf("new string cache: %v", err)
	}
	// A reference decodes only at the type arguments it came with.
	if _, err := baml_go.DecodeAs[baml.SapParseCache[int64]](value); err == nil {
		t.Fatal("a string cache decoded as an int cache")
	}
	texts, err := baml_go.DecodeAs[baml.SapParseCache[string]](value)
	if err != nil {
		t.Fatalf("decode string cache: %v", err)
	}
	text, err := texts.ParseFinal(ctx, "41")
	if err != nil || text != "41" {
		t.Fatalf("string cache parse = %q, %v", text, err)
	}
}

func Test_generic_references_one_engine_instance_is_one_reference(t *testing.T) {
	ctx := context.Background()
	cache, err := baml.SapNewParseCache[int64](ctx)
	if err != nil {
		t.Fatalf("new cache: %v", err)
	}
	same, err := baml_sdk.GenericTestsIdentity(ctx, cache)
	if err != nil || !same.Equal(cache) {
		t.Fatalf("identity = %#v, %v; want the same instance", same, err)
	}
	other, err := baml.SapNewParseCache[int64](ctx)
	if err != nil {
		t.Fatalf("new cache: %v", err)
	}
	if other.Equal(cache) {
		t.Fatal("two engine instances compared equal")
	}
}

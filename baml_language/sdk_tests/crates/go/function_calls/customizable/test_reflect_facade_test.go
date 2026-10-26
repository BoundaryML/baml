package sdk_test

import (
	"context"
	"errors"
	"testing"

	"baml.local/sdk/baml_sdk"
	bamlreflect "baml.local/sdk/baml_sdk/reflect"
	baml_go "github.com/boundaryml/baml-go"
)

// SDK_PARITY_LINT(skip): Go's hand-written reflect facade; the host_reflect fixture that would cover it does not compile
func Test_reflect_facade_builds_runtime_declarations_and_decodes_their_instances(t *testing.T) {
	ctx := context.Background()
	category, err := bamlreflect.Enum(ctx, "Category", []bamlreflect.EnumValue{
		bamlreflect.NewEnumValue("RED", bamlreflect.WithAlias("k7")),
		bamlreflect.NewEnumValue("BLUE"),
	})
	if err != nil {
		t.Fatalf("runtime enum: %v", err)
	}
	record, err := bamlreflect.Class(ctx, "RuntimeRecord", []bamlreflect.Field{
		bamlreflect.NewField("label", bamlreflect.TypeOf[string](), bamlreflect.WithAlias("display_label")),
		bamlreflect.NewField("category", category),
	})
	if err != nil {
		t.Fatalf("runtime class: %v", err)
	}
	if name, err := baml_sdk.GoTypeTestsTypeName(ctx, record); err != nil || name != "RuntimeRecord" {
		t.Fatalf("runtime class name = %q, %v", name, err)
	}

	// An instance of a run-time declaration crosses as the engine's own
	// reference: Go holds it as an OpaqueHandle and can pass it back.
	parsed, err := baml_sdk.GenericTestsParseAs[any](ctx, `{"label":"ok","category":"RED"}`, baml_go.WithTypeArg("T", record))
	instance, ok := parsed.(baml_go.OpaqueHandle)
	if err != nil || !ok {
		t.Fatalf("runtime class instance = %#v, %v; want an OpaqueHandle", parsed, err)
	}
	if echoed, err := baml_sdk.GenericTestsIdentity[any](ctx, instance, baml_go.WithTypeArg("T", record)); err != nil {
		t.Fatalf("runtime class instance passed back = %#v, %v", echoed, err)
	}

	pkg, err := bamlreflect.CompilePackage(ctx, map[string]string{
		"runtime.baml": "class CompiledRow { amount int note string? }",
	})
	if err != nil {
		t.Fatalf("compile package: %v", err)
	}
	compiled, err := pkg.GetClass("CompiledRow")
	if err != nil {
		t.Fatalf("get compiled class: %v", err)
	}
	if name, err := baml_sdk.GoTypeTestsTypeName(ctx, compiled); err != nil || name != "CompiledRow" {
		t.Fatalf("compiled class name = %q, %v", name, err)
	}
	var notFound *bamlreflect.NotFound
	if _, err := pkg.GetClass("Missing"); !errors.As(err, &notFound) {
		t.Fatalf("get missing class = %T %v; want *reflect.NotFound", err, err)
	}
	if found, err := pkg.FindClass("CompiledRow"); err != nil || found == nil {
		t.Fatalf("find compiled class = %#v, %v", found, err)
	}
	if missing, err := pkg.FindClass("Missing"); err != nil || missing != nil {
		t.Fatalf("find missing class = %#v, %v; want nil, nil", missing, err)
	}
}

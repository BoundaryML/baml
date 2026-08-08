package sdk_test

import (
	"bytes"
	"context"
	"encoding/gob"
	"encoding/json"
	"errors"
	stdreflect "reflect"
	"strings"
	"testing"

	"baml.local/sdk/baml_sdk"
	bamlreflect "baml.local/sdk/baml_sdk/reflect"
	baml_go "github.com/boundaryml/baml-go"
)

func Test_runtime_enum_definition_decodes_alias(t *testing.T) {
	ctx := context.Background()
	category, err := bamlreflect.Enum(ctx, "Category", []bamlreflect.EnumValue{
		bamlreflect.NewEnumValue("RED", bamlreflect.WithAlias("k7"), bamlreflect.WithDescription("warm")),
		bamlreflect.NewEnumValue("BLUE"),
	})
	if err != nil {
		t.Fatal(err)
	}
	got, err := baml_sdk.HostParse[any](ctx, `"k7"`, baml_go.WithTypeArg("T", category))
	if err != nil || got != "RED" {
		t.Fatalf("runtime enum parse = %#v, %v", got, err)
	}
}

func Test_runtime_class_definition_preserves_nested_metadata(t *testing.T) {
	ctx := context.Background()
	category, err := bamlreflect.Enum(ctx, "Category", []bamlreflect.EnumValue{
		bamlreflect.NewEnumValue("RED", bamlreflect.WithAlias("k7")),
	})
	if err != nil {
		t.Fatal(err)
	}
	record, err := bamlreflect.Class(ctx, "RuntimeRecord", []bamlreflect.Field{
		bamlreflect.NewField("label", bamlreflect.TypeOf[string](), bamlreflect.WithAlias("display_label")),
		bamlreflect.NewField("category", category),
		bamlreflect.NewField("scores", bamlreflect.TypeOf[int64]().Array()),
	})
	if err != nil {
		t.Fatal(err)
	}
	got, err := baml_sdk.HostParse[any](
		ctx,
		`{"display_label":"ok","category":"k7","scores":[1,2]}`,
		baml_go.WithTypeArg("T", record),
	)
	want := map[string]any{
		"label": "ok", "category": "RED", "scores": []any{int64(1), int64(2)},
	}
	if err != nil || !stdreflect.DeepEqual(got, want) {
		t.Fatalf("runtime class parse = %#v, %v; want %#v", got, err, want)
	}
}

func Test_compiled_package_returns_class_graph(t *testing.T) {
	ctx := context.Background()
	pkg, err := bamlreflect.CompilePackage(ctx, map[string]string{
		"runtime.baml": "class CompiledRow { amount int note string? }",
	})
	if err != nil {
		t.Fatal(err)
	}
	compiled, err := pkg.GetClass("CompiledRow")
	if err != nil || compiled == nil {
		t.Fatalf("get compiled class = %#v, %v", compiled, err)
	}
	got, err := baml_sdk.HostParse[any](ctx, `{"amount":7}`, baml_go.WithTypeArg("T", *compiled))
	want := map[string]any{"amount": int64(7), "note": nil}
	if err != nil || !stdreflect.DeepEqual(got, want) {
		t.Fatalf("compiled class parse = %#v, %v; want %#v", got, err, want)
	}
}

func Test_wire_occurrences_are_fresh_and_handles_reject_serialization(t *testing.T) {
	ctx := context.Background()
	runtimeType, err := bamlreflect.Class(ctx, "Fresh", []bamlreflect.Field{
		bamlreflect.NewField("value", bamlreflect.TypeOf[int64]()),
	})
	if err != nil {
		t.Fatal(err)
	}
	equal, err := baml_sdk.HostTypeEqual[any, any](
		ctx,
		baml_go.WithTypeArg("A", runtimeType),
		baml_go.WithTypeArg("B", runtimeType),
	)
	if err != nil || equal {
		t.Fatalf("fresh wire definitions equal = %v, %v; want false", equal, err)
	}
	if _, err := json.Marshal(runtimeType); err == nil || !strings.Contains(err.Error(), "cannot be serialized") {
		t.Fatalf("json serialization error = %v", err)
	}
	var encoded bytes.Buffer
	if err := gob.NewEncoder(&encoded).Encode(runtimeType); err == nil || !strings.Contains(err.Error(), "cannot be serialized") {
		t.Fatalf("gob serialization error = %v", err)
	}
}

func Test_known_type_tokens_compose_and_reject_unknowns(t *testing.T) {
	ctx := context.Background()
	assertTypeName := func(want string, ty bamlreflect.Type) {
		t.Helper()
		got, err := baml_sdk.HostTypeName[any](ctx, baml_go.WithTypeArg("T", ty))
		if err != nil || got != want {
			t.Fatalf("type name = %q, %v; want %q", got, err, want)
		}
	}

	assertTypeName("string", bamlreflect.TypeOf[string]())
	assertTypeName("(string | null)[]", bamlreflect.TypeOf[string]().Optional().Array())
	assertTypeName("StaticRecord", bamlreflect.TypeOf[baml_sdk.StaticRecord]())
	assertTypeName("StaticChoice", bamlreflect.TypeOf[baml_sdk.StaticChoice]())
	assertTypeName("StaticNamed", baml_sdk.StaticNamed)
	assertTypeName("image", bamlreflect.TypeOf[baml_go.Image]())

	type hostOnly struct{ Value string }
	_, err := baml_sdk.HostTypeName[any](ctx, baml_go.WithTypeArg("T", bamlreflect.TypeOf[hostOnly]()))
	if err == nil || !strings.Contains(err.Error(), "unsupported Go type token") {
		t.Fatalf("unsupported host token error = %v", err)
	}
}

func Test_host_handles_expose_composition_only(t *testing.T) {
	composed := bamlreflect.TypeOf[int64]().Meta(bamlreflect.WithDescription("count")).Type.Optional().Array()
	if _, err := json.Marshal(composed); err == nil {
		t.Fatal("composed type unexpectedly serialized")
	}
}

func Test_reflection_compile_errors_are_typed(t *testing.T) {
	_, err := bamlreflect.CompilePackage(context.Background(), map[string]string{
		"broken.baml": "class {",
	})
	var compilation *bamlreflect.CompilationError
	if !errors.As(err, &compilation) {
		t.Fatalf("compile error = %T %v; want *reflect.CompilationError", err, err)
	}
	if compilation.ClassName != "baml.reflect.errors.CompilationError" {
		t.Fatalf("compilation class = %q", compilation.ClassName)
	}
}

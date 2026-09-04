package sdk_test

import (
	"context"
	"fmt"
	"testing"

	bamlreflect "baml.local/sdk/baml_sdk/reflect"
	baml_go "github.com/boundaryml/baml-go"
)

func Test_compiled_package_keeps_its_originating_runtime_context(t *testing.T) {
	files := map[string]string{"runtime.baml": "class CompiledRow { amount int note string? }"}
	control, err := bamlreflect.CompilePackage(context.Background(), files)
	if err != nil {
		t.Fatal(err)
	}
	_, wantErr := control.GetClass("CompiledRow")

	ctx := baml_go.WithRuntime(context.Background(), ^uint64(0))
	pkg, err := bamlreflect.CompilePackage(ctx, files)
	if err != nil {
		t.Fatal(err)
	}
	got, gotErr := pkg.GetClass("CompiledRow")
	// Host-reflection extraction is still deferred in this compiler port.
	// Whatever that boundary supports, the foreign caller context must produce
	// the same result as the control instead of changing native runtime lookup.
	if fmt.Sprint(gotErr) != fmt.Sprint(wantErr) {
		t.Fatalf("foreign context changed GetClass: %v; control: %v", gotErr, wantErr)
	}
	if gotErr == nil && got == nil {
		t.Fatal("GetClass succeeded without a class")
	}
}

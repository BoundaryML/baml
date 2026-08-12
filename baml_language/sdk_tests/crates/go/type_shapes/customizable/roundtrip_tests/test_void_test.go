package sdk_test

import (
	"context"
	"testing"

	"baml.local/sdk/baml_sdk"
)

// Direct port of python_pydantic2/type_shapes/roundtrip_tests/test_void.py.
func Test_no_op(t *testing.T) {
	if err := baml_sdk.VoidNoOp(context.Background()); err != nil {
		t.Fatal(err)
	}
}

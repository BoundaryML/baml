package sdk_test

import (
	"context"
	"reflect"
	"testing"

	"baml.local/sdk/baml_sdk"
)

// Direct synchronous port of Python test_optional_args.py. Python's async
// siblings are N/A because Go uses the same context-aware call in a goroutine.
// The OptBox method matrix is ported in test_methods_on_classes_test.go.
func Test_optional_args_runtime_matrix(t *testing.T) {
	assertValues := func(label string, got []*int64, err error, want []*int64) {
		t.Helper()
		if err != nil {
			t.Fatalf("%s: %v", label, err)
		}
		if !reflect.DeepEqual(got, want) {
			t.Fatalf("%s = %#v, want %#v", label, got, want)
		}
	}
	pointer := func(value int64) *int64 { return &value }
	cases := []struct {
		name    string
		options []baml_sdk.OptionalArgsProbeOption
		want    []*int64
	}{
		{"omitted", nil, []*int64{pointer(1), pointer(5), pointer(99)}},
		{"explicit Go unset options", []baml_sdk.OptionalArgsProbeOption{nil, nil}, []*int64{pointer(1), pointer(5), pointer(99)}},
		{"opt1 explicit null", []baml_sdk.OptionalArgsProbeOption{baml_sdk.WithOptionalArgsProbeOpt1(nil)}, []*int64{pointer(1), nil, pointer(99)}},
		{"opt1 supplied", []baml_sdk.OptionalArgsProbeOption{baml_sdk.WithOptionalArgsProbeOpt1(pointer(8))}, []*int64{pointer(1), pointer(8), pointer(99)}},
		{"opt2 explicit null", []baml_sdk.OptionalArgsProbeOption{baml_sdk.WithOptionalArgsProbeOpt2(nil)}, []*int64{pointer(1), pointer(5), nil}},
		{"opt2 supplied", []baml_sdk.OptionalArgsProbeOption{baml_sdk.WithOptionalArgsProbeOpt2(pointer(9))}, []*int64{pointer(1), pointer(5), pointer(9)}},
		{"both explicit null", []baml_sdk.OptionalArgsProbeOption{baml_sdk.WithOptionalArgsProbeOpt1(nil), baml_sdk.WithOptionalArgsProbeOpt2(nil)}, []*int64{pointer(1), nil, nil}},
		{"both supplied", []baml_sdk.OptionalArgsProbeOption{baml_sdk.WithOptionalArgsProbeOpt1(pointer(8)), baml_sdk.WithOptionalArgsProbeOpt2(pointer(9))}, []*int64{pointer(1), pointer(8), pointer(9)}},
	}
	for _, test := range cases {
		t.Run(test.name, func(t *testing.T) {
			got, err := baml_sdk.OptionalArgsProbe(context.Background(), 1, test.options...)
			assertValues(test.name, got, err, test.want)
		})
	}
}

func Test_unset_and_none_differ_in_one_call(t *testing.T) {
	pointer := func(value int64) *int64 { return &value }
	got, err := baml_sdk.OptionalArgsProbe(context.Background(), 1, nil, baml_sdk.WithOptionalArgsProbeOpt2(nil))
	want := []*int64{pointer(1), pointer(5), nil}
	if err != nil || !reflect.DeepEqual(got, want) {
		t.Fatalf("unset/null = %#v, %v", got, err)
	}
	got, err = baml_sdk.OptionalArgsProbe(context.Background(), 1, baml_sdk.WithOptionalArgsProbeOpt1(nil), nil)
	want = []*int64{pointer(1), nil, pointer(99)}
	if err != nil || !reflect.DeepEqual(got, want) {
		t.Fatalf("null/unset = %#v, %v", got, err)
	}
}

// Python's async-only duplicates are N/A for Go's context-aware synchronous API.

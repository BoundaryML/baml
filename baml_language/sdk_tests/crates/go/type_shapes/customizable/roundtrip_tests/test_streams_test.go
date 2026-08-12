package sdk_test

import (
	"context"
	"reflect"
	"testing"

	"baml.local/sdk/baml_sdk"
)

// Direct host-constructible subset of the Python test_streams.py port. These
// calls round-trip stream companion types as ordinary values; they do not open
// or consume an actual stream.
func Test_round_trip_resume_stream(t *testing.T) {
	resume := baml_sdk.LoremResumeStream{}
	name := "ada"
	resume.Name = &name
	if got, err := baml_sdk.LoremRoundTripResumeStream(context.Background(), resume); err != nil || got.Name == nil || *got.Name != name || got.Email != nil {
		t.Fatalf("resume stream = %#v, %v", got, err)
	}
}

func Test_round_trip_root_foo_stream(t *testing.T) {
	value := int64(3)
	foo := baml_sdk.FooStream{V: &value}
	if got, err := baml_sdk.LoremRoundTripRootFooStream(context.Background(), foo); err != nil || got.V == nil || *got.V != value {
		t.Fatalf("foo stream = %#v, %v", got, err)
	}
}

func Test_round_trip_box_of_resume_stream(t *testing.T) {
	name := "grace"
	want := baml_sdk.LoremBox[baml_sdk.LoremResumeStream]{
		V: baml_sdk.LoremResumeStream{Name: &name},
	}
	got, err := baml_sdk.LoremRoundTripBoxOfResumeStream(context.Background(), want)
	if err != nil || !reflect.DeepEqual(got, want) {
		t.Fatalf("box of resume stream = %#v, %v, want %#v", got, err, want)
	}
}

func Test_round_trip_resume_or_resume_stream(t *testing.T) {
	resume := baml_sdk.LoremResume{Name: "hopper"}
	want := baml_sdk.NewLoremResumeOrLoremResumeStreamFromLoremResume(resume)
	got, err := baml_sdk.LoremRoundTripResumeOrResumeStream(context.Background(), want)
	if err != nil {
		t.Fatal(err)
	}
	gotResume, ok := got.AsLoremResume()
	if !ok || !reflect.DeepEqual(gotResume, resume) {
		t.Fatalf("resume union = %#v, ok %v, want %#v", gotResume, ok, resume)
	}
}

func Test_round_trip_resume_or_http_response(t *testing.T) {
	email := "a@x.com"
	resume := baml_sdk.LoremResume{Name: "lovelace", Email: &email}
	want := baml_sdk.NewHttpResponseOrLoremResumeFromLoremResume(resume)
	got, err := baml_sdk.LoremRoundTripResumeOrHttpResponse(context.Background(), want)
	if err != nil {
		t.Fatal(err)
	}
	gotResume, ok := got.AsLoremResume()
	if !ok || !reflect.DeepEqual(gotResume, resume) {
		t.Fatalf("HTTP response union's resume arm = %#v, ok %v, want %#v", gotResume, ok, resume)
	}
}

// Actual streaming remains deferred. Bare, list, and stream HTTP response
// inputs also remain deferred because their opaque handles cannot be created
// directly by a Go host; engine-minted handle coverage lives in
// test_rust_type_test.go.

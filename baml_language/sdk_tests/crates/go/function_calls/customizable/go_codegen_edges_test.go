package sdk_test

import (
	"context"
	"errors"
	"math/big"
	"reflect"
	"strings"
	"testing"
	"time"

	"baml.local/sdk/baml_sdk"
	"github.com/boundaryml/baml-go"
)

var (
	_ func(context.Context, func(int64, baml_sdk.CallbackIntWithYIntWithZIntOptions) int64, int64) ([]int64, error)     = baml_sdk.HostCallableTestsCallCallbackWithOptionalArgsAllUnset
	_ func(context.Context, func(int64, baml_sdk.CallbackIntWithValueOptionalIntOptions) int64, int64) ([]int64, error) = baml_sdk.HostCallableTestsCallCallbackWithNullableOptionalStates
	_ func(context.Context, int64, ...baml_sdk.OptionalArgsProbeOption) ([]*int64, error)                               = baml_sdk.OptionalArgsProbe
	_ func(*int64) baml_sdk.OptionalArgsProbeOption                                                                     = baml_sdk.WithOptionalArgsProbeOpt1
	_ func(*int64) baml_sdk.OptionalArgsProbeOption                                                                     = baml_sdk.WithOptionalArgsProbeOpt2
	_ func(context.Context, ...baml_sdk.DefaultArgsMatrixOption) (baml_sdk.DefaultArgsMatrixResult, error)              = baml_sdk.DefaultArgsMatrix
	_ func(string) baml_sdk.DefaultArgsMatrixOption                                                                     = baml_sdk.WithDefaultArgsMatrixStringValue
	_ func(int64) baml_sdk.DefaultArgsMatrixOption                                                                      = baml_sdk.WithDefaultArgsMatrixIntValue
	_ func(*big.Int) baml_sdk.DefaultArgsMatrixOption                                                                   = baml_sdk.WithDefaultArgsMatrixBigintValue
	_ func(float64) baml_sdk.DefaultArgsMatrixOption                                                                    = baml_sdk.WithDefaultArgsMatrixFloatValue
	_ func(bool) baml_sdk.DefaultArgsMatrixOption                                                                       = baml_sdk.WithDefaultArgsMatrixBoolValue
	_ func(baml_go.Null) baml_sdk.DefaultArgsMatrixOption                                                               = baml_sdk.WithDefaultArgsMatrixNullValue
	_ func(*[]byte) baml_sdk.DefaultArgsMatrixOption                                                                    = baml_sdk.WithDefaultArgsMatrixBytesValue
	_ func(baml_sdk.Person) baml_sdk.DefaultArgsMatrixOption                                                            = baml_sdk.WithDefaultArgsMatrixClassValue
	_ func([]string) baml_sdk.DefaultArgsMatrixOption                                                                   = baml_sdk.WithDefaultArgsMatrixListValue
	_ func(map[string]int64) baml_sdk.DefaultArgsMatrixOption                                                           = baml_sdk.WithDefaultArgsMatrixMapValue
	_ func([]*string) baml_sdk.DefaultArgsMatrixOption                                                                  = baml_sdk.WithDefaultArgsMatrixListOptional
	_ func(map[string]*int64) baml_sdk.DefaultArgsMatrixOption                                                          = baml_sdk.WithDefaultArgsMatrixMapOptional
	_ func(*string) baml_sdk.DefaultArgsMatrixOption                                                                    = baml_sdk.WithDefaultArgsMatrixNullableValue
	_ func(*baml_sdk.Person) baml_sdk.DefaultArgsMatrixOption                                                           = baml_sdk.WithDefaultArgsMatrixOptionalClass
	_ func(*[]string) baml_sdk.DefaultArgsMatrixOption                                                                  = baml_sdk.WithDefaultArgsMatrixOptionalList
	_ func(*map[string]int64) baml_sdk.DefaultArgsMatrixOption                                                          = baml_sdk.WithDefaultArgsMatrixOptionalMap
	_ func(context.Context, ...baml_sdk.DefaultedVoidOption) error                                                      = baml_sdk.DefaultedVoid
	_ func(string) baml_sdk.DefaultedVoidOption                                                                         = baml_sdk.WithDefaultedVoidValue
)

func Test_go_codegen_person_round_trip(t *testing.T) {
	want := baml_sdk.Person{Person: "record", Name: "Ada", Age: 37}
	got, err := baml_sdk.RoundTripPerson(context.Background(), want)
	if err != nil || got != want {
		t.Fatalf("got %#v, %v, want %#v", got, err, want)
	}
}

func Test_go_codegen_defaulted_argument_type_matrix(t *testing.T) {
	stringPointer := func(value string) *string { return &value }
	defaultPerson := baml_sdk.Person{Person: "default", Name: "Default", Age: 13}
	wantDefaults := baml_sdk.DefaultArgsMatrixResult{
		StringValue: "default", IntValue: 10, BigintValue: big.NewInt(11), FloatValue: 12.5,
		BoolValue: true, NullValue: baml_go.Null{}, ClassValue: defaultPerson,
		ListValue: []string{"default"}, MapValue: map[string]int64{"default": 14},
		ListOptional: []*string{nil, stringPointer("default")}, MapOptional: map[string]*int64{"default": nil},
	}
	got, err := baml_sdk.DefaultArgsMatrix(context.Background())
	if err != nil || !reflect.DeepEqual(got, wantDefaults) {
		t.Fatalf("defaults = %#v, %v, want %#v", got, err, wantDefaults)
	}

	bytesValue := []byte{1, 2, 3}
	classValue := baml_sdk.Person{Person: "override", Name: "Ada", Age: 37}
	listValue := []string{"one", "two"}
	mapValue := map[string]int64{"answer": 42}
	optionalText := "present"
	optionalInt := int64(9)
	listOptional := []*string{nil, &optionalText}
	mapOptional := map[string]*int64{"null": nil, "value": &optionalInt}
	wantOverrides := baml_sdk.DefaultArgsMatrixResult{
		StringValue: "override", IntValue: 20, BigintValue: big.NewInt(21), FloatValue: 22.5,
		BoolValue: false, NullValue: baml_go.Null{}, BytesValue: &bytesValue, ClassValue: classValue,
		ListValue: listValue, MapValue: mapValue, ListOptional: listOptional, MapOptional: mapOptional,
		NullableValue: &optionalText, OptionalClass: &classValue, OptionalList: &listValue, OptionalMap: &mapValue,
	}
	got, err = baml_sdk.DefaultArgsMatrix(context.Background(),
		baml_sdk.WithDefaultArgsMatrixStringValue("override"), baml_sdk.WithDefaultArgsMatrixIntValue(20),
		baml_sdk.WithDefaultArgsMatrixBigintValue(big.NewInt(21)), baml_sdk.WithDefaultArgsMatrixFloatValue(22.5),
		baml_sdk.WithDefaultArgsMatrixBoolValue(false), baml_sdk.WithDefaultArgsMatrixNullValue(baml_go.Null{}),
		baml_sdk.WithDefaultArgsMatrixBytesValue(&bytesValue), baml_sdk.WithDefaultArgsMatrixClassValue(classValue),
		baml_sdk.WithDefaultArgsMatrixListValue(listValue), baml_sdk.WithDefaultArgsMatrixMapValue(mapValue),
		baml_sdk.WithDefaultArgsMatrixListOptional(listOptional), baml_sdk.WithDefaultArgsMatrixMapOptional(mapOptional),
		baml_sdk.WithDefaultArgsMatrixNullableValue(&optionalText), baml_sdk.WithDefaultArgsMatrixOptionalClass(&classValue),
		baml_sdk.WithDefaultArgsMatrixOptionalList(&listValue), baml_sdk.WithDefaultArgsMatrixOptionalMap(&mapValue),
	)
	if err != nil || !reflect.DeepEqual(got, wantOverrides) {
		t.Fatalf("overrides = %#v, %v, want %#v", got, err, wantOverrides)
	}
}

func Test_go_codegen_option_name_collisions(t *testing.T) {
	got, err := baml_sdk.DefaultNameCollisions(context.Background(), "required")
	want := []string{"required", "options", "option"}
	if err != nil || !reflect.DeepEqual(got, want) {
		t.Fatalf("got %#v, %v, want %#v", got, err, want)
	}
}

func Test_go_codegen_defaulted_void(t *testing.T) {
	if err := baml_sdk.DefaultedVoid(context.Background()); err != nil {
		t.Fatal(err)
	}
	if err := baml_sdk.DefaultedVoid(context.Background(), baml_sdk.WithDefaultedVoidValue("override")); err != nil {
		t.Fatal(err)
	}
}

func Test_go_codegen_optional_arg_last_value_wins(t *testing.T) {
	pointer := func(value int64) *int64 { return &value }
	got, err := baml_sdk.OptionalArgsProbe(context.Background(), 3, nil,
		baml_sdk.WithOptionalArgsProbeOpt1(pointer(6)),
		baml_sdk.WithOptionalArgsProbeOpt1(pointer(7)),
	)
	want := []*int64{pointer(3), pointer(7), pointer(99)}
	if err != nil || !reflect.DeepEqual(got, want) {
		t.Fatalf("got %#v, %v, want %#v", got, err, want)
	}
}

func Test_go_codegen_context_deadline(t *testing.T) {
	ctx, cancel := context.WithTimeout(context.Background(), 50*time.Millisecond)
	defer cancel()
	start := time.Now()
	_, err := baml_sdk.ThrowsTestSleepMs(ctx, 2000)
	if !errors.Is(err, context.DeadlineExceeded) {
		t.Fatalf("deadline error = %v", err)
	}
	if elapsed := time.Since(start); elapsed >= 500*time.Millisecond {
		t.Fatalf("deadline took %s", elapsed)
	}
}

// Go-specific bridge boundary: a value that cannot be serialized never
// reaches BAML's semantic argument relation and remains distinguishable from
// a BAML InvalidArgument failure.
func Test_go_codegen_default_argument_serialization_error_names_argument(t *testing.T) {
	_, err := baml_sdk.DefaultArgsMatrix(
		context.Background(),
		baml_sdk.WithDefaultArgsMatrixBigintValue(nil),
	)
	if err == nil || !strings.Contains(err.Error(), `argument "bigint_value"`) || !strings.Contains(err.Error(), "uninitialized") {
		t.Fatalf("serialization error = %v", err)
	}
	if strings.Contains(err.Error(), "BAML error") {
		t.Fatalf("bridge serialization error was misclassified as runtime error: %v", err)
	}
}

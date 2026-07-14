package sdk_test

import (
	"context"
	"math/big"
	"reflect"
	"testing"

	"baml.local/sdk/baml_sdk"
	"github.com/boundaryml/baml/sdks/go/baml_go"
)

var (
	_ func(context.Context, int64, ...baml_sdk.OptionalArgsProbeOption) ([]*int64, error) = baml_sdk.OptionalArgsProbe
	_ func(*int64) baml_sdk.OptionalArgsProbeOption                                       = baml_sdk.WithOptionalArgsProbeOpt1
	_ func(*int64) baml_sdk.OptionalArgsProbeOption                                       = baml_sdk.WithOptionalArgsProbeOpt2

	_ func(context.Context, ...baml_sdk.DefaultArgsMatrixOption) (baml_sdk.DefaultArgsMatrixResult, error) = baml_sdk.DefaultArgsMatrix
	_ func(string) baml_sdk.DefaultArgsMatrixOption                                                        = baml_sdk.WithDefaultArgsMatrixStringValue
	_ func(int64) baml_sdk.DefaultArgsMatrixOption                                                         = baml_sdk.WithDefaultArgsMatrixIntValue
	_ func(*big.Int) baml_sdk.DefaultArgsMatrixOption                                                      = baml_sdk.WithDefaultArgsMatrixBigintValue
	_ func(float64) baml_sdk.DefaultArgsMatrixOption                                                       = baml_sdk.WithDefaultArgsMatrixFloatValue
	_ func(bool) baml_sdk.DefaultArgsMatrixOption                                                          = baml_sdk.WithDefaultArgsMatrixBoolValue
	_ func(baml_go.Null) baml_sdk.DefaultArgsMatrixOption                                                  = baml_sdk.WithDefaultArgsMatrixNullValue
	_ func(*[]byte) baml_sdk.DefaultArgsMatrixOption                                                       = baml_sdk.WithDefaultArgsMatrixBytesValue
	_ func(baml_sdk.Person) baml_sdk.DefaultArgsMatrixOption                                               = baml_sdk.WithDefaultArgsMatrixClassValue
	_ func([]string) baml_sdk.DefaultArgsMatrixOption                                                      = baml_sdk.WithDefaultArgsMatrixListValue
	_ func(map[string]int64) baml_sdk.DefaultArgsMatrixOption                                              = baml_sdk.WithDefaultArgsMatrixMapValue
	_ func([]*string) baml_sdk.DefaultArgsMatrixOption                                                     = baml_sdk.WithDefaultArgsMatrixListOptional
	_ func(map[string]*int64) baml_sdk.DefaultArgsMatrixOption                                             = baml_sdk.WithDefaultArgsMatrixMapOptional
	_ func(*string) baml_sdk.DefaultArgsMatrixOption                                                       = baml_sdk.WithDefaultArgsMatrixNullableValue
	_ func(*baml_sdk.Person) baml_sdk.DefaultArgsMatrixOption                                              = baml_sdk.WithDefaultArgsMatrixOptionalClass
	_ func(*[]string) baml_sdk.DefaultArgsMatrixOption                                                     = baml_sdk.WithDefaultArgsMatrixOptionalList
	_ func(*map[string]int64) baml_sdk.DefaultArgsMatrixOption                                             = baml_sdk.WithDefaultArgsMatrixOptionalMap
	_ func(context.Context, ...baml_sdk.DefaultedVoidOption) error                                         = baml_sdk.DefaultedVoid
	_ func(string) baml_sdk.DefaultedVoidOption                                                            = baml_sdk.WithDefaultedVoidValue
)

func TestScalarFunctions(t *testing.T) {
	ctx := context.Background()

	hello, err := baml_sdk.HelloWorld(ctx)
	if err != nil {
		t.Fatal(err)
	}
	if hello != "hello world" {
		t.Fatalf("HelloWorld() = %q, want %q", hello, "hello world")
	}

	echo, err := baml_sdk.SingleRequiredArg(ctx, "hi")
	if err != nil {
		t.Fatal(err)
	}
	if echo != "hi" {
		t.Fatalf("SingleRequiredArg() = %q, want %q", echo, "hi")
	}
}

func TestPersonRoundTrip(t *testing.T) {
	want := baml_sdk.Person{Person: "record", Name: "Ada", Age: 37}
	got, err := baml_sdk.RoundTripPerson(context.Background(), want)
	if err != nil {
		t.Fatal(err)
	}
	if got != want {
		t.Fatalf("RoundTripPerson() = %#v, want %#v", got, want)
	}
}

func TestDefaultedFunctionArguments(t *testing.T) {
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

	got, err := baml_sdk.OptionalArgsProbe(context.Background(), 1)
	assertValues("omitted", got, err, []*int64{pointer(1), pointer(5), pointer(99)})

	got, err = baml_sdk.OptionalArgsProbe(
		context.Background(),
		2,
		baml_sdk.WithOptionalArgsProbeOpt1(nil),
		baml_sdk.WithOptionalArgsProbeOpt2(pointer(8)),
	)
	assertValues("explicit null and value", got, err, []*int64{pointer(2), nil, pointer(8)})

	got, err = baml_sdk.OptionalArgsProbe(
		context.Background(),
		3,
		nil,
		baml_sdk.WithOptionalArgsProbeOpt1(pointer(6)),
		baml_sdk.WithOptionalArgsProbeOpt1(pointer(7)),
	)
	assertValues("nil option and last value wins", got, err, []*int64{pointer(3), pointer(7), pointer(99)})
}

func TestDefaultedArgumentTypeMatrix(t *testing.T) {
	stringPointer := func(value string) *string { return &value }
	defaultPerson := baml_sdk.Person{Person: "default", Name: "Default", Age: 13}
	wantDefaults := baml_sdk.DefaultArgsMatrixResult{
		StringValue:   "default",
		IntValue:      10,
		BigintValue:   big.NewInt(11),
		FloatValue:    12.5,
		BoolValue:     true,
		NullValue:     baml_go.Null{},
		BytesValue:    nil,
		ClassValue:    defaultPerson,
		ListValue:     []string{"default"},
		MapValue:      map[string]int64{"default": 14},
		ListOptional:  []*string{nil, stringPointer("default")},
		MapOptional:   map[string]*int64{"default": nil},
		NullableValue: nil,
		OptionalClass: nil,
		OptionalList:  nil,
		OptionalMap:   nil,
	}
	got, err := baml_sdk.DefaultArgsMatrix(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	if !reflect.DeepEqual(got, wantDefaults) {
		t.Fatalf("omitted defaults = %#v, want %#v", got, wantDefaults)
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
		StringValue:   "override",
		IntValue:      20,
		BigintValue:   big.NewInt(21),
		FloatValue:    22.5,
		BoolValue:     false,
		NullValue:     baml_go.Null{},
		BytesValue:    &bytesValue,
		ClassValue:    classValue,
		ListValue:     listValue,
		MapValue:      mapValue,
		ListOptional:  listOptional,
		MapOptional:   mapOptional,
		NullableValue: &optionalText,
		OptionalClass: &classValue,
		OptionalList:  &listValue,
		OptionalMap:   &mapValue,
	}
	got, err = baml_sdk.DefaultArgsMatrix(
		context.Background(),
		baml_sdk.WithDefaultArgsMatrixStringValue("override"),
		baml_sdk.WithDefaultArgsMatrixIntValue(20),
		baml_sdk.WithDefaultArgsMatrixBigintValue(big.NewInt(21)),
		baml_sdk.WithDefaultArgsMatrixFloatValue(22.5),
		baml_sdk.WithDefaultArgsMatrixBoolValue(false),
		baml_sdk.WithDefaultArgsMatrixNullValue(baml_go.Null{}),
		baml_sdk.WithDefaultArgsMatrixBytesValue(&bytesValue),
		baml_sdk.WithDefaultArgsMatrixClassValue(classValue),
		baml_sdk.WithDefaultArgsMatrixListValue(listValue),
		baml_sdk.WithDefaultArgsMatrixMapValue(mapValue),
		baml_sdk.WithDefaultArgsMatrixListOptional(listOptional),
		baml_sdk.WithDefaultArgsMatrixMapOptional(mapOptional),
		baml_sdk.WithDefaultArgsMatrixNullableValue(&optionalText),
		baml_sdk.WithDefaultArgsMatrixOptionalClass(&classValue),
		baml_sdk.WithDefaultArgsMatrixOptionalList(&listValue),
		baml_sdk.WithDefaultArgsMatrixOptionalMap(&mapValue),
	)
	if err != nil {
		t.Fatal(err)
	}
	if !reflect.DeepEqual(got, wantOverrides) {
		t.Fatalf("explicit overrides = %#v, want %#v", got, wantOverrides)
	}
}

func TestGeneratedOptionNameCollisionsCompileAndDefault(t *testing.T) {
	got, err := baml_sdk.DefaultNameCollisions(context.Background(), "required")
	if err != nil {
		t.Fatal(err)
	}
	want := []string{"required", "options", "option"}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("collision defaults = %#v, want %#v", got, want)
	}
}

func TestDefaultedVoidFunction(t *testing.T) {
	if err := baml_sdk.DefaultedVoid(context.Background()); err != nil {
		t.Fatal(err)
	}
	if err := baml_sdk.DefaultedVoid(
		context.Background(),
		baml_sdk.WithDefaultedVoidValue("override"),
	); err != nil {
		t.Fatal(err)
	}
}

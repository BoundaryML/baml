package sdk_test

import (
	"context"
	"errors"
	"fmt"
	"reflect"
	"runtime"
	"strings"
	"sync"
	"sync/atomic"
	"testing"
	"time"

	baml_sdk "baml.local/sdk/baml_sdk"
	baml_go "github.com/boundaryml/baml-go"
)

type callbackLifetimeMarker struct{ id uint64 }

var callbackLifetimeMarkerID atomic.Uint64

type callbackLifetimeError struct {
	marker *callbackLifetimeMarker
}

func (*callbackLifetimeError) Error() string { return "tracked callback error" }

func waitForFinalizers(t *testing.T, finalized *atomic.Int64, want int64) {
	t.Helper()
	deadline := time.Now().Add(5 * time.Second)
	for finalized.Load() < want && time.Now().Before(deadline) {
		runtime.GC()
		runtime.Gosched()
		time.Sleep(10 * time.Millisecond)
	}
	if got := finalized.Load(); got != want {
		t.Fatalf("finalized callback identities = %d, want %d", got, want)
	}
}

func trackedCallbackError(finalized *atomic.Int64) error {
	marker := &callbackLifetimeMarker{id: callbackLifetimeMarkerID.Add(1)}
	runtime.SetFinalizer(marker, func(*callbackLifetimeMarker) { finalized.Add(1) })
	return &callbackLifetimeError{marker: marker}
}

func trackedPanicValue(finalized *atomic.Int64) any {
	marker := &callbackLifetimeMarker{id: callbackLifetimeMarkerID.Add(1)}
	runtime.SetFinalizer(marker, func(*callbackLifetimeMarker) { finalized.Add(1) })
	return &callbackLifetimeError{marker: marker}
}

func waitForCallbackStart(t *testing.T, started <-chan struct{}, done <-chan error) {
	t.Helper()
	select {
	case <-started:
		return
	case err := <-done:
		t.Fatalf("BAML call ended before dispatching callback: %v", err)
	case <-time.After(5 * time.Second):
		t.Fatal("timed out waiting for BAML callback dispatch")
	}
}

func Test_host_callable_primitive_and_multiple_arguments(t *testing.T) {
	ctx := context.Background()
	got, err := baml_sdk.HostCallableTestsCallWithCallback(ctx, func(value int64) string {
		return fmt.Sprintf("got %d", value)
	}, 5)
	if err != nil || got != "got 5" {
		t.Fatalf("callback result = %q, %v", got, err)
	}

	got, err = baml_sdk.HostCallableTestsCallWithTwoArgs(ctx, func(value int64, prefix string) string {
		return fmt.Sprintf("%s:%d", prefix, value)
	}, 7, "value")
	if err != nil || got != "value:7" {
		t.Fatalf("two-argument callback result = %q, %v", got, err)
	}

	integer, err := baml_sdk.HostCallableTestsCallIntCallback(ctx, func(value int64) int64 {
		return value * 2
	}, 21)
	if err != nil || integer != 42 {
		t.Fatalf("integer callback result = %d, %v", integer, err)
	}
}

func Test_baml_closure_is_a_native_callable_with_host_language_arguments(t *testing.T) {
	ctx := context.Background()
	addTen, err := baml_sdk.HostCallableTestsMakeAdder(ctx, 10)
	if err != nil {
		t.Fatal(err)
	}
	if got := addTen(ctx, 5); got != 15 {
		t.Fatalf("addTen(5) = %d, want 15", got)
	}
	if got := addTen(ctx, 7); got != 17 {
		t.Fatalf("addTen(7) = %d, want 17", got)
	}
}

func Test_baml_closure_decodes_multiple_args_and_structured_return_values(t *testing.T) {
	ctx := context.Background()
	build, err := baml_sdk.HostCallableTestsMakePairBuilder(ctx, 30)
	if err != nil {
		t.Fatal(err)
	}
	if got := build(ctx, 12, "Ada"); got != (baml_sdk.HostCallableTestsPerson{Name: "Ada", Age: 42}) {
		t.Fatalf("build(12, Ada) = %#v", got)
	}
	if got := build(ctx, 5, "Grace"); got != (baml_sdk.HostCallableTestsPerson{Name: "Grace", Age: 35}) {
		t.Fatalf("build(5, Grace) = %#v", got)
	}
}

func Test_baml_closure_is_reusable_and_retains_mutable_captures(t *testing.T) {
	ctx := context.Background()
	nextValue, err := baml_sdk.HostCallableTestsMakeCounter(ctx, 40)
	if err != nil {
		t.Fatal(err)
	}
	if got := nextValue(ctx); got != 41 {
		t.Fatalf("first counter value = %d, want 41", got)
	}
	if got := nextValue(ctx); got != 42 {
		t.Fatalf("second counter value = %d, want 42", got)
	}
}

func optionalArgsCallback(
	x int64,
	options baml_sdk.CallbackIntWithYIntWithZIntOptions,
) int64 {
	return x*100 + options.Y.Or(8)*10 + options.Z.Or(9)
}

func Test_host_callable_optional_arguments(t *testing.T) {
	ctx := context.Background()

	unset, err := baml_sdk.HostCallableTestsCallCallbackWithOptionalArgsAllUnset(ctx, optionalArgsCallback, 5)
	if err != nil || !reflect.DeepEqual(unset, []int64{589}) {
		t.Fatalf("all-unset optional callback = %#v, %v; want [589]", unset, err)
	}

	partial, err := baml_sdk.HostCallableTestsCallCallbackWithOptionalArgsPartiallySet(ctx, optionalArgsCallback, 5)
	if err != nil || !reflect.DeepEqual(partial, []int64{529, 583}) {
		t.Fatalf("partially-set optional callback = %#v, %v; want [529 583]", partial, err)
	}

	all, err := baml_sdk.HostCallableTestsCallCallbackWithOptionalArgsAllSet(ctx, optionalArgsCallback, 5)
	if err != nil || !reflect.DeepEqual(all, []int64{523}) {
		t.Fatalf("all-set optional callback = %#v, %v; want [523]", all, err)
	}
}

func Test_host_callable_nullable_optional_distinguishes_all_three_states(t *testing.T) {
	callback := func(x int64, options baml_sdk.CallbackIntWithValueOptionalIntOptions) int64 {
		value, supplied := options.Value.Get()
		if !supplied {
			return x * 100
		}
		if value == nil {
			return x*100 + 1
		}
		return x*100 + *value
	}
	got, err := baml_sdk.HostCallableTestsCallCallbackWithNullableOptionalStates(
		context.Background(),
		callback,
		5,
	)
	if err != nil || !reflect.DeepEqual(got, []int64{500, 501, 507}) {
		t.Fatalf("nullable optional states = %#v, %v; want [500 501 507]", got, err)
	}
}

func Test_host_callable_optional_names_avoid_generated_and_projection_collisions(t *testing.T) {
	callback := func(options baml_sdk.CallbackWithCtxIntWithErrIntWithResultIntWithZeroIntWithBootstrapIntWithInitIntWithMainIntWithFooBarIntWithFooBarIntOptions) int64 {
		return options.Ctx.Or(0) + options.Err.Or(0) + options.Result.Or(0) +
			options.Zero.Or(0) + options.Bootstrap.Or(0) + options.Init.Or(0) +
			options.Main.Or(0) + options.FooBar.Or(0)*10 + options.FooBar_.Or(0)
	}
	got, err := baml_sdk.HostCallableTestsCallCallbackWithOptionalNameEdges(
		context.Background(),
		callback,
	)
	if err != nil || got != 117 {
		t.Fatalf("optional callback name edges = %d, %v; want 117", got, err)
	}
}

func Test_host_callable_class_argument(t *testing.T) {
	person := baml_sdk.HostCallableTestsPerson{Name: "Ada", Age: 37}
	got, err := baml_sdk.HostCallableTestsCallWithClassCallback(
		context.Background(),
		func(value baml_sdk.HostCallableTestsPerson) string {
			return fmt.Sprintf("%s:%d", value.Name, value.Age)
		},
		person,
	)
	if err != nil || got != "Ada:37" {
		t.Fatalf("class callback result = %q, %v", got, err)
	}
}

func Test_host_callable_structured_round_trips(t *testing.T) {
	ctx := context.Background()
	person := baml_sdk.HostCallableTestsPerson{Name: "Grace", Age: 45}
	gotPerson, err := baml_sdk.HostCallableTestsCallClassRoundtripCallback(ctx, func(value baml_sdk.HostCallableTestsPerson) baml_sdk.HostCallableTestsPerson {
		value.Age++
		return value
	}, person)
	if err != nil || gotPerson.Name != "Grace" || gotPerson.Age != 46 {
		t.Fatalf("class round trip = %#v, %v", gotPerson, err)
	}

	gotMood, err := baml_sdk.HostCallableTestsCallEnumRoundtripCallback(ctx, func(value baml_sdk.HostCallableTestsCallbackMood) baml_sdk.HostCallableTestsCallbackMood {
		if value == baml_sdk.HostCallableTestsCallbackMoodHAPPY {
			return baml_sdk.HostCallableTestsCallbackMoodSAD
		}
		return baml_sdk.HostCallableTestsCallbackMoodHAPPY
	}, baml_sdk.HostCallableTestsCallbackMoodHAPPY)
	if err != nil || gotMood != baml_sdk.HostCallableTestsCallbackMoodSAD {
		t.Fatalf("enum round trip = %q, %v", gotMood, err)
	}

	gotList, err := baml_sdk.HostCallableTestsCallListRoundtripCallback(ctx, func(values []int64) []int64 {
		return append(values, 3)
	}, []int64{1, 2})
	if err != nil || !reflect.DeepEqual(gotList, []int64{1, 2, 3}) {
		t.Fatalf("list round trip = %#v, %v", gotList, err)
	}

	gotMap, err := baml_sdk.HostCallableTestsCallMapRoundtripCallback(ctx, func(values map[string]int64) map[string]int64 {
		values["two"] = 2
		return values
	}, map[string]int64{"one": 1})
	if err != nil || !reflect.DeepEqual(gotMap, map[string]int64{"one": 1, "two": 2}) {
		t.Fatalf("map round trip = %#v, %v", gotMap, err)
	}
}

func Test_host_callable_closed_union_round_trips(t *testing.T) {
	ctx := context.Background()
	input := baml_sdk.NewStringOrIntFromInt(7)
	got, err := baml_sdk.HostCallableTestsCallUnionRoundtripCallback(ctx, func(value baml_sdk.StringOrInt) baml_sdk.StringOrInt {
		if integer, ok := value.AsInt(); !ok || integer != 7 {
			t.Fatalf("union callback input = %#v", value)
		}
		return baml_sdk.NewStringOrIntFromString("seven")
	}, input)
	if err != nil {
		t.Fatal(err)
	}
	if text, ok := got.AsString(); !ok || text != "seven" {
		t.Fatalf("union callback output = %#v", got)
	}

	nullable, err := baml_sdk.HostCallableTestsCallNullableUnionRoundtripCallback(ctx, func(value *baml_sdk.StringOrInt) *baml_sdk.StringOrInt {
		if value != nil {
			t.Fatalf("nullable union input = %#v; want nil", value)
		}
		result := baml_sdk.NewStringOrIntFromInt(11)
		return &result
	}, nil)
	if err != nil || nullable == nil {
		t.Fatalf("nullable union output = %#v, %v", nullable, err)
	}
	if integer, ok := nullable.AsInt(); !ok || integer != 11 {
		t.Fatalf("nullable union arm = %#v", nullable)
	}
	nullable, err = baml_sdk.HostCallableTestsCallNullableUnionRoundtripCallback(ctx, func(*baml_sdk.StringOrInt) *baml_sdk.StringOrInt {
		return nil
	}, nullable)
	if err != nil || nullable != nil {
		t.Fatalf("nullable union null return = %#v, %v", nullable, err)
	}
}

func Test_host_callable_closed_union_containers_and_nominal_arms(t *testing.T) {
	ctx := context.Background()
	listInput := []baml_sdk.StringOrInt{
		baml_sdk.NewStringOrIntFromString("one"),
		baml_sdk.NewStringOrIntFromInt(2),
	}
	list, err := baml_sdk.HostCallableTestsCallUnionListRoundtripCallback(ctx, func(values []baml_sdk.StringOrInt) []baml_sdk.StringOrInt {
		return append(values, baml_sdk.NewStringOrIntFromString("three"))
	}, listInput)
	if err != nil || len(list) != 3 {
		t.Fatalf("nested union list = %#v, %v", list, err)
	}
	if text, ok := list[2].AsString(); !ok || text != "three" {
		t.Fatalf("nested union list arm = %#v", list[2])
	}

	unionMap, err := baml_sdk.HostCallableTestsCallUnionMapRoundtripCallback(ctx, func(values map[string]baml_sdk.StringOrInt) map[string]baml_sdk.StringOrInt {
		values["answer"] = baml_sdk.NewStringOrIntFromInt(42)
		return values
	}, map[string]baml_sdk.StringOrInt{"label": baml_sdk.NewStringOrIntFromString("ok")})
	if err != nil {
		t.Fatal(err)
	}
	if integer, ok := unionMap["answer"].AsInt(); !ok || integer != 42 {
		t.Fatalf("nested union map arm = %#v", unionMap["answer"])
	}

	person := baml_sdk.NewStringOrHostCallableTestsPersonOrHostCallableTestsCallbackMoodFromHostCallableTestsPerson(
		baml_sdk.HostCallableTestsPerson{Name: "Ada", Age: 37},
	)
	nominal, err := baml_sdk.HostCallableTestsCallNominalUnionRoundtripCallback(ctx, func(value baml_sdk.StringOrHostCallableTestsPersonOrHostCallableTestsCallbackMood) baml_sdk.StringOrHostCallableTestsPersonOrHostCallableTestsCallbackMood {
		if gotPerson, ok := value.AsHostCallableTestsPerson(); !ok || gotPerson.Name != "Ada" {
			t.Fatalf("nominal union input = %#v", value)
		}
		return baml_sdk.NewStringOrHostCallableTestsPersonOrHostCallableTestsCallbackMoodFromHostCallableTestsCallbackMood(
			baml_sdk.HostCallableTestsCallbackMoodHAPPY,
		)
	}, person)
	if err != nil {
		t.Fatal(err)
	}
	if mood, ok := nominal.AsHostCallableTestsCallbackMood(); !ok || mood != baml_sdk.HostCallableTestsCallbackMoodHAPPY {
		t.Fatalf("nominal union output = %#v", nominal)
	}
}

func Test_host_callable_closed_union_literal_optional_and_selected_empty_container_arms(t *testing.T) {
	ctx := context.Background()
	literalInput := baml_sdk.NewIntOrStringLiteralcd322617OrStringLiteral6ca6c75cFromStringLiteralcd322617()
	literal, err := baml_sdk.HostCallableTestsCallLiteralUnionRoundtripCallback(ctx, func(value baml_sdk.IntOrStringLiteralcd322617OrStringLiteral6ca6c75c) baml_sdk.IntOrStringLiteralcd322617OrStringLiteral6ca6c75c {
		if text, ok := value.AsStringLiteralcd322617(); !ok || text != "first" {
			t.Fatalf("literal union input = %#v", value)
		}
		return baml_sdk.NewIntOrStringLiteralcd322617OrStringLiteral6ca6c75cFromInt(9)
	}, literalInput)
	if err != nil {
		t.Fatal(err)
	}
	if integer, ok := literal.AsInt(); !ok || integer != 9 {
		t.Fatalf("literal union output = %#v", literal)
	}

	states, err := baml_sdk.HostCallableTestsCallCallbackWithOptionalUnionStates(ctx, func(options baml_sdk.CallbackWithValueOptionalStringOrIntOptions) int64 {
		value, supplied := options.Value.Get()
		if !supplied {
			return 0
		}
		if value == nil {
			return 1
		}
		if _, ok := value.AsString(); ok {
			return 2
		}
		if _, ok := value.AsInt(); ok {
			return 3
		}
		return -1
	})
	if err != nil || !reflect.DeepEqual(states, []int64{0, 1, 2, 3}) {
		t.Fatalf("optional union states = %#v, %v", states, err)
	}

	emptyInts := baml_sdk.NewStringListOrIntListFromIntList([]int64{})
	emptyStrings, err := baml_sdk.HostCallableTestsCallOverlappingContainerUnionCallback(ctx, func(value baml_sdk.StringListOrIntList) baml_sdk.StringListOrIntList {
		if integers, ok := value.AsIntList(); !ok || len(integers) != 0 {
			t.Fatalf("selected empty int-list arm = %#v", value)
		}
		return baml_sdk.NewStringListOrIntListFromStringList([]string{})
	}, emptyInts)
	if err != nil {
		t.Fatal(err)
	}
	if strings, ok := emptyStrings.AsStringList(); !ok || len(strings) != 0 {
		t.Fatalf("selected empty string-list arm = %#v", emptyStrings)
	}
}

func Test_host_callable_media_round_trip(t *testing.T) {
	mime := "image/png"
	image, err := baml_go.NewImageFromUrl("https://example.com/callback.png", &mime)
	if err != nil {
		t.Fatal(err)
	}
	got, err := baml_sdk.HostCallableTestsCallMediaRoundtripCallback(context.Background(), func(value baml_go.Image) baml_go.Image {
		return value
	}, image)
	if err != nil {
		t.Fatal(err)
	}
	url, err := got.Url()
	if err != nil || url == nil || *url != "https://example.com/callback.png" {
		t.Fatalf("media callback URL = %#v, %v", url, err)
	}
}

func Test_host_callable_repeated_and_concurrent_reuse(t *testing.T) {
	ctx := context.Background()
	var calls atomic.Int64
	callback := func(value int64) string {
		calls.Add(1)
		return fmt.Sprintf("item-%d", value)
	}
	got, err := baml_sdk.HostCallableTestsCallRepeatedly(ctx, callback, 5)
	if err != nil || !reflect.DeepEqual(got, []string{"item-0", "item-1", "item-2", "item-3", "item-4"}) {
		t.Fatalf("repeated callback result = %#v, %v", got, err)
	}

	const workers = 8
	var wait sync.WaitGroup
	errorsSeen := make(chan error, workers)
	for index := 0; index < workers; index++ {
		wait.Add(1)
		go func(index int) {
			defer wait.Done()
			value, err := baml_sdk.HostCallableTestsCallWithCallback(ctx, callback, int64(index))
			if err != nil {
				errorsSeen <- err
				return
			}
			if want := fmt.Sprintf("item-%d", index); value != want {
				errorsSeen <- fmt.Errorf("result %q, want %q", value, want)
			}
		}(index)
	}
	wait.Wait()
	close(errorsSeen)
	for err := range errorsSeen {
		t.Error(err)
	}
}

func Test_host_callable_declared_throw_is_catchable(t *testing.T) {
	got, err := baml_sdk.HostCallableTestsCallWithThrowing(
		context.Background(),
		func(int64) (string, error) { return "", errors.New("callback failed") },
		1,
	)
	if err != nil || got != "caught:Error" {
		t.Fatalf("caught callback error = %q, %v", got, err)
	}
}

func Test_host_callable_panic_does_not_cross_cgo_boundary(t *testing.T) {
	_, err := baml_sdk.HostCallableTestsCallWithCallback(
		context.Background(),
		func(int64) string { panic("callback exploded") },
		1,
	)
	if err == nil {
		t.Fatal("panicking callback unexpectedly succeeded")
	}
}

func Test_host_callable_cancellation_while_dispatched(t *testing.T) {
	ctx, cancel := context.WithCancel(context.Background())
	started := make(chan struct{})
	release := make(chan struct{})
	done := make(chan error, 1)
	go func() {
		_, err := baml_sdk.HostCallableTestsCallWithCallback(ctx, func(int64) string {
			close(started)
			<-release
			return "late"
		}, 1)
		done <- err
	}()
	waitForCallbackStart(t, started, done)
	cancel()
	if err := <-done; !errors.Is(err, context.Canceled) {
		t.Fatalf("cancelled callback call returned %v", err)
	}
	close(release)
}

func Test_host_callable_void_signatures(t *testing.T) {
	var seen atomic.Int64
	got, err := baml_sdk.HostCallableTestsCallVoidCallback(context.Background(), func(value int64) {
		seen.Store(value)
	}, 42)
	if err != nil || got != 42 || seen.Load() != 42 {
		t.Fatalf("void callback = %d, seen %d, error %v", got, seen.Load(), err)
	}

	if err := baml_sdk.HostCallableTestsCallThrowingVoidCallback(context.Background(), func(int64) error { return nil }, 1); err != nil {
		t.Fatalf("throwing void callback success = %v", err)
	}
	if err := baml_sdk.HostCallableTestsCallThrowingVoidCallback(context.Background(), func(int64) error {
		return errors.New("void failed")
	}, 1); err == nil {
		t.Fatal("throwing void callback error unexpectedly succeeded")
	}
	if _, err := baml_sdk.HostCallableTestsCallVoidCallback(context.Background(), func(int64) {
		panic("void panic")
	}, 1); err == nil {
		t.Fatal("panicking void callback unexpectedly succeeded")
	}
}

func Test_nil_host_callable_fails_before_dispatch(t *testing.T) {
	var callback func(int64) string
	_, err := baml_sdk.HostCallableTestsIgnoreCallback(context.Background(), callback)
	if err == nil || !strings.Contains(err.Error(), `argument "callback"`) || !strings.Contains(err.Error(), "host callable is nil") {
		t.Fatalf("nil callback error = %v", err)
	}
}

func Test_host_callable_reentrant_call_does_not_deadlock(t *testing.T) {
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()
	got, err := baml_sdk.HostCallableTestsCallWithCallback(ctx, func(int64) string {
		inner, innerErr := baml_sdk.HelloWorld(ctx)
		if innerErr != nil {
			return "inner-error:" + innerErr.Error()
		}
		return inner
	}, 1)
	if err != nil || got != "hello world" {
		t.Fatalf("reentrant callback = %q, %v", got, err)
	}
}

func Test_host_callable_late_and_uncaught_failures_release_native_identity(t *testing.T) {
	const repetitions = 16
	var finalized atomic.Int64
	for index := 0; index < repetitions; index++ {
		_, err := baml_sdk.HostCallableTestsPropagateThrowingCallback(context.Background(), func(int64) (string, error) {
			return "", trackedCallbackError(&finalized)
		}, 1)
		if err == nil {
			t.Fatal("uncaught callback error unexpectedly succeeded")
		}
		_, err = baml_sdk.HostCallableTestsCallWithCallback(context.Background(), func(int64) string {
			panic(trackedPanicValue(&finalized))
		}, 1)
		if err == nil {
			t.Fatal("uncaught callback panic unexpectedly succeeded")
		}
	}
	waitForFinalizers(t, &finalized, 2*repetitions)

	for index := 0; index < repetitions; index++ {
		ctx, cancel := context.WithCancel(context.Background())
		started := make(chan struct{})
		release := make(chan struct{})
		done := make(chan error, 1)
		go func(panicLate bool) {
			if panicLate {
				_, err := baml_sdk.HostCallableTestsCallWithCallback(ctx, func(int64) string {
					close(started)
					<-release
					panic(trackedPanicValue(&finalized))
				}, 1)
				done <- err
				return
			}
			_, err := baml_sdk.HostCallableTestsPropagateThrowingCallback(ctx, func(int64) (string, error) {
				close(started)
				<-release
				return "", trackedCallbackError(&finalized)
			}, 1)
			done <- err
		}(index%2 == 1)
		waitForCallbackStart(t, started, done)
		cancel()
		if err := <-done; !errors.Is(err, context.Canceled) {
			t.Fatalf("late callback cancellation = %v", err)
		}
		close(release)
	}
	waitForFinalizers(t, &finalized, 3*repetitions)
}

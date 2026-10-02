package sdk_test

import (
	baml_sdk "baml.local/sdk/baml_sdk"
	"baml.local/sdk/baml_sdk/baml"
	"baml.local/sdk/baml_sdk/invocation"
	trace "baml.local/sdk/baml_sdk/packages/trace"
	"context"
	"fmt"
	baml_go "github.com/boundaryml/baml-go"
	"reflect"
	"testing"
	"time"
)

func Test_invocation_options(t *testing.T) {
	ctx := context.Background()
	t.Run("four_call_forms", func(t *testing.T) {
		controls := baml_sdk.WithOptions(baml_sdk.BamlOptions{Timeout: time.Second})
		seven := int64(7)
		for index, options := range [][]baml_sdk.CallOption{nil, {baml_sdk.WithOptionalArgsProbeOpt1(&seven)}, {controls}, {baml_sdk.WithOptionalArgsProbeOpt1(&seven), controls}} {
			got, err := baml_sdk.OptionalArgsProbe(ctx, 1, options...)
			want := int64(5)
			if index%2 == 1 {
				want = 7
			}
			if err != nil || len(got) != 3 || *got[0] != 1 || *got[1] != want || *got[2] != 99 {
				t.Fatalf("form %d: %v %v", index, got, err)
			}
		}
	})
	t.Run("empty_controls", func(t *testing.T) {
		for _, options := range [][]baml_sdk.CallOption{nil, {nil}, {baml_sdk.WithOptions(baml_sdk.BamlOptions{})}} {
			got, err := baml_sdk.HelloWorld(ctx, options...)
			if err != nil || got != "hello world" {
				t.Fatalf("%q %v", got, err)
			}
		}
	})
	t.Run("omitted_argument_is_not_null", func(t *testing.T) {
		got, err := baml_sdk.OptionalArgsProbe(ctx, 1, baml_sdk.WithOptionalArgsProbeOpt1(nil), baml_sdk.WithOptions(baml_sdk.BamlOptions{}))
		if err != nil || got[1] != nil {
			t.Fatalf("null optional: %v %v", got, err)
		}
	})
	t.Run("invalid_timeout_rejected", func(t *testing.T) {
		if _, err := baml_sdk.HelloWorld(ctx, baml_sdk.WithOptions(baml_sdk.BamlOptions{Timeout: -1})); err == nil {
			t.Fatal("negative timeout accepted")
		}
	})
	t.Run("duplicate_options_rejected_go_only", func(t *testing.T) {
		if _, err := baml_sdk.HelloWorld(ctx, baml_sdk.WithOptions(baml_sdk.BamlOptions{}), baml_sdk.WithOptions(baml_sdk.BamlOptions{})); err == nil {
			t.Fatal("two WithOptions accepted")
		}
	})
	t.Run("zero_duration_inherits_go_only", func(t *testing.T) {
		got, err := baml_sdk.HelloWorld(ctx, baml_sdk.WithOptions(baml_sdk.BamlOptions{Timeout: 0}))
		if err != nil || got != "hello world" {
			t.Fatalf("zero duration: %q %v", got, err)
		}
	})
}
func Test_invocation_surfaces(t *testing.T) {
	ctx := context.Background()
	controls := baml_sdk.WithOptions(baml_sdk.BamlOptions{Timeout: time.Second})
	t.Run("methods_accept_controls", func(t *testing.T) {
		value, err := baml_sdk.OptBoxMake(ctx, 1, controls)
		if err != nil {
			t.Fatal(err)
		}
		got, err := value.Probe(ctx, 2, controls)
		if err != nil || len(got) != 3 || *got[0] != 8 || *got[1] != 2 || *got[2] != 5 {
			t.Fatalf("method: %v %v", got, err)
		}
	})
	t.Run("returned_callable_accepts_controls", func(t *testing.T) {
		add, err := baml_sdk.HostCallableTestsMakeAdder(ctx, 3)
		if err != nil {
			t.Fatal(err)
		}
		if got := add.Call(ctx, 4, controls); got != 7 {
			t.Fatalf("returned callable: %d", got)
		}
	})
	t.Run("dynamic_call_accepts_controls", func(t *testing.T) {
		value, err := baml_sdk.Invoke(ctx, baml_sdk.NamedTarget("user.hello_world"), baml_sdk.Arguments{}, nil, controls)
		if err != nil {
			t.Fatal(err)
		}
		got, err := value.String()
		if err != nil || got != "hello world" {
			t.Fatalf("dynamic: %q %v", got, err)
		}
		add, err := baml_sdk.HostCallableTestsMakeAdder(ctx, 3)
		if err != nil {
			t.Fatal(err)
		}
		value, err = baml_sdk.Invoke(ctx, baml_sdk.FunctionTarget(add), baml_sdk.Arguments{"value": baml_go.Int64(4)}, nil, controls)
		if err != nil {
			t.Fatal(err)
		}
		integer, err := value.Int64()
		if err != nil || integer != 7 {
			t.Fatalf("dynamic callable: %d %v", integer, err)
		}
	})
	t.Run("specialized_callable_rejects_type_bindings", func(t *testing.T) {
		add, err := baml_sdk.HostCallableTestsMakeAdder(ctx, 3)
		if err != nil {
			t.Fatal(err)
		}
		_, err = baml_sdk.Invoke(ctx, baml_sdk.FunctionTarget(add), baml_sdk.Arguments{"value": baml_go.Int64(4)}, baml_sdk.TypeBindings{"T": baml_go.TypeOf[int64]()}, controls)
		if err == nil {
			t.Fatal("specialized callable accepted new bindings")
		}
	})
	t.Run("dynamic_application_map_preserves_control_like_keys", func(t *testing.T) {
		if _, err := baml_sdk.Invoke(ctx, baml_sdk.NamedTarget("user.hello_world"), baml_sdk.Arguments{"baml": baml_go.NullInput(baml_go.Null{})}, nil, controls); err == nil {
			t.Fatal("control-like argument was stripped")
		}
	})
	t.Run("dynamic_options_reject_argument_and_type_setters_go_only", func(t *testing.T) {
		for _, option := range []baml_sdk.CallOption{baml_go.WithArgument("bad", baml_go.Int64(1)), baml_go.WithTypeArg("T", baml_go.TypeOf[int64]())} {
			if _, err := baml_sdk.Invoke(ctx, baml_sdk.NamedTarget("user.hello_world"), nil, nil, option); err == nil {
				t.Fatal("dynamic options accepted another binding channel")
			}
		}
	})
}
func Test_invocation_lifecycle(t *testing.T) {
	ctx := context.Background()
	t.Run("pre_cancelled_call_does_not_enter_callback", func(t *testing.T) {
		token, err := baml.SpawnCancelTokenNew(ctx)
		if err != nil {
			t.Fatal(err)
		}
		if _, err := token.Cancel(ctx); err != nil {
			t.Fatal(err)
		}
		calls := 0
		_, err = baml_sdk.HostCallableTestsCallIntCallback(ctx, func(context.Context, int64) int64 { calls++; return 1 }, 1, baml_sdk.WithOptions(baml_sdk.BamlOptions{Cancel: &token}))
		if err == nil || calls != 0 {
			t.Fatalf("pre-cancel: calls=%d err=%v", calls, err)
		}
	})
	t.Run("composite_token_observes_every_source", func(t *testing.T) {
		for index := 0; index < 2; index++ {
			a, err := baml.SpawnCancelTokenNew(ctx)
			if err != nil {
				t.Fatal(err)
			}
			b, err := baml.SpawnCancelTokenNew(ctx)
			if err != nil {
				t.Fatal(err)
			}
			sources := []baml.SpawnCancelToken{a, b}
			combined, err := baml.SpawnCancelTokenAny(ctx, sources)
			if err != nil {
				t.Fatal(err)
			}
			if _, err := sources[index].Cancel(ctx); err != nil {
				t.Fatal(err)
			}
			got, err := combined.IsCancelled(ctx)
			if err != nil || !got {
				t.Fatalf("composite: %v %v", got, err)
			}
			got, err = sources[1-index].IsCancelled(ctx)
			if err != nil || got {
				t.Fatalf("sibling source: %v %v", got, err)
			}
			if _, err := baml_sdk.HelloWorld(ctx, baml_sdk.WithOptions(baml_sdk.BamlOptions{Cancel: &combined})); err == nil {
				t.Fatal("composite didn't cancel admission")
			}
		}
	})
	t.Run("child_cancellation_does_not_cancel_input", func(t *testing.T) {
		source, err := baml.SpawnCancelTokenNew(ctx)
		if err != nil {
			t.Fatal(err)
		}
		checked := make(chan error, 1)
		_, _ = baml_sdk.HostCallableTestsCallIntCallback(ctx, func(callbackContext context.Context, value int64) int64 {
			active := invocation.Current(callbackContext)
			if active == nil {
				panic("no active frame")
			}
			if _, err := active.Cancel.Cancel(callbackContext); err != nil {
				panic(err)
			}
			cancelled, err := active.Cancel.IsCancelled(callbackContext)
			if err != nil || !cancelled {
				checked <- fmt.Errorf("effective token: %v %v", cancelled, err)
			} else {
				checked <- nil
			}
			return value
		}, 1, baml_sdk.WithOptions(baml_sdk.BamlOptions{Cancel: &source}))
		select {
		case err := <-checked:
			if err != nil {
				t.Fatal(err)
			}
		case <-time.After(5 * time.Second):
			t.Fatal("callback did not verify effective cancellation")
		}
		cancelled, err := source.IsCancelled(ctx)
		if err != nil || cancelled {
			t.Fatalf("source token: %v %v", cancelled, err)
		}
	})
	t.Run("reservation_is_single_use", func(t *testing.T) {
		options, err := trace.Hidden(ctx)
		if err != nil {
			t.Fatal(err)
		}
		reserved, err := options.Reserve(ctx)
		if err != nil {
			t.Fatal(err)
		}
		controls := baml_sdk.WithOptions(baml_sdk.BamlOptions{Trace: reserved})
		if _, err := baml_sdk.HelloWorld(ctx, controls); err != nil {
			t.Fatal(err)
		}
		if _, err := baml_sdk.HelloWorld(ctx, controls); err == nil {
			t.Fatal("reservation attached twice")
		}
	})
	t.Run("failed_admission_does_not_consume_reservation", func(t *testing.T) {
		options, err := trace.Hidden(ctx)
		if err != nil {
			t.Fatal(err)
		}
		reserved, err := options.Reserve(ctx)
		if err != nil {
			t.Fatal(err)
		}
		source, err := baml.SpawnCancelTokenNew(ctx)
		if err != nil {
			t.Fatal(err)
		}
		if _, err := source.Cancel(ctx); err != nil {
			t.Fatal(err)
		}
		if _, err := baml_sdk.HelloWorld(ctx, baml_sdk.WithOptions(baml_sdk.BamlOptions{Trace: reserved, Cancel: &source})); err == nil {
			t.Fatal("pre-cancelled admission succeeded")
		}
		if _, err := baml_sdk.HelloWorld(ctx, baml_sdk.WithOptions(baml_sdk.BamlOptions{Trace: reserved})); err != nil {
			t.Fatal(err)
		}
	})
	t.Run("retained_effective_token_observes_late_parent_cancellation", func(t *testing.T) {
		source, err := baml.SpawnCancelTokenNew(ctx)
		if err != nil {
			t.Fatal(err)
		}
		var active *invocation.Invocation
		var captured context.Context
		_, err = baml_sdk.HostCallableTestsCallIntCallback(ctx, func(child context.Context, value int64) int64 {
			active = invocation.Current(child)
			captured = child
			return value
		}, 1, baml_sdk.WithOptions(baml_sdk.BamlOptions{Cancel: &source}))
		if err != nil || active == nil {
			t.Fatalf("capture: %v %v", active, err)
		}
		if _, err := source.Cancel(ctx); err != nil {
			t.Fatal(err)
		}
		cancelled, err := active.Cancel.IsCancelled(ctx)
		if err != nil || !cancelled {
			t.Fatalf("late cancellation: %v %v", cancelled, err)
		}
		if _, err := baml_sdk.HelloWorld(captured); err == nil {
			t.Fatal("retained frame lost inherited cancellation")
		}
	})
	t.Run("timeout_cancels_waiter_before_host_cleanup", func(t *testing.T) {
		exited := make(chan struct{})
		started := make(chan struct{})
		release := make(chan struct{})
		defer func() {
			close(release)
			select {
			case <-exited:
			case <-time.After(5 * time.Second):
				t.Error("callback cleanup stalled")
			}
		}()
		_, err := baml_sdk.HostCallableTestsCallIntCallback(ctx, func(child context.Context, value int64) int64 {
			close(started)
			<-release
			defer close(exited)
			return value
		}, 1, baml_sdk.WithOptions(baml_sdk.BamlOptions{Timeout: 100 * time.Millisecond}))
		if err == nil {
			t.Fatal("deadline ignored")
		}
		select {
		case <-started:
		default:
			t.Fatal("callback didn't start")
		}
		select {
		case <-exited:
			t.Fatal("waiter blocked on callback cleanup")
		default:
		}
	})
}
func Test_invocation_inheritance(t *testing.T) {
	ctx := context.Background()
	id := "A"
	parent, err := trace.Context_91819880(ctx, trace.WithContextDistinctId(&id), trace.WithContextMetadata(map[string]any{"outer": int64(1), "remove": "old", "shared": "A"}))
	if err != nil {
		t.Fatal(err)
	}
	id = "C"
	patch, err := trace.Context_91819880(ctx, trace.WithContextDistinctId(&id), trace.WithContextMetadata(map[string]any{"inner": int64(2), "remove": nil, "shared": "C"}))
	if err != nil {
		t.Fatal(err)
	}
	t.Run("callback_frame_is_installed_and_restored", func(t *testing.T) {
		if invocation.Current(ctx) != nil {
			t.Fatal("ambient frame outside dispatch")
		}
		_, err := baml_sdk.HostCallableTestsCallIntCallback(ctx, func(child context.Context, value int64) int64 {
			active := invocation.Current(child)
			if active == nil || active.Cancel == nil {
				panic("no active token")
			}
			current, err := trace.CurrentContext(child)
			if err != nil || current.DistinctId == nil || *current.DistinctId != "A" {
				panic(fmt.Sprintf("context: %#v %v", current, err))
			}
			fresh, err := trace.CurrentContext(context.Background())
			if err != nil || fresh.DistinctId != nil || len(fresh.Metadata) != 0 {
				panic("background context inherited")
			}
			return value
		}, 1, baml_sdk.WithOptions(baml_sdk.BamlOptions{Trace: parent}))
		if err != nil {
			t.Fatal(err)
		}
		if invocation.Current(ctx) != nil {
			t.Fatal("frame leaked")
		}
	})
	t.Run("multi_layer_context_patch_inherits_and_restores", func(t *testing.T) {
		for mask := 0; mask < 8; mask++ {
			t.Run(fmt.Sprintf("empty_controls_%d", mask), func(t *testing.T) {
				empty := func(bit int) []baml_sdk.CallOption {
					if mask&(1<<bit) != 0 {
						return []baml_sdk.CallOption{baml_sdk.WithOptions(baml_sdk.BamlOptions{})}
					}
					return nil
				}
				_, err := baml_sdk.HostCallableTestsCallIntCallback(ctx, func(a context.Context, value int64) int64 {
					_, err := baml_sdk.HostCallableTestsCallIntCallback(a, func(b context.Context, value int64) int64 {
						before, err := trace.CurrentContext(b)
						if err != nil || before.DistinctId == nil || *before.DistinctId != "A" {
							panic("B lost A")
						}
						_, err = baml_sdk.HostCallableTestsCallIntCallback(b, func(c context.Context, value int64) int64 {
							_, err := baml_sdk.HostCallableTestsCallIntCallback(c, func(d context.Context, value int64) int64 {
								current, err := trace.CurrentContext(d)
								want := map[string]any{"outer": int64(1), "inner": int64(2), "shared": "C"}
								if err != nil || current.DistinctId == nil || *current.DistinctId != "C" || !reflect.DeepEqual(current.Metadata, want) {
									panic(fmt.Sprintf("D context: %#v %v", current, err))
								}
								result := make(chan error, 1)
								go func() { _, err := baml_sdk.HelloWorld(d, empty(2)...); result <- err }()
								if err := <-result; err != nil {
									panic(err)
								}
								return value
							}, value, empty(1)...)
							if err != nil {
								panic(err)
							}
							return value
						}, value, baml_sdk.WithOptions(baml_sdk.BamlOptions{Trace: patch}))
						if err != nil {
							panic(err)
						}
						after, err := trace.CurrentContext(b)
						if err != nil || !reflect.DeepEqual(before, after) {
							panic("C escaped its frame")
						}
						return value
					}, value, empty(0)...)
					if err != nil {
						panic(err)
					}
					return value
				}, 1, baml_sdk.WithOptions(baml_sdk.BamlOptions{Trace: parent}))
				if err != nil {
					t.Fatal(err)
				}
			})
		}
	})
}

// SDK_PARITY_LINT(skip): Go cancellation controls remain usable with a canceled context
func Test_cancel_controls_with_cancelled_context_go_only(t *testing.T) {
	token, err := baml.SpawnCancelTokenNew(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	if _, err := token.Cancel(ctx); err != nil {
		t.Fatal(err)
	}
	cancelled, err := token.IsCancelled(ctx)
	if err != nil || !cancelled {
		t.Fatalf("cancelled token: %v %v", cancelled, err)
	}
}

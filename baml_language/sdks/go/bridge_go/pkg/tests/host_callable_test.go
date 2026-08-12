// host_callable_test.go — Go equivalent of
// `sdks/typescript/bridge_typescript/tests/host_callable.test.ts` and
// `sdks/python/tests/test_host_callable.py`.
//
// Exercises the Go host-callable bridge end-to-end: encoder auto-registration
// in `pkg.proto.goToInboundValueTracking` (reflective `reflect.Func` fallback), the C
// ABI round-trip via `bridge_cffi::complete_host_call`, the Go-side dispatch
// goroutine launched by `bamlHostDispatch`, and the result/error encode path.
//
// We do not need a generated SDK fixture — the tests build an in-memory BAML
// runtime with a small program declaring `(int) -> string` and `(int) -> int`
// callables.
//
// Race-detection: `go test -race ./pkg/tests/...` exercises the registry
// mutex and the goroutine-per-dispatch model.

package tests

import (
	"context"
	"errors"
	"fmt"
	"sync"
	"sync/atomic"
	"testing"
	"time"

	"bridge_go/pkg"
)

const hostCallableBamlSource = `
function CallCb(callback: (int) -> string, x: int) -> string {
    callback(x)
}

function CallIntCb(callback: (int) -> int, x: int) -> int {
    callback(x)
}
`

var (
	hostCallableRT       *pkg.BamlRuntime
	hostCallableRTSetup  sync.Once
	hostCallableRTSetupE error
)

func getHostCallableRuntime(t *testing.T) *pkg.BamlRuntime {
	t.Helper()
	hostCallableRTSetup.Do(func() {
		rt, err := pkg.NewRuntime(".", map[string]string{"main.baml": hostCallableBamlSource})
		if err != nil {
			hostCallableRTSetupE = err
			return
		}
		hostCallableRT = rt
	})
	if hostCallableRTSetupE != nil {
		t.Fatalf("NewRuntime failed: %v", hostCallableRTSetupE)
	}
	return hostCallableRT
}

// ---------------------------------------------------------------------------
// Simple round-trip: plain Go func returning a string
// ---------------------------------------------------------------------------

func TestHostCallableSimpleString(t *testing.T) {
	rt := getHostCallableRuntime(t)
	cb := func(x int64) string { return fmt.Sprintf("got %d", x) }
	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	defer cancel()
	out, err := rt.CallFunction(ctx, "CallCb", map[string]any{
		"callback": cb,
		"x":        int64(5),
	})
	if err != nil {
		t.Fatalf("CallFunction failed: %v", err)
	}
	if out != "got 5" {
		t.Fatalf("expected 'got 5', got %v (%T)", out, out)
	}
}

// ---------------------------------------------------------------------------
// int-returning callback
// ---------------------------------------------------------------------------

func TestHostCallableInt(t *testing.T) {
	rt := getHostCallableRuntime(t)
	cb := func(x int64) int64 { return x + 1 }
	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	defer cancel()
	out, err := rt.CallFunction(ctx, "CallIntCb", map[string]any{
		"callback": cb,
		"x":        int64(41),
	})
	if err != nil {
		t.Fatalf("CallFunction failed: %v", err)
	}
	if out != int64(42) {
		t.Fatalf("expected 42, got %v (%T)", out, out)
	}
}

// ---------------------------------------------------------------------------
// Native Go `int` (not just int64) as the param type — exercises the
// reflective coerceToType numeric widening.
// ---------------------------------------------------------------------------

func TestHostCallableIntParamWidens(t *testing.T) {
	rt := getHostCallableRuntime(t)
	cb := func(x int) string { return fmt.Sprintf("native %d", x) }
	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	defer cancel()
	out, err := rt.CallFunction(ctx, "CallCb", map[string]any{
		"callback": cb,
		"x":        int64(7),
	})
	if err != nil {
		t.Fatalf("CallFunction failed: %v", err)
	}
	if out != "native 7" {
		t.Fatalf("expected 'native 7', got %v", out)
	}
}

// ---------------------------------------------------------------------------
// Multiple distinct callables → distinct registry entries (no key collision)
// ---------------------------------------------------------------------------

func TestHostCallableDistinctClosures(t *testing.T) {
	rt := getHostCallableRuntime(t)
	seenA := int64(0)
	seenB := int64(0)
	cbA := func(x int64) string {
		atomic.AddInt64(&seenA, 1)
		return fmt.Sprintf("a:%d", x)
	}
	cbB := func(x int64) string {
		atomic.AddInt64(&seenB, 1)
		return fmt.Sprintf("b:%d", x)
	}
	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	defer cancel()
	a, err := rt.CallFunction(ctx, "CallCb", map[string]any{"callback": cbA, "x": int64(1)})
	if err != nil {
		t.Fatalf("CallFunction(a): %v", err)
	}
	if a != "a:1" {
		t.Fatalf("expected 'a:1', got %v", a)
	}
	b, err := rt.CallFunction(ctx, "CallCb", map[string]any{"callback": cbB, "x": int64(2)})
	if err != nil {
		t.Fatalf("CallFunction(b): %v", err)
	}
	if b != "b:2" {
		t.Fatalf("expected 'b:2', got %v", b)
	}
	if atomic.LoadInt64(&seenA) != 1 || atomic.LoadInt64(&seenB) != 1 {
		t.Fatalf("expected each callback to fire once, got a=%d b=%d", seenA, seenB)
	}
}

// ---------------------------------------------------------------------------
// Panic inside the callback surfaces as a BAML error containing the message.
// ---------------------------------------------------------------------------

func TestHostCallablePanicSurfacesAsError(t *testing.T) {
	rt := getHostCallableRuntime(t)
	cb := func(_ int64) string {
		panic("boom")
	}
	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	defer cancel()
	_, err := rt.CallFunction(ctx, "CallCb", map[string]any{
		"callback": cb,
		"x":        int64(1),
	})
	if err == nil {
		t.Fatal("expected error from panicking callback")
	}
	if !contains(err.Error(), "boom") {
		t.Fatalf("expected error to contain 'boom', got %v", err)
	}
}

// ---------------------------------------------------------------------------
// `(value, error)` return shape: a non-nil error surfaces as a BAML error.
// ---------------------------------------------------------------------------

func TestHostCallableReturnedErrorSurfaces(t *testing.T) {
	rt := getHostCallableRuntime(t)
	expected := errors.New("explicit failure")
	cb := func(_ int64) (string, error) {
		return "", expected
	}
	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	defer cancel()
	_, err := rt.CallFunction(ctx, "CallCb", map[string]any{
		"callback": cb,
		"x":        int64(0),
	})
	if err == nil {
		t.Fatal("expected error from callback returning non-nil error")
	}
	if err != expected {
		t.Fatalf("expected the original Go error by identity, got %T: %v", err, err)
	}
}

// ---------------------------------------------------------------------------
// `(value, error)` where error is nil: result flows through normally.
// ---------------------------------------------------------------------------

func TestHostCallableReturnedNilErrorOK(t *testing.T) {
	rt := getHostCallableRuntime(t)
	cb := func(x int64) (string, error) {
		return fmt.Sprintf("ok-%d", x), nil
	}
	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	defer cancel()
	out, err := rt.CallFunction(ctx, "CallCb", map[string]any{
		"callback": cb,
		"x":        int64(3),
	})
	if err != nil {
		t.Fatalf("CallFunction failed: %v", err)
	}
	if out != "ok-3" {
		t.Fatalf("expected 'ok-3', got %v", out)
	}
}

// ---------------------------------------------------------------------------
// Concurrent invocations: the same callable invoked from many goroutines in
// parallel. Each dispatch runs on its own goroutine launched by
// `bamlHostDispatch`; the registry mutex must keep them race-free.
// ---------------------------------------------------------------------------

func TestHostCallableConcurrent(t *testing.T) {
	rt := getHostCallableRuntime(t)
	var concurrent int64
	var maxConcurrent int64
	cb := func(x int64) string {
		c := atomic.AddInt64(&concurrent, 1)
		// Bump the high-water mark; don't worry about exact concurrency in
		// the assertion because `go test -race` only needs the calls to
		// happen in parallel safely, not provably overlapping.
		for {
			m := atomic.LoadInt64(&maxConcurrent)
			if c <= m || atomic.CompareAndSwapInt64(&maxConcurrent, m, c) {
				break
			}
		}
		time.Sleep(2 * time.Millisecond)
		atomic.AddInt64(&concurrent, -1)
		return fmt.Sprintf("n%d", x)
	}

	const N = 8
	results := make([]any, N)
	errs := make([]error, N)
	var wg sync.WaitGroup
	for i := 0; i < N; i++ {
		wg.Add(1)
		go func(i int) {
			defer wg.Done()
			ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
			defer cancel()
			out, err := rt.CallFunction(ctx, "CallCb", map[string]any{
				"callback": cb,
				"x":        int64(i),
			})
			results[i] = out
			errs[i] = err
		}(i)
	}
	wg.Wait()
	for i, err := range errs {
		if err != nil {
			t.Errorf("invocation %d failed: %v", i, err)
		}
	}
	for i, r := range results {
		want := fmt.Sprintf("n%d", i)
		if r != want {
			t.Errorf("invocation %d: expected %q, got %v", i, want, r)
		}
	}
	// At least one invocation should have happened — we don't assert
	// > 1 to avoid flakiness on single-core or heavily-loaded CI.
	if atomic.LoadInt64(&maxConcurrent) < 1 {
		t.Fatalf("expected at least 1 concurrent invocation, got %d", maxConcurrent)
	}
}

// ---------------------------------------------------------------------------
// Release path: invoking the callable once and then dropping the runtime's
// reference must cause the registry entry to drop. We don't have a direct
// way to observe `bamlHostRelease` here (it fires inside Rust's Arc-drop)
// without a hook; the smoke-test below confirms the runtime tears down
// cleanly after many host-call invocations.
// ---------------------------------------------------------------------------

func TestHostCallableSmokeMany(t *testing.T) {
	rt := getHostCallableRuntime(t)
	for i := 0; i < 16; i++ {
		x := int64(i)
		cb := func(v int64) string { return fmt.Sprintf("%d", v*2) }
		ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
		out, err := rt.CallFunction(ctx, "CallCb", map[string]any{
			"callback": cb,
			"x":        x,
		})
		cancel()
		if err != nil {
			t.Fatalf("iter %d: %v", i, err)
		}
		want := fmt.Sprintf("%d", x*2)
		if out != want {
			t.Fatalf("iter %d: expected %q, got %v", i, want, out)
		}
	}
}

// ---------------------------------------------------------------------------
// An out-of-range integer arg into a narrow-typed callable must surface
// as an invalid-argument BAML error, NOT a silently truncated value.
//
// The BAML side passes `x` as an int; the Go callback declares `x int8`.
// `coerceToType` range-checks before narrowing, so `300` (which would
// truncate to `44`) is rejected and the dispatch path emits a
// HOST_CALLABLE_INVALID_ARGUMENT error instead of invoking the callback.
// ---------------------------------------------------------------------------

func TestHostCallableNarrowingOutOfRangeRejected(t *testing.T) {
	rt := getHostCallableRuntime(t)
	called := int64(0)
	cb := func(x int8) string {
		atomic.AddInt64(&called, 1)
		return fmt.Sprintf("got %d", x)
	}
	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	defer cancel()
	_, err := rt.CallFunction(ctx, "CallCb", map[string]any{
		"callback": cb,
		"x":        int64(300), // out of int8 range; truncating would give 44
	})
	if err == nil {
		t.Fatal("expected an error for out-of-range int8 arg, got success")
	}
	if atomic.LoadInt64(&called) != 0 {
		t.Fatalf("callback should not have been invoked with truncated data, but ran %d times", called)
	}
}

// In-range narrowing still works: `5` fits int8, so the callback runs.
func TestHostCallableNarrowingInRangeOK(t *testing.T) {
	rt := getHostCallableRuntime(t)
	cb := func(x int8) string { return fmt.Sprintf("got %d", x) }
	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	defer cancel()
	out, err := rt.CallFunction(ctx, "CallCb", map[string]any{
		"callback": cb,
		"x":        int64(5),
	})
	if err != nil {
		t.Fatalf("CallFunction failed: %v", err)
	}
	if out != "got 5" {
		t.Fatalf("expected 'got 5', got %v", out)
	}
}

// A negative arg into an unsigned param is also out of range.
func TestHostCallableNarrowingNegativeIntoUnsigned(t *testing.T) {
	rt := getHostCallableRuntime(t)
	cb := func(x uint8) string { return fmt.Sprintf("got %d", x) }
	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	defer cancel()
	_, err := rt.CallFunction(ctx, "CallCb", map[string]any{
		"callback": cb,
		"x":        int64(-1),
	})
	if err == nil {
		t.Fatal("expected an error for negative arg into uint8 param")
	}
}

func contains(haystack, needle string) bool {
	for i := 0; i+len(needle) <= len(haystack); i++ {
		if haystack[i:i+len(needle)] == needle {
			return true
		}
	}
	return false
}

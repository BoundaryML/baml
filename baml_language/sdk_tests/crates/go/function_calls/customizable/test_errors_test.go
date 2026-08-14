package sdk_test

import (
	"context"
	"errors"
	"fmt"
	"os"
	"os/exec"
	"reflect"
	"strconv"
	"strings"
	"testing"
	"time"

	"baml.local/sdk/baml_sdk"
	"github.com/boundaryml/baml-go"
)

const badJSON = "{not valid json"

func initializeGeneratedRuntime(t *testing.T) {
	t.Helper()
	if _, err := baml_sdk.HelloWorld(context.Background()); err != nil {
		t.Fatalf("initialize generated BAML runtime: %v", err)
	}
}

// Direct Go counterpart to Python test_stdlib_error_surfaces_as_baml_error.
func Test_stdlib_error_surfaces_as_go_error(t *testing.T) {
	_, err := baml_sdk.ThrowsTestParseJson(context.Background(), badJSON)
	assertErrorContains(t, err, "BAML error", "baml.json.JsonParseError")
}

func Test_parse_json_successful_value_uses_generated_json_projection(t *testing.T) {
	got, err := baml_sdk.ThrowsTestParseJson(context.Background(), `{"name":"Ada","items":[null,true,7,1.5]}`)
	want := map[string]any{"name": "Ada", "items": []any{nil, true, int64(7), 1.5}}
	if err != nil || !reflect.DeepEqual(got, want) {
		t.Fatalf("ParseJson = %#v, %v; want %#v", got, err, want)
	}
}

// Direct Go counterpart to Python test_user_throw_surfaces_declared_instance.
// Go currently exposes the declared class identity in Error(), not a decoded
// structured error value.
func Test_user_throw_surfaces_declared_class_identity(t *testing.T) {
	_, err := baml_sdk.ThrowsTestThrowMyError(context.Background())
	assertErrorContains(t, err, "BAML error", "user.throws_test.MyError")
}

// Direct Go counterpart to Python test_union_throws_preserves_class_name.
func Test_union_throws_preserves_concrete_class_identity(t *testing.T) {
	_, single := baml_sdk.RaisesTestReparse(context.Background(), "x")
	_, union := baml_sdk.RaisesTestLoadDoc(context.Background(), "x")
	assertErrorContains(t, single, "BAML error", "user.raises_test.ParseError")
	assertErrorContains(t, union, "BAML error", "user.raises_test.ParseError")
}

// Direct Go counterpart to Python
// test_host_invalid_argument_wraps_baml_errors_invalid_argument. Generated Go
// signatures prevent extra arguments statically, so use the low-level call to
// prove the BAML-owned relation reports the canonical runtime error.
func Test_invalid_function_arguments_surface_baml_error(t *testing.T) {
	initializeGeneratedRuntime(t)
	_, err := baml_go.Call(context.Background(), "user.hello_world", map[string]baml_go.Input{
		"not_a_param": baml_go.Int64(2),
	})
	assertErrorContains(t, err, "BAML error", "baml.errors.InvalidArgument")
}

// Direct Go counterpart to Python test_user_panic_surfaces_as_baml_panic.
// Go intentionally returns a plain error and never raises a Go panic.
func Test_user_panic_surfaces_as_go_error_without_panicking(t *testing.T) {
	err := baml_sdk.ThrowsTestDoPanic(context.Background(), "user-initiated boom")
	assertErrorContains(t, err, "BAML panic", "baml.panics.UserPanic", "user-initiated boom")
}

// Direct synchronous Go counterpart to Python
// test_cancellation_surfaces_as_baml_panic. Go cancellation preserves the
// context error identity rather than exposing the runtime panic envelope.
func Test_error_call_cancellation_preserves_context_identity(t *testing.T) {
	ctx, cancel := context.WithCancel(context.Background())
	timer := time.AfterFunc(50*time.Millisecond, cancel)
	defer timer.Stop()
	_, err := baml_sdk.ThrowsTestSleepMs(ctx, 2000)
	if err != ctx.Err() {
		t.Fatalf("cancellation error identity = %v, want exact ctx.Err() %v", err, ctx.Err())
	}
}

// Direct Go counterpart to Python test_str_is_non_empty.
func Test_error_string_is_non_empty(t *testing.T) {
	_, err := baml_sdk.ThrowsTestThrowMyError(context.Background())
	if err == nil || err.Error() == "" {
		t.Fatalf("error = %v", err)
	}
}

// Direct Go counterpart to Python test_baml_error_carries_baml_trace.
func Test_baml_error_carries_baml_trace(t *testing.T) {
	_, err := baml_sdk.ThrowsTestThrowMyError(context.Background())
	assertErrorContains(t, err, "user.throws_test.ThrowMyError", "types.baml")
}

// Python's traceback-object splicing is host-specific. Go honestly embeds
// every BAML frame in the returned error string; it does not synthesize Go
// runtime stack frames.
func Test_baml_trace_is_embedded_in_go_error_string(t *testing.T) {
	initializeGeneratedRuntime(t)
	_, err := baml_go.Call(context.Background(), "user.throws_test.ParseJson", map[string]baml_go.Input{
		"s": baml_go.String(badJSON),
	})
	assertErrorContains(t, err, "types.baml", "user.throws_test.ParseJson")
}

// Direct subprocess-safe Go counterpart to Python
// test_clean_exit_terminates_process_with_code. The helper is the only process
// that invokes the generated exit function; the main test runner never does.
func Test_clean_exit_terminates_process_with_code(t *testing.T) {
	for _, code := range []int{0, 7} {
		t.Run(strconv.Itoa(code), func(t *testing.T) {
			command := exec.Command(os.Args[0], "-test.run=^Test_clean_exit_helper_process$")
			command.Env = append(os.Environ(), fmt.Sprintf("BAML_GO_EXIT_HELPER=%d", code))
			output, err := command.CombinedOutput()
			if code == 0 {
				if err != nil {
					t.Fatalf("exit(0) failed: %v\n%s", err, output)
				}
			} else {
				var exitError *exec.ExitError
				if !errors.As(err, &exitError) || exitError.ExitCode() != code {
					t.Fatalf("exit(%d) = %v\n%s", code, err, output)
				}
			}
			if strings.Contains(string(output), "UNREACHABLE") {
				t.Fatalf("exit(%d) returned to Go:\n%s", code, output)
			}
		})
	}
}

func Test_clean_exit_helper_process(t *testing.T) {
	raw := os.Getenv("BAML_GO_EXIT_HELPER")
	if raw == "" {
		return
	}
	code, err := strconv.Atoi(raw)
	if err != nil {
		t.Fatal(err)
	}
	_ = baml_sdk.ThrowsTestDoExit(context.Background(), int64(code))
	fmt.Print("UNREACHABLE")
}

func assertErrorContains(t *testing.T, err error, fragments ...string) {
	t.Helper()
	if err == nil {
		t.Fatal("expected Go error")
	}
	for _, fragment := range fragments {
		if !strings.Contains(err.Error(), fragment) {
			t.Fatalf("error %q missing %q", err, fragment)
		}
	}
}

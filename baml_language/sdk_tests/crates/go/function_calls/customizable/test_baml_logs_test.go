package sdk_test

import (
	"bytes"
	"context"
	"os"
	"os/exec"
	"strings"
	"testing"

	"baml.local/sdk/baml_sdk"
)

// SDK_PARITY_LINT(skip): requires subprocess-level SDK harness support
func Test_baml_log_env_var_streams_logs_to_stderr(t *testing.T) {
	t.Run("baml_log_env_var_streams_logs_to_stderr", func(t *testing.T) {
		stderr := runEmitLogsChild(t, "BAML_LOG=info")
		for _, want := range []string{
			"[INFO] info go-log-marker",
			"[WARN] warn go-log-marker",
			"[ERROR] error go-log-marker",
		} {
			if !strings.Contains(stderr, want) {
				t.Errorf("child stderr missing %q:\n%s", want, stderr)
			}
		}
		if strings.Contains(stderr, "debug go-log-marker") {
			t.Errorf("child stderr leaked a debug log below the info threshold:\n%s", stderr)
		}
	})
}

// SDK_PARITY_LINT(skip): requires subprocess-level SDK harness support
func Test_baml_logs_stay_off_without_baml_log(t *testing.T) {
	t.Run("baml_logs_stay_off_without_baml_log", func(t *testing.T) {
		stderr := runEmitLogsChild(t, "BAML_LOG=")
		if strings.Contains(stderr, "go-log-marker") {
			t.Errorf("child stderr contains BAML logs without BAML_LOG set:\n%s", stderr)
		}
	})
}

// The child re-runs this test binary with BAML_GO_LOG_SINK_CHILD set, calls
// emit_logs, and exits; captured BAML logs land on the child's stderr.
// SDK_PARITY_LINT(skip): child-process entry point for the BAML_LOG stderr tests
func Test_baml_log_sink_child(t *testing.T) {
	if os.Getenv("BAML_GO_LOG_SINK_CHILD") == "" {
		t.Skip("child-process entry point; driven by the BAML_LOG stderr tests")
	}
	got, err := baml_sdk.EmitLogs(context.Background(), "go-log-marker")
	if err != nil || got != "go-log-marker" {
		t.Fatalf("EmitLogs() = %q, %v", got, err)
	}
}

func runEmitLogsChild(t *testing.T, bamlLogEnv string) string {
	t.Helper()
	command := exec.Command(os.Args[0], "-test.run=^Test_baml_log_sink_child$")
	command.Env = append(os.Environ(), "BAML_GO_LOG_SINK_CHILD=1", bamlLogEnv)
	var stderr bytes.Buffer
	command.Stderr = &stderr
	if output, err := command.Output(); err != nil {
		t.Fatalf("child failed: %v\nstdout:\n%s\nstderr:\n%s", err, output, stderr.String())
	}
	return stderr.String()
}

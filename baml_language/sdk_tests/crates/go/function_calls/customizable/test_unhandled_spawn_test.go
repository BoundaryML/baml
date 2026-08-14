package sdk_test

import (
	"context"
	"os"
	"os/exec"
	"strings"
	"testing"
	"time"

	"baml.local/sdk/baml_sdk"
	"github.com/boundaryml/baml-go"
)

// SDK_PARITY_LINT(skip): requires subprocess-level SDK harness support
func Test_unhandled_spawn_error_uses_host_default(t *testing.T) {
	t.Run("unhandled_spawn_error_uses_host_default", func(t *testing.T) {
		testUnhandledSpawnErrorUsesHostDefault(t)
	})
}

func testUnhandledSpawnErrorUsesHostDefault(t *testing.T) {
	if os.Getenv("BAML_GO_UNHANDLED_SPAWN_CHILD") != "" {
		got, err := baml_sdk.SpawnUnhandledError(context.Background())
		if err != nil || got != 1 {
			t.Fatalf("SpawnUnhandledError() = %d, %v", got, err)
		}
		if err := baml_go.Shutdown(); err != nil {
			t.Fatal(err)
		}
		time.Sleep(time.Second)
		t.Fatal("unhandled spawn error did not panic")
	}

	command := exec.Command(os.Args[0], "-test.run=^Test_unhandled_spawn_error_uses_host_default$")
	command.Env = append(os.Environ(), "BAML_GO_UNHANDLED_SPAWN_CHILD=1")
	output, err := command.CombinedOutput()
	if err == nil {
		t.Fatalf("child succeeded:\n%s", output)
	}
	if !strings.Contains(string(output), "user.unhandled_spawn_error") {
		t.Fatalf("child output missing spawn error:\n%s", output)
	}
}

package baml

import (
	"context"
	"fmt"
	"sync"
	"testing"
	"time"
)

// TestIssue4641ConcurrentClientCacheEnvOverrides reproduces
// https://github.com/BoundaryML/baml/issues/4641.
//
// Every request changes an environment variable referenced by the named client,
// forcing the runtime to invalidate that client's cached provider. Concurrent
// invalidation used to race the contains_key/get pair in get_llm_provider_impl,
// panic a Tokio task, and strand the corresponding Go callback.
func TestIssue4641ConcurrentClientCacheEnvOverrides(t *testing.T) {
	if testing.Short() {
		t.Skip("concurrent client-cache stress test")
	}

	const (
		workers           = 64
		requestsPerWorker = 2_000
		testTimeout       = 45 * time.Second
	)

	source := map[string]string{
		"main.baml": `
client<llm> Local {
  provider openai-generic
  options {
    base_url "http://127.0.0.1:1/v1"
    model "mock"
    api_key "test"
    headers {
      "x-session-tag" env.SESSION_TAG
    }
  }
}

function Ping(input: string) -> string {
  client Local
  prompt #"{{ input }}"#
}
`,
	}

	runtime, err := CreateRuntime(".", source, map[string]string{"SESSION_TAG": "initial"})
	if err != nil {
		t.Fatalf("create runtime: %v", err)
	}

	ctx, cancel := context.WithTimeout(context.Background(), testTimeout)
	defer cancel()

	start := make(chan struct{})
	errors := make(chan error, workers)
	var wg sync.WaitGroup
	for worker := 0; worker < workers; worker++ {
		wg.Add(1)
		go func(worker int) {
			defer wg.Done()
			<-start

			for request := 0; request < requestsPerWorker; request++ {
				args := BamlFunctionArguments{
					Kwargs: map[string]any{
						"input":  "hello",
						"stream": false,
					},
					Env: map[string]string{
						"SESSION_TAG": fmt.Sprintf("worker-%d-request-%d", worker, request),
					},
				}
				encoded, err := args.Encode()
				if err != nil {
					errors <- fmt.Errorf("worker %d request %d encode: %w", worker, request, err)
					return
				}

				if _, err := runtime.BuildRequest(ctx, "Ping", encoded); err != nil {
					errors <- fmt.Errorf("worker %d request %d build request: %w", worker, request, err)
					return
				}
			}
		}(worker)
	}

	close(start)
	done := make(chan struct{})
	go func() {
		wg.Wait()
		close(done)
	}()

	select {
	case <-done:
	case <-ctx.Done():
		t.Fatalf("concurrent BuildRequest calls did not complete within %s: %v", testTimeout, ctx.Err())
	}

	close(errors)
	for err := range errors {
		t.Error(err)
	}
}

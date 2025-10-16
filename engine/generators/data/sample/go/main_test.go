package main

import (
	"context"
	"fmt"
	"strings"
	"time"

	b "sample/baml_client"
	"testing"

	baml "github.com/boundaryml/baml/engine/language_client_go/pkg"
	"github.com/tidwall/gjson"
)

func getOnTick() (baml.TickCallback, *string, *int) {
	var lastThinking string
	var tickCount int
	onTick := func(ctx context.Context, reason baml.TickReason, log baml.FunctionLog) baml.FunctionSignal {
		tickCount++
		calls, err := log.Calls()
		if err != nil {
			fmt.Println("Error getting selected call: ", err)
			return nil
		}

		if len(calls) == 0 {
			fmt.Println("No calls found")
			return nil
		}

		selectedCall := calls[len(calls)-1]

		if as_stream, ok := selectedCall.(baml.LLMStreamCall); ok {
			stream, err := as_stream.SSEChunks()
			if err != nil {
				fmt.Println("Error getting stream: ", err)
				return nil
			}
			fmt.Println("Tick: ", len(stream), " chunks")
			accumulated := ""
			for _, chunk := range stream {
				text, err := chunk.Text()
				if err != nil {
					// this is an openai stream
				} else {
					if content := gjson.Get(text, "delta.thinking"); content.Exists() {
						accumulated += content.String()
					}
				}
			}
			fmt.Println("Accumulated: ", accumulated)
			lastThinking = accumulated
			fmt.Println("--------------------------------")
		} else {
			fmt.Println("Response is not a stream: ", selectedCall)
		}

		return nil
	}

	return onTick, &lastThinking, &tickCount
}

func TestOnTickRequest(t *testing.T) {
	ctx := context.Background()

	onTick, lastThinking, tickCount := getOnTick()

	_, err := b.Foo(ctx, 8192, b.WithExperimentalOnTick(onTick))
	if err != nil {
		t.Fatalf("Error in Foo: %v", err)
	}

	if *tickCount < 10 {
		t.Errorf("Expected more than 10 ticks, got %d", *tickCount)
	}

	if *lastThinking == "" {
		t.Errorf("Expected thinking, got %s", *lastThinking)
	}
}

func TestOnTickStream(t *testing.T) {
	ctx := context.Background()

	onTick, lastThinking, tickCount := getOnTick()

	result, err := b.Stream.Foo(ctx, 8192, b.WithOnTick(onTick))
	if err != nil {
		t.Fatalf("Error in Foo: %v", err)
	}
	for result := range result {
		if result.IsError {
			t.Fatalf("Error in Foo: %v", result.Error)
		} else if result.IsFinal {
			final := result.Final()
			fmt.Println("final", final)
		} else {
			fmt.Printf("Stream: %+v\n", result.Stream())
		}
	}

	if *tickCount < 10 {
		t.Errorf("Expected more than 10 ticks, got %d", *tickCount)
	}

	if *lastThinking == "" {
		t.Errorf("Expected thinking, got %s", *lastThinking)
	}
}

func TestFoo(t *testing.T) {
	t.Parallel()
	ctx := context.Background()
	collector, err := b.NewCollector("test-foo-collector")
	if err != nil {
		t.Fatalf("Error creating collector: %v", err)
	}

	result, err := b.Foo(ctx, 8192, b.WithCollector(collector))
	if err != nil {
		t.Fatalf("Error in Foo: %v", err)
	}

	// Basic validation - check if the union has a valid variant
	if result.BamlTypeName() == "" {
		t.Errorf("Expected valid result from Foo")
	}

	// Test comprehensive collector API
	testCollectorAPI(t, collector, "Foo")
}

func TestBar(t *testing.T) {
	t.Parallel()
	ctx := context.Background()
	collector, err := b.NewCollector("test-bar-collector")
	if err != nil {
		t.Fatalf("Error creating collector: %v", err)
	}

	result, err := b.Bar(ctx, 42, b.WithCollector(collector))
	if err != nil {
		t.Fatalf("Error in Bar: %v", err)
	}

	// Basic validation - check if the union has a valid variant
	if result.BamlTypeName() == "" {
		t.Errorf("Expected valid result from Bar")
	}

	// Test comprehensive collector API
	testCollectorAPI(t, collector, "Bar")
}

func TestFooStream(t *testing.T) {
	t.Parallel()
	ctx := context.Background()
	collector, err := b.NewCollector("test-foo-stream-collector")
	if err != nil {
		t.Fatalf("Error creating collector: %v", err)
	}

	channel, err := b.Stream.Foo(ctx, 8192, b.WithCollector(collector))
	if err != nil {
		t.Fatalf("Error starting Foo stream: %v", err)
	}

	gotFinal := false
	streamCount := 0

	for result := range channel {
		if result.IsFinal {
			gotFinal = true
			final := result.Final()
			if final == nil {
				t.Errorf("Expected non-nil final result")
			}
		} else {
			streamCount++
			stream := result.Stream()
			if stream == nil {
				t.Errorf("Expected non-nil stream result")
			}
		}
	}

	if !gotFinal {
		t.Errorf("Expected to receive a final result from stream")
	}

	t.Logf("Received %d stream chunks before final result", streamCount)

	// Test comprehensive collector API for streaming
	testCollectorAPI(t, collector, "Foo")
}

func TestBarStream(t *testing.T) {
	t.Parallel()
	ctx := context.Background()
	collector, err := b.NewCollector("test-bar-stream-collector")
	if err != nil {
		t.Fatalf("Error creating collector: %v", err)
	}

	channel, err := b.Stream.Bar(ctx, 99, b.WithCollector(collector))
	if err != nil {
		t.Fatalf("Error starting Bar stream: %v", err)
	}

	gotFinal := false
	streamCount := 0

	for result := range channel {
		if result.IsFinal {
			gotFinal = true
			final := result.Final()
			if final == nil {
				t.Errorf("Expected non-nil final result")
			}
		} else {
			streamCount++
			stream := result.Stream()
			if stream == nil {
				t.Errorf("Expected non-nil stream result")
			}
		}
	}

	if !gotFinal {
		t.Errorf("Expected to receive a final result from stream")
	}

	t.Logf("Received %d stream chunks before final result", streamCount)

	// Test comprehensive collector API for streaming
	testCollectorAPI(t, collector, "Bar")

	t.Logf("Collector: %+v", collector)
	count, err := collector.Clear()
	if err != nil {
		t.Fatalf("Error clearing collector: %v", err)
	}
	if count != 1 {
		t.Errorf("Expected 1 log to be cleared, got %d", count)
	}
}

func TestMultipleFunctionsWithCollector(t *testing.T) {
	t.Parallel()
	ctx := context.Background()
	collector, err := b.NewCollector("test-multiple-functions-collector")
	if err != nil {
		t.Fatalf("Error creating collector: %v", err)
	}

	// Call Foo
	result1, err := b.Foo(ctx, 123, b.WithCollector(collector))
	if err != nil {
		t.Fatalf("Error in first Foo call: %v", err)
	}
	if result1.BamlTypeName() == "" {
		t.Errorf("Expected valid result from first Foo call")
	}

	// Call Bar
	result2, err := b.Bar(ctx, 456, b.WithCollector(collector))
	if err != nil {
		t.Fatalf("Error in Bar call: %v", err)
	}
	if result2.BamlTypeName() == "" {
		t.Errorf("Expected valid result from Bar call")
	}

	// Call Foo again
	result3, err := b.Foo(ctx, 789, b.WithCollector(collector))
	if err != nil {
		t.Fatalf("Error in second Foo call: %v", err)
	}
	if result3.BamlTypeName() == "" {
		t.Errorf("Expected valid result from second Foo call")
	}

	// Test that we have multiple logs
	logs, err := collector.Logs()
	if err != nil {
		t.Fatalf("Error getting logs: %v", err)
	}
	if len(logs) != 3 {
		t.Errorf("Expected 3 logs, got %d", len(logs))
	}

	// Verify function names in logs
	expectedFunctions := []string{"Foo", "Bar", "Foo"}
	for i, log := range logs {
		functionName, err := log.FunctionName()
		if err != nil {
			t.Errorf("Error getting function name for log %d: %v", i, err)
			continue
		}
		if functionName != expectedFunctions[i] {
			t.Errorf("Expected function name %s for log %d, got %s", expectedFunctions[i], i, functionName)
		}
	}

	// Test comprehensive collector API with multiple calls
	testCollectorAPI(t, collector, "Multiple")
}

func TestCollectorClear(t *testing.T) {
	t.Parallel()
	ctx := context.Background()
	collector, err := b.NewCollector("test-clear-collector")
	if err != nil {
		t.Fatalf("Error creating collector: %v", err)
	}

	// Make some calls
	_, err = b.Foo(ctx, 111, b.WithCollector(collector))
	if err != nil {
		t.Fatalf("Error in Foo call: %v", err)
	}

	_, err = b.Bar(ctx, 222, b.WithCollector(collector))
	if err != nil {
		t.Fatalf("Error in Bar call: %v", err)
	}

	// Verify we have logs
	logs, err := collector.Logs()
	if err != nil {
		t.Fatalf("Error getting logs: %v", err)
	}
	if len(logs) == 0 {
		t.Errorf("Expected logs before clear, got none")
	}

	// Clear the collector
	count, err := collector.Clear()
	if err != nil {
		t.Errorf("Error clearing collector: %v", err)
	}
	if count != 2 {
		t.Errorf("Expected 2 logs to be cleared, got %d", count)
	}

	t.Log("Collector cleared successfully")
}

func TestCollectorWithNamedCollector(t *testing.T) {
	t.Parallel()
	ctx := context.Background()
	collectorName := "comprehensive-test-collector"
	collector, err := b.NewCollector(collectorName)
	if err != nil {
		t.Fatalf("Error creating collector: %v", err)
	}

	// Test collector name
	name, err := collector.Name()
	if err != nil {
		t.Fatalf("Error getting collector name: %v", err)
	}
	if name != collectorName {
		t.Errorf("Expected collector name %s, got %s", collectorName, name)
	}

	result, err := b.Foo(ctx, 777, b.WithCollector(collector))
	if err != nil {
		t.Fatalf("Error in Foo: %v", err)
	}

	if result.BamlTypeName() == "" {
		t.Errorf("Expected valid result from Foo")
	}

	testCollectorAPI(t, collector, "Foo")
}

// testCollectorAPI is a comprehensive helper function that tests all collector APIs
func testCollectorAPI(t *testing.T, collector baml.Collector, expectedFunction string) {
	t.Helper()

	// Test collector name
	name, err := collector.Name()
	if err != nil {
		t.Errorf("Error getting collector name: %v", err)
	} else {
		t.Logf("Collector name: %s", name)
	}

	// Test usage collection
	usage, err := collector.Usage()
	if err != nil {
		t.Fatalf("Error getting usage: %v", err)
	}

	inputTokens, err := usage.InputTokens()
	if err != nil {
		t.Fatalf("Error getting input tokens: %v", err)
	}
	if inputTokens <= 0 {
		t.Errorf("Expected positive input tokens, got %d", inputTokens)
	} else {
		t.Logf("Input tokens: %d", inputTokens)
	}

	outputTokens, err := usage.OutputTokens()
	if err != nil {
		t.Fatalf("Error getting output tokens: %v", err)
	}
	if outputTokens <= 0 {
		t.Errorf("Expected positive output tokens, got %d", outputTokens)
	} else {
		t.Logf("Output tokens: %d", outputTokens)
	}

	// Test logs collection
	logs, err := collector.Logs()
	if err != nil {
		t.Fatalf("Error getting logs: %v", err)
	}
	if len(logs) == 0 {
		t.Errorf("Expected at least one log entry")
		return
	}
	t.Logf("Found %d log entries", len(logs))

	// Test last log
	lastLog, err := collector.Last()
	if err != nil {
		t.Fatalf("Error getting last log: %v", err)
	}
	if lastLog == nil {
		t.Errorf("Expected last log to be non-nil")
		return
	}

	// Test function log details
	testFunctionLogAPI(t, lastLog, expectedFunction)

	// Test all logs
	for i, log := range logs {
		t.Logf("Testing log entry %d", i)
		testFunctionLogAPI(t, log, "")
	}
}

// testFunctionLogAPI tests all FunctionLog APIs
func testFunctionLogAPI(t *testing.T, log baml.FunctionLog, expectedFunction string) {
	t.Helper()

	// Test log ID
	id, err := log.Id()
	if err != nil {
		t.Errorf("Error getting log ID: %v", err)
	} else {
		t.Logf("Log ID: %s", id)
	}

	// Test function name
	functionName, err := log.FunctionName()
	if err != nil {
		t.Errorf("Error getting function name: %v", err)
	} else {
		t.Logf("Function name: %s", functionName)
		if expectedFunction != "" && !strings.Contains(expectedFunction, functionName) && expectedFunction != "Multiple" {
			t.Errorf("Expected function name to contain %s, got %s", expectedFunction, functionName)
		}
	}

	// Test log type
	logType, err := log.LogType()
	if err != nil {
		t.Errorf("Error getting log type: %v", err)
	} else {
		t.Logf("Log type: %s", logType)
	}

	// Test timing
	timing, err := log.Timing()
	if err != nil {
		t.Errorf("Error getting timing: %v", err)
	} else if timing != nil {
		startTime, err := timing.StartTimeUTCMs()
		if err != nil {
			t.Errorf("Error getting start time: %v", err)
		} else {
			t.Logf("Start time (UTC ms): %d", startTime)
			// Validate start time is reasonable (within last hour)
			now := time.Now().UnixMilli()
			if startTime > now || startTime < now-3600000 {
				t.Errorf("Start time seems unreasonable: %d (now: %d)", startTime, now)
			}
		}

		duration, err := timing.DurationMs()
		if err != nil {
			t.Errorf("Error getting duration: %v", err)
		} else if duration == nil {
			t.Errorf("Duration is nil")
		} else {
			t.Logf("Duration (ms): %d", duration)
			// Validate duration is reasonable (less than 30 seconds)
			if *duration < 0 || *duration > 30000 {
				t.Errorf("Duration seems unreasonable: %d ms", *duration)
			}
		}
	}

	// Test usage from log
	logUsage, err := log.Usage()
	if err != nil {
		t.Errorf("Error getting log usage: %v", err)
	} else if logUsage != nil {
		inputTokens, err := logUsage.InputTokens()
		if err != nil {
			t.Errorf("Error getting log input tokens: %v", err)
		} else {
			t.Logf("Log input tokens: %d", inputTokens)
		}

		outputTokens, err := logUsage.OutputTokens()
		if err != nil {
			t.Errorf("Error getting log output tokens: %v", err)
		} else {
			t.Logf("Log output tokens: %d", outputTokens)
		}
	}

	// Test raw LLM response
	rawResponse, err := log.RawLLMResponse()
	if err != nil {
		t.Errorf("Error getting raw LLM response: %v", err)
	} else if rawResponse != "" {
		t.Logf("Raw LLM response length: %d characters", len(rawResponse))
		// Basic validation that it's JSON-like
		if !strings.Contains(rawResponse, "{") && !strings.Contains(rawResponse, "[") {
			t.Errorf("Raw LLM response doesn't look like JSON: %s", rawResponse[:min(100, len(rawResponse))])
		}
	}

	// // Test metadata
	// metadata, err := log.Metadata()
	// if err != nil {
	// 	t.Errorf("Error getting metadata: %v", err)
	// } else if metadata != nil {
	// 	t.Logf("Metadata keys: %v", getMapKeys(metadata))
	// }

	// Test calls count and calls
	calls, err := log.Calls()
	if err != nil {
		t.Errorf("Error getting calls: %v", err)
	} else {
		// Test each call
		for i, call := range calls {
			testLLMCallAPI(t, call, i)
		}

		// Test selected call
		selectedCall, err := log.SelectedCall()
		if err != nil {
			t.Errorf("Error getting selected call: %v", err)
		} else if selectedCall != nil {
			t.Logf("Found selected call")
			testLLMCallAPI(t, selectedCall, -1) // -1 indicates this is the selected call
		} else {
			t.Log("No selected call found")
		}
	}
}

// testLLMCallAPI tests all LLMCall APIs
func testLLMCallAPI(t *testing.T, call baml.LLMCall, index int) {
	t.Helper()

	prefix := fmt.Sprintf("Call %d", index)
	if index == -1 {
		prefix = "Selected call"
	}

	// Test client name
	clientName, err := call.ClientName()
	if err != nil {
		t.Errorf("%s: Error getting client name: %v", prefix, err)
	} else {
		t.Logf("%s: Client name: %s", prefix, clientName)
	}

	// Test provider
	provider, err := call.Provider()
	if err != nil {
		t.Errorf("%s: Error getting provider: %v", prefix, err)
	} else {
		t.Logf("%s: Provider: %s", prefix, provider)
	}

	// Test selected status
	selected, err := call.Selected()
	if err != nil {
		t.Errorf("%s: Error getting selected status: %v", prefix, err)
	} else {
		t.Logf("%s: Selected: %v", prefix, selected)
		if index == -1 && !selected {
			t.Errorf("Selected call should have selected=true")
		}
	}

	// Test timing
	timing, err := call.Timing()
	if err != nil {
		t.Errorf("%s: Error getting timing: %v", prefix, err)
	} else if timing != nil {
		startTime, err := timing.StartTimeUTCMs()
		if err != nil {
			t.Errorf("%s: Error getting start time: %v", prefix, err)
		} else {
			t.Logf("%s: Start time: %d", prefix, startTime)
		}

		duration, err := timing.DurationMs()
		if err != nil {
			t.Errorf("%s: Error getting duration: %v", prefix, err)
		} else {
			t.Logf("%s: Duration: %d ms", prefix, duration)
		}
	}

	// Test usage
	usage, err := call.Usage()
	if err != nil {
		t.Errorf("%s: Error getting usage: %v", prefix, err)
	} else if usage != nil {
		inputTokens, err := usage.InputTokens()
		if err != nil {
			t.Errorf("%s: Error getting input tokens: %v", prefix, err)
		} else {
			t.Logf("%s: Input tokens: %d", prefix, inputTokens)
		}

		outputTokens, err := usage.OutputTokens()
		if err != nil {
			t.Errorf("%s: Error getting output tokens: %v", prefix, err)
		} else {
			t.Logf("%s: Output tokens: %d", prefix, outputTokens)
		}
	}

	requestId, err := call.RequestId()
	if err != nil {
		t.Errorf("%s: Error getting request id: %v", prefix, err)
	}

	// Test HTTP request
	httpRequest, err := call.HttpRequest()
	if err != nil {
		t.Errorf("%s: Error getting HTTP request: %v", prefix, err)
	} else if httpRequest != nil {
		testHTTPRequestAPI(t, httpRequest, requestId, prefix)
	}

	// Test HTTP response
	httpResponse, err := call.HttpResponse()
	if err != nil {
		t.Errorf("%s: Error getting HTTP response: %v", prefix, err)
	} else if httpResponse != nil {
		testHTTPResponseAPI(t, httpResponse, requestId, prefix)
	}

	// Test SSE responses (only for streaming calls)
	if sseResponses, ok := call.(baml.LLMStreamCall); ok {
		sseResponses, err := sseResponses.SSEChunks()
		if err != nil {
			t.Errorf("%s: Error getting SSE responses: %v", prefix, err)
		} else if sseResponses != nil {
			t.Logf("%s: Found %d SSE responses", prefix, len(sseResponses))
		}
		for _, sse := range sseResponses {
			testSSEResponseAPI(t, sse, prefix)
		}
	}
}

// testHTTPRequestAPI tests all HTTPRequest APIs
func testHTTPRequestAPI(t *testing.T, req baml.HTTPRequest, requestId string, prefix string) {
	t.Helper()

	// Test request ID
	id, err := req.RequestId()
	if err != nil {
		t.Errorf("%s: Error getting request ID: %v", prefix, err)
	} else {
		if id != requestId {
			t.Errorf("%s: Request ID mismatch: %s != %s", prefix, id, requestId)
		}
		t.Logf("%s: Request ID: %s", prefix, id)
	}

	// Test URL
	url, err := req.Url()
	if err != nil {
		t.Errorf("%s: Error getting URL: %v", prefix, err)
	} else {
		t.Logf("%s: URL: %s", prefix, url)
		if !strings.HasPrefix(url, "http") {
			t.Errorf("%s: URL should start with http, got: %s", prefix, url)
		}
	}

	// Test method
	method, err := req.Method()
	if err != nil {
		t.Errorf("%s: Error getting method: %v", prefix, err)
	} else {
		t.Logf("%s: Method: %s", prefix, method)
		if method != "POST" && method != "GET" {
			t.Errorf("%s: Unexpected HTTP method: %s", prefix, method)
		}
	}

	// Test headers
	headers, err := req.Headers()
	if err != nil {
		t.Errorf("%s: Error getting headers: %v", prefix, err)
	} else if headers != nil {
		t.Logf("%s: Headers count: %d", prefix, len(headers))
		// Look for common headers
		if contentType, ok := headers["content-type"]; ok {
			t.Logf("%s: Content-Type: %v", prefix, contentType)
		}
		if authorization, ok := headers["authorization"]; ok {
			t.Logf("%s: Has Authorization header", prefix)
			// Don't log the actual value for security
			_ = authorization
		}
	}

	// Test body
	body, err := req.Body()
	if err != nil {
		t.Errorf("%s: Error getting request body: %v", prefix, err)
	} else if body != nil {
		testHTTPBodyAPI(t, body, prefix+" request")
	}
}

// testHTTPResponseAPI tests all HTTPResponse APIs
func testHTTPResponseAPI(t *testing.T, resp baml.HTTPResponse, requestId string, prefix string) {
	t.Helper()

	id, err := resp.RequestId()
	if err != nil {
		t.Errorf("%s: Error getting request id: %v", prefix, err)
	} else {
		if id != requestId {
			t.Errorf("%s: Request ID mismatch: %s != %s", prefix, id, requestId)
		}
		t.Logf("%s: Request ID: %s", prefix, id)
	}

	// Test status
	status, err := resp.Status()
	if err != nil {
		t.Errorf("%s: Error getting status: %v", prefix, err)
	} else {
		t.Logf("%s: Status: %d", prefix, status)
		if status < 200 || status >= 300 {
			t.Errorf("%s: Unexpected HTTP status: %d", prefix, status)
		}
	}

	// Test headers
	headers, err := resp.Headers()
	if err != nil {
		t.Errorf("%s: Error getting response headers: %v", prefix, err)
	} else if headers != nil {
		t.Logf("%s: Response headers count: %d", prefix, len(headers))
		// Look for common response headers
		if contentType, ok := headers["content-type"]; ok {
			t.Logf("%s: Response Content-Type: %v", prefix, contentType)
		}
	}

	// Test body
	body, err := resp.Body()
	if err != nil {
		t.Errorf("%s: Error getting response body: %v", prefix, err)
	} else if body != nil {
		testHTTPBodyAPI(t, body, prefix+" response")
	}
}

// testHTTPBodyAPI tests all HTTPBody APIs
func testHTTPBodyAPI(t *testing.T, body baml.HTTPBody, prefix string) {
	t.Helper()

	// Test raw bytes
	raw, err := body.Text()
	if err != nil {
		t.Errorf("%s: Error getting raw body: %v", prefix, err)
	} else {
		t.Logf("%s: Body size: %d bytes", prefix, len(raw))
	}

	// Test text
	text, err := body.Text()
	if err != nil {
		t.Errorf("%s: Error getting body text: %v", prefix, err)
	} else {
		t.Logf("%s: Body text length: %d characters", prefix, len(text))
		if len(text) > 0 && len(text) < 1000 {
			t.Logf("%s: Body text preview: %s", prefix, text[:min(200, len(text))])
		}
	}

	// Test JSON
	jsonData, err := body.JSON()
	if err != nil {
		t.Logf("%s: Body is not valid JSON: %v", prefix, err)
	} else if jsonData != nil {
		t.Logf("%s: Body contains valid JSON data", prefix)
	}
}

// testSSEResponseAPI tests all SSEResponse APIs
func testSSEResponseAPI(t *testing.T, sse baml.SSEResponse, prefix string) {
	t.Helper()

	// Test text
	text, err := sse.Text()
	if err != nil {
		t.Errorf("%s: Error getting SSE text: %v", prefix, err)
	} else {
		t.Logf("%s: Text length: %d characters", prefix, len(text))
		if len(text) > 0 && len(text) < 500 {
			t.Logf("%s: Text content: %s", prefix, text[:min(100, len(text))])
		}
	}

	// Test JSON
	jsonData, err := sse.JSON()
	if err != nil {
		t.Logf("%s: SSE text is not valid JSON: %v", prefix, err)
	} else if jsonData != nil {
		t.Logf("%s: SSE contains: %v", prefix, jsonData)
	} else {
		t.Logf("%s: SSE JSON data is null", prefix)
	}
}

// Helper functions
func getMapKeys(m map[string]interface{}) []string {
	keys := make([]string, 0, len(m))
	for k := range m {
		keys = append(keys, k)
	}
	return keys
}

func min(a, b int) int {
	if a < b {
		return a
	}
	return b
}

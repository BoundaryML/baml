package baml

import (
	"testing"
)

func TestCollectorAPI(t *testing.T) {
	// Test creating a collector
	collector, err := NewCollector("test-collector")
	if err != nil {
		t.Fatalf("Failed to create collector: %v", err)
	}

	// Test getting the name
	name, err := collector.Name()
	if err != nil {
		t.Errorf("Failed to get collector name: %v", err)
	}
	t.Logf("Collector name: %s", name)

	// Test getting usage (should be empty initially)
	usage, err := collector.Usage()
	if err != nil {
		t.Errorf("Failed to get usage: %v", err)
	}

	inputTokens, err := usage.InputTokens()
	if err != nil {
		t.Errorf("Failed to get input tokens: %v", err)
	}
	t.Logf("Input tokens: %d", inputTokens)

	outputTokens, err := usage.OutputTokens()
	if err != nil {
		t.Errorf("Failed to get output tokens: %v", err)
	}
	t.Logf("Output tokens: %d", outputTokens)

	// Test getting logs (should be empty initially)
	logs, err := collector.Logs()
	if err != nil {
		t.Errorf("Failed to get logs: %v", err)
	}
	t.Logf("Number of logs: %d", len(logs))

	// Test getting last log (should be nil initially)
	lastLog, err := collector.Last()
	if err != nil {
		t.Errorf("Failed to get last log: %v", err)
	}
	if lastLog != nil {
		t.Errorf("Expected no last log, got: %v", lastLog)
	}

	t.Log("Collector API test completed successfully")
}

func TestCollectorNoName(t *testing.T) {
	// Test creating a collector without a name
	collector, err := NewCollector("")
	if err != nil {
		t.Errorf("Failed to create collector: %v", err)
	}

	// Test getting the name
	name, err := collector.Name()
	if err != nil {
		t.Errorf("Failed to get collector name: %v", err)
	}
	t.Logf("Collector name (no name): %s", name)

	t.Log("Collector no-name test completed successfully")
}

func TestCollectorCallsAPI(t *testing.T) {
	// Test creating a collector
	collector, err := NewCollector("calls-test-collector")
	if err != nil {
		t.Errorf("Failed to create collector: %v", err)
		return
	}

	// Test getting logs (should be empty initially)
	logs, err := collector.Logs()
	if err != nil {
		t.Errorf("Failed to get logs: %v", err)
	}

	// If we have logs, test the calls API
	for _, log := range logs {
		calls, err := log.Calls()
		if err != nil {
			t.Errorf("Failed to get calls: %v", err)
			continue
		}

		t.Logf("Found %d calls in log %s", len(calls), "test")

		// Test each call's properties
		for i, call := range calls {
			clientName, err := call.ClientName()
			if err != nil {
				t.Errorf("Failed to get client name for call %d: %v", i, err)
				continue
			}

			provider, err := call.Provider()
			if err != nil {
				t.Errorf("Failed to get provider for call %d: %v", i, err)
				continue
			}

			selected, err := call.Selected()
			if err != nil {
				t.Errorf("Failed to get selected for call %d: %v", i, err)
				continue
			}

			t.Logf("Call %d: client=%s, provider=%s, selected=%v", i, clientName, provider, selected)
		}

		// Test SelectedCall method
		selectedCall, err := log.SelectedCall()
		if err != nil {
			t.Errorf("Failed to get selected call: %v", err)
		} else if selectedCall != nil {
			clientName, _ := selectedCall.ClientName()
			t.Logf("Selected call: client=%s", clientName)
		} else {
			t.Log("No selected call found")
		}
	}

	t.Log("Collector calls API test completed successfully")
}

func TestCollectorClearAPI(t *testing.T) {
	// Test creating a collector
	collector, err := NewCollector("clear-test-collector")
	if err != nil {
		t.Errorf("Failed to create collector: %v", err)
		return
	}

	// Test initial state
	logs, err := collector.Logs()
	if err != nil {
		t.Errorf("Failed to get logs: %v", err)
	}
	t.Logf("Initial logs count: %d", len(logs))

	// Test Clear method
	count, err := collector.Clear()
	if err != nil {
		t.Errorf("Failed to clear collector: %v", err)
	}
	t.Logf("Cleared %d logs", count)

	// Test state after clear (should still be empty in this case)
	logsAfterClear, err := collector.Logs()
	if err != nil {
		t.Errorf("Failed to get logs after clear: %v", err)
	}
	t.Logf("Logs count after clear: %d", len(logsAfterClear))

	t.Log("Collector clear API test completed successfully")
}

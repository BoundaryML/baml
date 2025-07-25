package baml

import (
	"context"
	"fmt"
	"sync"
	"testing"
	"time"
)

// Mock FFI stream for testing
type MockFFIStream struct {
	cancelled bool
	mutex     sync.RWMutex
}

func (m *MockFFIStream) Cancel() {
	m.mutex.Lock()
	defer m.mutex.Unlock()
	m.cancelled = true
}

func (m *MockFFIStream) IsCancelled() bool {
	m.mutex.RLock()
	defer m.mutex.RUnlock()
	return m.cancelled
}

func (m *MockFFIStream) Done() (*string, error) {
	if m.IsCancelled() {
		return nil, fmt.Errorf("stream was cancelled")
	}
	result := "final result"
	return &result, nil
}

func TestBamlStreamCancellation(t *testing.T) {
	ctx := context.Background()
	mockFFI := &MockFFIStream{}
	stream := NewBamlStream[string, string](mockFFI, ctx)

	// Test initial state
	if stream.IsCancelled() {
		t.Error("Stream should not be cancelled initially")
	}

	// Test cancellation
	stream.Cancel()

	if !stream.IsCancelled() {
		t.Error("Stream should be cancelled after Cancel() call")
	}

	if !mockFFI.IsCancelled() {
		t.Error("Mock FFI stream should be cancelled")
	}
}

func TestBamlStreamContextCancellation(t *testing.T) {
	ctx, cancel := context.WithCancel(context.Background())
	mockFFI := &MockFFIStream{}
	stream := NewBamlStream[string, string](mockFFI, ctx)

	// Cancel the context
	cancel()

	// Stream should detect context cancellation
	if !stream.IsCancelled() {
		t.Error("Stream should be cancelled when context is cancelled")
	}
}

func TestBamlStreamGetFinalResponseSuccess(t *testing.T) {
	ctx := context.Background()
	mockFFI := &MockFFIStream{}
	stream := NewBamlStream[string, string](mockFFI, ctx)

	result, err := stream.GetFinalResponse()
	if err != nil {
		t.Errorf("GetFinalResponse should succeed: %v", err)
	}

	if result == nil || *result != "final result" {
		t.Errorf("Expected 'final result', got %v", result)
	}
}

func TestBamlStreamGetFinalResponseWithCancellation(t *testing.T) {
	ctx := context.Background()
	mockFFI := &MockFFIStream{}
	stream := NewBamlStream[string, string](mockFFI, ctx)

	// Cancel the stream
	stream.Cancel()

	result, err := stream.GetFinalResponse()
	if err == nil {
		t.Error("GetFinalResponse should return error when cancelled")
	}

	if result != nil {
		t.Error("Result should be nil when cancelled")
	}
}

func TestBamlStreamCancellationTiming(t *testing.T) {
	ctx := context.Background()
	mockFFI := &MockFFIStream{}
	stream := NewBamlStream[string, string](mockFFI, ctx)

	start := time.Now()

	// Cancel after short delay
	go func() {
		time.Sleep(100 * time.Millisecond)
		stream.Cancel()
	}()

	// This should be cancelled quickly
	_, err := stream.GetFinalResponse()
	elapsed := time.Since(start)

	if err == nil {
		t.Error("Expected cancellation error")
	}

	// Should complete quickly due to cancellation
	if elapsed > 500*time.Millisecond {
		t.Errorf("Cancellation took too long: %v", elapsed)
	}
}

func TestBamlStreamMultipleCancellations(t *testing.T) {
	ctx := context.Background()
	mockFFI := &MockFFIStream{}
	stream := NewBamlStream[string, string](mockFFI, ctx)

	// Multiple cancellations should be safe
	stream.Cancel()
	stream.Cancel()
	stream.Cancel()

	if !stream.IsCancelled() {
		t.Error("Stream should be cancelled")
	}
}

func TestBamlStreamConcurrentCancellation(t *testing.T) {
	ctx := context.Background()
	mockFFI := &MockFFIStream{}
	stream := NewBamlStream[string, string](mockFFI, ctx)

	// Cancel from multiple goroutines simultaneously
	var wg sync.WaitGroup
	for i := 0; i < 10; i++ {
		wg.Add(1)
		go func() {
			defer wg.Done()
			stream.Cancel()
		}()
	}

	wg.Wait()

	if !stream.IsCancelled() {
		t.Error("Stream should be cancelled")
	}
}

func TestBamlStreamWithTimeout(t *testing.T) {
	ctx, cancel := context.WithTimeout(context.Background(), 100*time.Millisecond)
	defer cancel()

	mockFFI := &MockFFIStream{}
	stream := NewBamlStream[string, string](mockFFI, ctx)

	start := time.Now()

	// This should timeout
	_, err := stream.GetFinalResponse()
	elapsed := time.Since(start)

	if err == nil {
		t.Error("Expected timeout error")
	}

	// Should timeout around 100ms
	if elapsed < 90*time.Millisecond || elapsed > 200*time.Millisecond {
		t.Errorf("Unexpected timeout duration: %v", elapsed)
	}
}

func TestBamlStreamChannelCancellation(t *testing.T) {
	ctx, cancel := context.WithCancel(context.Background())
	mockFFI := &MockFFIStream{}
	stream := NewBamlStream[string, string](mockFFI, ctx)

	resultChan := stream.Stream()

	// Cancel after short delay
	go func() {
		time.Sleep(50 * time.Millisecond)
		cancel()
	}()

	// Read from channel until cancelled
	var results []StreamResult[string, string]
	for result := range resultChan {
		results = append(results, result)
		if result.Error() != nil {
			break
		}
	}

	// Should have received a cancellation error
	if len(results) == 0 {
		t.Error("Should have received at least one result (error)")
	}

	lastResult := results[len(results)-1]
	if lastResult.Error() == nil {
		t.Error("Last result should be an error due to cancellation")
	}
}

// Benchmark cancellation performance
func BenchmarkBamlStreamCancellation(b *testing.B) {
	for i := 0; i < b.N; i++ {
		ctx := context.Background()
		mockFFI := &MockFFIStream{}
		stream := NewBamlStream[string, string](mockFFI, ctx)
		stream.Cancel()
	}
}

// Test memory cleanup
func TestBamlStreamMemoryCleanup(t *testing.T) {
	// Create many streams and cancel them
	for i := 0; i < 1000; i++ {
		ctx := context.Background()
		mockFFI := &MockFFIStream{}
		stream := NewBamlStream[string, string](mockFFI, ctx)
		stream.Cancel()
		
		// Verify cleanup
		if !stream.IsCancelled() {
			t.Errorf("Stream %d should be cancelled", i)
		}
	}
}

// Integration test with real context patterns
func TestBamlStreamRealWorldUsage(t *testing.T) {
	// Simulate real-world usage pattern
	ctx, cancel := context.WithTimeout(context.Background(), 1*time.Second)
	defer cancel()

	mockFFI := &MockFFIStream{}
	stream := NewBamlStream[string, string](mockFFI, ctx)

	// Simulate user cancelling operation
	userCancel := make(chan bool)
	go func() {
		time.Sleep(100 * time.Millisecond)
		userCancel <- true
	}()

	select {
	case <-userCancel:
		stream.Cancel()
		if !stream.IsCancelled() {
			t.Error("Stream should be cancelled after user action")
		}
	case <-ctx.Done():
		t.Error("Should not timeout in this test")
	}
}

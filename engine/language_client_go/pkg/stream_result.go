package baml

import (
	"context"
	"fmt"
	"sync"
)

type StreamResult[Partial any, Final any] struct {
	partial *Partial
	final   *Final
	error   error
}

func (result *StreamResult[Partial, Final]) Partial() Partial {
	return *result.partial
}

func (result *StreamResult[Partial, Final]) Final() Final {
	return *result.final
}

func (result *StreamResult[Partial, Final]) IsFinal() bool {
	return result.final != nil
}

func (result *StreamResult[Partial, Final]) IsPartial() bool {
	return result.partial != nil
}

func (result *StreamResult[Partial, Final]) Error() error {
	return result.error
}

// BamlStream provides streaming functionality with cancellation support
type BamlStream[Partial any, Final any] struct {
	ffiStream    interface{} // The underlying FFI stream
	ctx          context.Context
	cancel       context.CancelFunc
	finalResult  *Final
	finalError   error
	mutex        sync.RWMutex
	cancelled    bool
}

// NewBamlStream creates a new BAML stream with cancellation support
func NewBamlStream[Partial any, Final any](ffiStream interface{}, ctx context.Context) *BamlStream[Partial, Final] {
	streamCtx, cancel := context.WithCancel(ctx)
	return &BamlStream[Partial, Final]{
		ffiStream: ffiStream,
		ctx:       streamCtx,
		cancel:    cancel,
	}
}

// Cancel cancels the stream processing.
// This will:
// 1. Cancel the Rust-level stream
// 2. Cancel ongoing HTTP requests to LLM providers
// 3. Stop consuming network bandwidth and API quota
// 4. Clean up resources
func (s *BamlStream[Partial, Final]) Cancel() {
	s.mutex.Lock()
	defer s.mutex.Unlock()
	
	if !s.cancelled {
		s.cancelled = true
		s.cancel()
		
		// Call the FFI stream's cancel method if it exists
		if cancellable, ok := s.ffiStream.(interface{ Cancel() }); ok {
			cancellable.Cancel()
		}
	}
}

// IsCancelled returns true if the stream has been cancelled
func (s *BamlStream[Partial, Final]) IsCancelled() bool {
	s.mutex.RLock()
	defer s.mutex.RUnlock()
	return s.cancelled || s.ctx.Err() != nil
}

// Context returns the stream's context for cancellation checking
func (s *BamlStream[Partial, Final]) Context() context.Context {
	return s.ctx
}

// GetFinalResponse waits for the stream to complete and returns the final result.
// If the stream is cancelled, this will return a cancellation error.
func (s *BamlStream[Partial, Final]) GetFinalResponse() (*Final, error) {
	s.mutex.RLock()
	if s.finalResult != nil {
		defer s.mutex.RUnlock()
		return s.finalResult, s.finalError
	}
	s.mutex.RUnlock()
	
	// Check for cancellation
	select {
	case <-s.ctx.Done():
		return nil, s.ctx.Err()
	default:
	}
	
	// This would call the FFI stream's done method
	// Implementation depends on the specific FFI interface
	if doneMethod, ok := s.ffiStream.(interface{ Done() (*Final, error) }); ok {
		result, err := doneMethod.Done()
		
		s.mutex.Lock()
		s.finalResult = result
		s.finalError = err
		s.mutex.Unlock()
		
		return result, err
	}
	
	return nil, ErrStreamNotSupported
}

// Stream returns a channel that yields partial results as they become available
func (s *BamlStream[Partial, Final]) Stream() <-chan StreamResult[Partial, Final] {
	resultChan := make(chan StreamResult[Partial, Final])
	
	go func() {
		defer close(resultChan)
		
		// This would set up the event handler and stream processing
		// Implementation depends on the specific FFI interface
		
		for {
			select {
			case <-s.ctx.Done():
				// Stream was cancelled
				resultChan <- StreamResult[Partial, Final]{
					error: s.ctx.Err(),
				}
				return
			default:
				// Check if we have new partial results
				// This would come from the FFI stream's event handler
				// For now, we'll just wait for the final result
				
				final, err := s.GetFinalResponse()
				if err != nil {
					resultChan <- StreamResult[Partial, Final]{
						error: err,
					}
				} else {
					resultChan <- StreamResult[Partial, Final]{
						final: final,
					}
				}
				return
			}
		}
	}()
	
	return resultChan
}

var ErrStreamNotSupported = fmt.Errorf("stream operation not supported by FFI interface")

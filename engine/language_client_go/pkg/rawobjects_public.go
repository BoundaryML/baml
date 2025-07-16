package baml

import "github.com/boundaryml/baml/engine/language_client_go/baml_go/raw_objects"

type Collector interface {
	raw_objects.RawPointer
	// Usage gathers the Usage object but keeps the underlying C collector alive
	// until the Usage is done.
	Usage() (Usage, error)
	// Name returns the collector name
	Name() (string, error)
	// Logs returns all function logs
	Logs() ([]FunctionLog, error)
	// Last returns the most recent function log
	Last() (FunctionLog, error)
	// ID looks up a function log by ID
	Id(functionId string) (FunctionLog, error)
	// Clear removes all logs and frees memory
	Clear() (int64, error)
}

type Usage interface {
	raw_objects.RawPointer
	// InputTokens returns the number of input tokens
	InputTokens() (int64, error)
	// OutputTokens returns the number of output tokens
	OutputTokens() (int64, error)
}

type FunctionLog interface {
	raw_objects.RawPointer
	// ID returns the function log ID
	Id() (string, error)
	// FunctionName returns the function name
	FunctionName() (string, error)
	// LogType returns the log type
	LogType() (string, error)
	// Timing returns the timing information
	Timing() (Timing, error)
	// Usage returns the usage information
	Usage() (Usage, error)
	// RawLLMResponse returns the raw LLM response
	RawLLMResponse() (string, error)
	// Calls returns all LLM calls made during this function execution (can be LLMCall or LLMStreamCall)
	Calls() ([]LLMCall, error)
	// SelectedCall returns the call that was selected for parsing (can be LLMCall or LLMStreamCall)
	SelectedCall() (LLMCall, error)
	// Tags returns any user-provided metadata
	Tags() (map[string]any, error)
}

type Timing interface {
	raw_objects.RawPointer
	// StartTimeUTCMs returns the start time in milliseconds since epoch
	StartTimeUTCMs() (int64, error)
	// DurationMs returns the duration in milliseconds (nullable)
	DurationMs() (*int64, error)
}

type StreamTiming interface {
	Timing
}

type LLMCall interface {
	raw_objects.RawPointer
	// ID returns the request ID: Not the same as the function log ID
	RequestId() (string, error)
	// ClientName returns the name of the client used
	ClientName() (string, error)
	// Provider returns the provider of the client
	Provider() (string, error)
	// HttpRequest returns the raw HTTP request
	HttpRequest() (HTTPRequest, error)
	// HttpResponse returns the raw HTTP response (nullable for streaming)
	HttpResponse() (HTTPResponse, error)
	// Usage returns the usage information (nullable)
	Usage() (Usage, error)
	// Selected returns whether this call was selected for parsing
	Selected() (bool, error)
	// Timing returns the timing information
	Timing() (Timing, error)
}

type LLMStreamCall interface {
	LLMCall
	// SSEChunks returns the SSE chunks of the response
	SSEChunks() ([]SSEResponse, error)
}

type HTTPRequest interface {
	raw_objects.RawPointer
	// ID returns the request ID
	RequestId() (string, error)
	// URL returns the request URL
	Url() (string, error)
	// Method returns the HTTP method
	Method() (string, error)
	// Headers returns the request headers
	Headers() (map[string]string, error)
	// Body returns the request body
	Body() (HTTPBody, error)
}

type HTTPResponse interface {
	raw_objects.RawPointer
	// ID returns the request ID
	RequestId() (string, error)
	// Status returns the HTTP status code
	Status() (int64, error)
	// Headers returns the response headers
	Headers() (map[string]string, error)
	// Body returns the response body
	Body() (HTTPBody, error)
}

type HTTPBody interface {
	raw_objects.RawPointer
	// Text returns the body as a string
	Text() (string, error)
	// JSON returns the body as a JSON object
	JSON() (any, error)
}

type SSEResponse interface {
	raw_objects.RawPointer
	// Text returns the body as a string
	Text() (string, error)
	// JSON returns the body as a JSON object (nullable)
	JSON() (any, error)
}

package baml

import (
	"encoding/json"
	"fmt"
	"runtime"
	"sync/atomic"
	"unsafe"

	"github.com/boundaryml/baml/engine/language_client_go/baml_go"
)

/*
#include <stdlib.h>
#include <string.h>
*/
import "C"

// ---- public API ------------------------------------------------------------
const collectorType = "collector"
const usageType = "usage"
const functionLogType = "function_log"
const timingType = "timing"
const llmCallType = "llm_call"
const httpRequestType = "http_request"
const httpResponseType = "http_response"
const httpBodyType = "http_body"
const sseResponseType = "sse_response"

type Collector interface {
	// Usage gathers the Usage object but keeps the underlying C collector alive
	// until the Usage is done.
	Usage() (*Usage, error)
	// Name returns the collector name
	Name() (string, error)
	// Logs returns all function logs
	Logs() ([]*FunctionLog, error)
	// Last returns the most recent function log
	Last() (*FunctionLog, error)
	// ID looks up a function log by ID
	ID(functionID string) (*FunctionLog, error)
	// Clear removes all logs and frees memory
	Clear() error
	id() int64
}

func NewCollector(name string) Collector {
	var encodedArgs []byte
	if name != "" {
		args := BamlFunctionArguments{
			Kwargs: map[string]any{"name": name},
		}
		encoded, err := EncodeArgs(args)
		if err != nil {
			panic(err)
		}
		encodedArgs = encoded
	}

	cPtr, err := baml_go.CallCollectorFunction(nil, collectorType, "new", encodedArgs)
	if err != nil {
		panic(err)
	}
	return newCollector(cPtr)
}

// ---- implementation --------------------------------------------------------

type collector struct {
	c    unsafe.Pointer
	refs int32  // reference count (wrapper itself == 1)
	once uint32 // ensure destroy runs exactly once
}

func (c *collector) id() int64 {
	return int64(uintptr(c.c))
}

func newCollector(cPtr unsafe.Pointer) *collector {
	col := &collector{c: cPtr, refs: 1}

	// Tell the GC “this Go object represents ≈nativeBytes of memory”.
	// With < Go-1.22 you’d skip the 3rd arg and instead keep a []byte of the
	// same size inside the struct to bias the GC.
	runtime.SetFinalizer(col, (*collector).finalize)

	return col
}

func (c *collector) finalize() {
	// Drop our own ref; if zero we own the final destruction.
	if atomic.AddInt32(&c.refs, -1) == 0 {
		c.destroy()
	}
}

func (c *collector) destroy() {
	// Make absolutely sure we never double-free from racing finalizers.
	if atomic.CompareAndSwapUint32(&c.once, 0, 1) {
		baml_go.CallCollectorFunction(c.c, collectorType, "destroy", nil)
	}
}

func (c *collector) Usage() (*Usage, error) {
	uPtr, err := baml_go.CallCollectorFunction(c.c, collectorType, "usage", nil)
	if err != nil {
		return nil, err
	}

	atomic.AddInt32(&c.refs, 1) // Usage holds an extra reference
	u := &Usage{c: uPtr, parent: c}
	runtime.SetFinalizer(u, (*Usage).finalize)
	return u, nil
}

func (c *collector) Name() (string, error) {
	namePtr, err := baml_go.CallCollectorFunction(c.c, collectorType, "name", nil)
	if err != nil {
		return "", err
	}
	if namePtr == nil {
		return "", nil
	}

	ret := cStringToGoString(namePtr)
	fmt.Println("name", ret)

	// The returned pointer is a CString that we need to free
	defer baml_go.CallCollectorFunction(namePtr, "string", "destroy", nil)

	// Convert C string to Go string
	return ret, nil
}

func (c *collector) Logs() ([]*FunctionLog, error) {
	countPtr, err := baml_go.CallCollectorFunction(c.c, collectorType, "logs_count", nil)
	if err != nil {
		return nil, err
	}

	count := int(uintptr(countPtr))
	logs := make([]*FunctionLog, count)

	for i := 0; i < count; i++ {
		args := BamlFunctionArguments{
			Kwargs: map[string]any{"index": i},
		}
		encodedArgs, err := EncodeArgs(args)
		if err != nil {
			return nil, err
		}

		logPtr, err := baml_go.CallCollectorFunction(c.c, collectorType, "log_at", encodedArgs)
		if err != nil {
			return nil, err
		}

		if logPtr != nil {
			atomic.AddInt32(&c.refs, 1) // FunctionLog holds an extra reference
			log := &FunctionLog{c: logPtr, parent: c}
			runtime.SetFinalizer(log, (*FunctionLog).finalize)
			logs[i] = log
		}
	}

	return logs, nil
}

func (c *collector) Last() (*FunctionLog, error) {
	logPtr, err := baml_go.CallCollectorFunction(c.c, collectorType, "last", nil)
	if err != nil {
		return nil, err
	}

	if logPtr == nil {
		return nil, nil
	}

	atomic.AddInt32(&c.refs, 1) // FunctionLog holds an extra reference
	log := &FunctionLog{c: logPtr, parent: c}
	runtime.SetFinalizer(log, (*FunctionLog).finalize)
	return log, nil
}

func (c *collector) ID(functionID string) (*FunctionLog, error) {
	args := BamlFunctionArguments{
		Kwargs: map[string]any{"function_id": functionID},
	}
	encodedArgs, err := EncodeArgs(args)
	if err != nil {
		return nil, err
	}

	logPtr, err := baml_go.CallCollectorFunction(c.c, collectorType, "id", encodedArgs)
	if err != nil {
		return nil, err
	}

	if logPtr == nil {
		return nil, nil
	}

	atomic.AddInt32(&c.refs, 1) // FunctionLog holds an extra reference
	log := &FunctionLog{c: logPtr, parent: c}
	runtime.SetFinalizer(log, (*FunctionLog).finalize)
	return log, nil
}

func (c *collector) Clear() error {
	_, err := baml_go.CallCollectorFunction(c.c, collectorType, "clear", nil)
	return err
}

// ---- Helper functions ------------------------------------------------------

func cStringToGoString(ptr unsafe.Pointer) string {
	if ptr == nil {
		return ""
	}
	result := C.GoString((*C.char)(ptr))
	// Free the C string since Rust transferred ownership to us
	C.free(ptr)
	return result
}

// ---- FunctionLog wrapper --------------------------------------------------

type FunctionLog struct {
	c      unsafe.Pointer
	parent *collector
}

func (f *FunctionLog) finalize() {
	baml_go.CallCollectorFunction(f.c, functionLogType, "destroy", nil)
	f.parent.finalize()
}

func (f *FunctionLog) ID() (string, error) {
	idPtr, err := baml_go.CallCollectorFunction(f.c, functionLogType, "id", nil)
	if err != nil {
		return "", err
	}
	if idPtr == nil {
		return "", nil
	}
	defer baml_go.CallCollectorFunction(idPtr, "string", "destroy", nil)
	return cStringToGoString(idPtr), nil
}

func (f *FunctionLog) FunctionName() (string, error) {
	namePtr, err := baml_go.CallCollectorFunction(f.c, functionLogType, "function_name", nil)
	if err != nil {
		return "", err
	}
	if namePtr == nil {
		return "", nil
	}
	defer baml_go.CallCollectorFunction(namePtr, "string", "destroy", nil)
	return cStringToGoString(namePtr), nil
}

func (f *FunctionLog) LogType() (string, error) {
	typePtr, err := baml_go.CallCollectorFunction(f.c, functionLogType, "log_type", nil)
	if err != nil {
		return "", err
	}
	if typePtr == nil {
		return "", nil
	}
	defer baml_go.CallCollectorFunction(typePtr, "string", "destroy", nil)
	return cStringToGoString(typePtr), nil
}

func (f *FunctionLog) Timing() (*Timing, error) {
	timingPtr, err := baml_go.CallCollectorFunction(f.c, functionLogType, "timing", nil)
	if err != nil {
		return nil, err
	}
	if timingPtr == nil {
		return nil, nil
	}

	timing := &Timing{c: timingPtr, parent: f}
	runtime.SetFinalizer(timing, (*Timing).finalize)
	return timing, nil
}

func (f *FunctionLog) Usage() (*Usage, error) {
	usagePtr, err := baml_go.CallCollectorFunction(f.c, functionLogType, "usage", nil)
	if err != nil {
		return nil, err
	}
	if usagePtr == nil {
		return nil, nil
	}

	usage := &Usage{c: usagePtr, parent: f.parent}
	runtime.SetFinalizer(usage, (*Usage).finalize)
	return usage, nil
}

func (f *FunctionLog) RawLLMResponse() (string, error) {
	respPtr, err := baml_go.CallCollectorFunction(f.c, functionLogType, "raw_llm_response", nil)
	if err != nil {
		return "", err
	}
	if respPtr == nil {
		return "", nil
	}
	defer baml_go.CallCollectorFunction(respPtr, "string", "destroy", nil)
	return cStringToGoString(respPtr), nil
}

func (f *FunctionLog) CallsCount() (int, error) {
	countPtr, err := baml_go.CallCollectorFunction(f.c, functionLogType, "calls_count", nil)
	if err != nil {
		return 0, err
	}
	return int(uintptr(countPtr)), nil
}

func (f *FunctionLog) Metadata() (map[string]interface{}, error) {
	metaPtr, err := baml_go.CallCollectorFunction(f.c, functionLogType, "metadata", nil)
	if err != nil {
		return nil, err
	}
	if metaPtr == nil {
		return nil, nil
	}
	defer baml_go.CallCollectorFunction(metaPtr, "string", "destroy", nil)

	jsonStr := cStringToGoString(metaPtr)
	var metadata map[string]interface{}
	err = json.Unmarshal([]byte(jsonStr), &metadata)
	if err != nil {
		return nil, err
	}
	return metadata, nil
}

func (f *FunctionLog) Calls() ([]*LLMCall, error) {
	count, err := f.CallsCount()
	if err != nil {
		return nil, err
	}

	calls := make([]*LLMCall, count)
	for i := range count {
		args := BamlFunctionArguments{
			Kwargs: map[string]any{"index": i},
		}
		encodedArgs, err := EncodeArgs(args)
		if err != nil {
			return nil, err
		}

		callPtr, err := baml_go.CallCollectorFunction(f.c, functionLogType, "call_at", encodedArgs)
		if err != nil {
			return nil, err
		}

		if callPtr != nil {
			call := &LLMCall{c: callPtr, parent: f}
			runtime.SetFinalizer(call, (*LLMCall).finalize)
			calls[i] = call
		}
	}

	return calls, nil
}

func (f *FunctionLog) SelectedCall() (*LLMCall, error) {
	callPtr, err := baml_go.CallCollectorFunction(f.c, functionLogType, "selected_call", nil)
	if err != nil {
		return nil, err
	}

	if callPtr == nil {
		return nil, nil
	}

	call := &LLMCall{c: callPtr, parent: f}
	runtime.SetFinalizer(call, (*LLMCall).finalize)
	return call, nil
}

// ---- Timing wrapper -------------------------------------------------------

type Timing struct {
	c      unsafe.Pointer
	parent *FunctionLog
}

func (t *Timing) finalize() {
	baml_go.CallCollectorFunction(t.c, timingType, "destroy", nil)
}

func (t *Timing) StartTimeUTCMs() (int64, error) {
	ptr, err := baml_go.CallCollectorFunction(t.c, timingType, "start_time_utc_ms", nil)
	if err != nil {
		return 0, err
	}
	return int64(uintptr(ptr)), nil
}

func (t *Timing) DurationMs() (int64, error) {
	ptr, err := baml_go.CallCollectorFunction(t.c, timingType, "duration_ms", nil)
	if err != nil {
		return 0, err
	}
	return int64(uintptr(ptr)), nil
}

// ---- LLMCall wrapper -------------------------------------------------------

type LLMCall struct {
	c      unsafe.Pointer
	parent *FunctionLog
}

func (l *LLMCall) finalize() {
	baml_go.CallCollectorFunction(l.c, llmCallType, "destroy", nil)
}

func (l *LLMCall) ClientName() (string, error) {
	namePtr, err := baml_go.CallCollectorFunction(l.c, llmCallType, "client_name", nil)
	if err != nil {
		return "", err
	}
	if namePtr == nil {
		return "", nil
	}
	return cStringToGoString(namePtr), nil
}

func (l *LLMCall) Provider() (string, error) {
	providerPtr, err := baml_go.CallCollectorFunction(l.c, llmCallType, "provider", nil)
	if err != nil {
		return "", err
	}
	if providerPtr == nil {
		return "", nil
	}
	return cStringToGoString(providerPtr), nil
}

func (l *LLMCall) Selected() (bool, error) {
	selectedPtr, err := baml_go.CallCollectorFunction(l.c, llmCallType, "selected", nil)
	if err != nil {
		return false, err
	}
	return uintptr(selectedPtr) != 0, nil
}

func (l *LLMCall) Timing() (*Timing, error) {
	timingPtr, err := baml_go.CallCollectorFunction(l.c, llmCallType, "timing", nil)
	if err != nil {
		return nil, err
	}
	if timingPtr == nil {
		return nil, nil
	}

	timing := &Timing{c: timingPtr, parent: l.parent}
	runtime.SetFinalizer(timing, (*Timing).finalize)
	return timing, nil
}

func (l *LLMCall) Usage() (*Usage, error) {
	usagePtr, err := baml_go.CallCollectorFunction(l.c, llmCallType, "usage", nil)
	if err != nil {
		return nil, err
	}
	if usagePtr == nil {
		return nil, nil
	}

	usage := &Usage{c: usagePtr, parent: l.parent.parent}
	runtime.SetFinalizer(usage, (*Usage).finalize)
	return usage, nil
}

func (l *LLMCall) HTTPRequest() (*HTTPRequest, error) {
	reqPtr, err := baml_go.CallCollectorFunction(l.c, llmCallType, "http_request", nil)
	if err != nil {
		return nil, err
	}
	if reqPtr == nil {
		return nil, nil
	}

	req := &HTTPRequest{c: reqPtr, parent: l}
	runtime.SetFinalizer(req, (*HTTPRequest).finalize)
	return req, nil
}

func (l *LLMCall) HTTPResponse() (*HTTPResponse, error) {
	respPtr, err := baml_go.CallCollectorFunction(l.c, llmCallType, "http_response", nil)
	if err != nil {
		return nil, err
	}
	if respPtr == nil {
		return nil, nil
	}

	resp := &HTTPResponse{c: respPtr, parent: l}
	runtime.SetFinalizer(resp, (*HTTPResponse).finalize)
	return resp, nil
}

func (l *LLMCall) SSEResponses() ([]*SSEResponse, error) {
	// Get count of SSE responses directly from the LLM call
	countPtr, err := baml_go.CallCollectorFunction(l.c, llmCallType, "sse_responses_count", nil)
	if err != nil {
		return nil, err
	}
	
	count := int(uintptr(countPtr))
	if count == 0 {
		return nil, nil
	}
	
	responses := make([]*SSEResponse, count)
	
	for i := 0; i < count; i++ {
		args := BamlFunctionArguments{
			Kwargs: map[string]any{"index": i},
		}
		encodedArgs, err := EncodeArgs(args)
		if err != nil {
			return nil, err
		}
		
		ssePtr, err := baml_go.CallCollectorFunction(l.c, llmCallType, "sse_response_at", encodedArgs)
		if err != nil {
			return nil, err
		}
		
		if ssePtr != nil {
			sse := &SSEResponse{c: ssePtr, parent: l}
			runtime.SetFinalizer(sse, (*SSEResponse).finalize)
			responses[i] = sse
		}
	}
	
	return responses, nil
}

// ---- SSEResponse wrapper --------------------------------------------------

type SSEResponse struct {
	c      unsafe.Pointer
	parent *LLMCall
}

func (s *SSEResponse) finalize() {
	baml_go.CallCollectorFunction(s.c, sseResponseType, "destroy", nil)
}

func (s *SSEResponse) Text() (string, error) {
	textPtr, err := baml_go.CallCollectorFunction(s.c, sseResponseType, "text", nil)
	if err != nil {
		return "", err
	}
	if textPtr == nil {
		return "", nil
	}
	return cStringToGoString(textPtr), nil
}

func (s *SSEResponse) JSON() (any, error) {
	jsonPtr, err := baml_go.CallCollectorFunction(s.c, sseResponseType, "json", nil)
	if err != nil {
		return nil, err
	}
	if jsonPtr == nil {
		return nil, nil
	}
	
	jsonStr := cStringToGoString(jsonPtr)
	var result any
	err = json.Unmarshal([]byte(jsonStr), &result)
	if err != nil {
		return nil, err
	}
	return result, nil
}

// ---- HTTPRequest wrapper --------------------------------------------------

type HTTPRequest struct {
	c      unsafe.Pointer
	parent *LLMCall
}

func (h *HTTPRequest) finalize() {
	baml_go.CallCollectorFunction(h.c, httpRequestType, "destroy", nil)
}

func (h *HTTPRequest) ID() (string, error) {
	idPtr, err := baml_go.CallCollectorFunction(h.c, httpRequestType, "id", nil)
	if err != nil {
		return "", err
	}
	if idPtr == nil {
		return "", nil
	}
	return cStringToGoString(idPtr), nil
}

func (h *HTTPRequest) URL() (string, error) {
	urlPtr, err := baml_go.CallCollectorFunction(h.c, httpRequestType, "url", nil)
	if err != nil {
		return "", err
	}
	if urlPtr == nil {
		return "", nil
	}
	return cStringToGoString(urlPtr), nil
}

func (h *HTTPRequest) Method() (string, error) {
	methodPtr, err := baml_go.CallCollectorFunction(h.c, httpRequestType, "method", nil)
	if err != nil {
		return "", err
	}
	if methodPtr == nil {
		return "", nil
	}
	return cStringToGoString(methodPtr), nil
}

func (h *HTTPRequest) Headers() (map[string]any, error) {
	headersPtr, err := baml_go.CallCollectorFunction(h.c, httpRequestType, "headers", nil)
	if err != nil {
		return nil, err
	}
	if headersPtr == nil {
		return nil, nil
	}

	jsonStr := cStringToGoString(headersPtr)
	var headers map[string]any
	err = json.Unmarshal([]byte(jsonStr), &headers)
	if err != nil {
		return nil, err
	}
	return headers, nil
}

func (h *HTTPRequest) Body() (*HTTPBody, error) {
	bodyPtr, err := baml_go.CallCollectorFunction(h.c, httpRequestType, "body", nil)
	if err != nil {
		return nil, err
	}
	if bodyPtr == nil {
		return nil, nil
	}

	body := &HTTPBody{c: bodyPtr, parent: h}
	runtime.SetFinalizer(body, (*HTTPBody).finalize)
	return body, nil
}

// ---- HTTPResponse wrapper -------------------------------------------------

type HTTPResponse struct {
	c      unsafe.Pointer
	parent *LLMCall
}

func (h *HTTPResponse) finalize() {
	baml_go.CallCollectorFunction(h.c, httpResponseType, "destroy", nil)
}

func (h *HTTPResponse) Status() (int, error) {
	statusPtr, err := baml_go.CallCollectorFunction(h.c, httpResponseType, "status", nil)
	if err != nil {
		return 0, err
	}
	return int(uintptr(statusPtr)), nil
}

func (h *HTTPResponse) Headers() (map[string]any, error) {
	headersPtr, err := baml_go.CallCollectorFunction(h.c, httpResponseType, "headers", nil)
	if err != nil {
		return nil, err
	}
	if headersPtr == nil {
		return nil, nil
	}

	jsonStr := cStringToGoString(headersPtr)
	var headers map[string]any
	err = json.Unmarshal([]byte(jsonStr), &headers)
	if err != nil {
		return nil, err
	}
	return headers, nil
}

func (h *HTTPResponse) Body() (*HTTPBody, error) {
	bodyPtr, err := baml_go.CallCollectorFunction(h.c, httpResponseType, "body", nil)
	if err != nil {
		return nil, err
	}
	if bodyPtr == nil {
		return nil, nil
	}

	body := &HTTPBody{c: bodyPtr, parent: h}
	runtime.SetFinalizer(body, (*HTTPBody).finalize)
	return body, nil
}

// ---- HTTPBody wrapper -----------------------------------------------------

type HTTPBody struct {
	c      unsafe.Pointer
	parent interface{} // Can be *HTTPRequest or *HTTPResponse
}

func (h *HTTPBody) finalize() {
	baml_go.CallCollectorFunction(h.c, httpBodyType, "destroy", nil)
}

func (h *HTTPBody) Raw() ([]byte, error) {
	// This would need special handling for binary data
	textPtr, err := baml_go.CallCollectorFunction(h.c, httpBodyType, "text", nil)
	if err != nil {
		return nil, err
	}
	if textPtr == nil {
		return nil, nil
	}

	text := cStringToGoString(textPtr)
	return []byte(text), nil
}

func (h *HTTPBody) Text() (string, error) {
	textPtr, err := baml_go.CallCollectorFunction(h.c, httpBodyType, "text", nil)
	if err != nil {
		return "", err
	}
	if textPtr == nil {
		return "", nil
	}
	return cStringToGoString(textPtr), nil
}

func (h *HTTPBody) JSON() (any, error) {
	jsonPtr, err := baml_go.CallCollectorFunction(h.c, httpBodyType, "json", nil)
	if err != nil {
		return nil, err
	}
	if jsonPtr == nil {
		return nil, nil
	}

	jsonStr := cStringToGoString(jsonPtr)
	var result any
	err = json.Unmarshal([]byte(jsonStr), &result)
	if err != nil {
		return nil, err
	}
	return result, nil
}

// ---- Usage wrapper ---------------------------------------------------------

type Usage struct {
	c      unsafe.Pointer
	parent *collector
}

func (u *Usage) finalize() {
	// Destroy the C-side Usage first …
	baml_go.CallCollectorFunction(u.c, usageType, "destroy", nil)
	// … then drop the extra reference on the collector.
	u.parent.finalize()
}

func (u *Usage) InputTokens() (int, error) {
	ptr, err := baml_go.CallCollectorFunction(u.c, usageType, "input_tokens", nil)
	if err != nil {
		return 0, err
	}
	return int(uintptr(ptr)), nil // NB: safer to have the C layer return C.int
}

func (u *Usage) OutputTokens() (int, error) {
	ptr, err := baml_go.CallCollectorFunction(u.c, usageType, "output_tokens", nil)
	if err != nil {
		return 0, err
	}
	return int(uintptr(ptr)), nil
}

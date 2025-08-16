---
date: 2025-08-16T13:11:09-07:00
researcher: dex
git_commit: 4d80a47208a98378037c6abce7efa45e32707841
branch: canary
repository: baml
topic: "Add support for abort handlers into the BAML runtime and expose it to every language client"
tags: [research, codebase, abort-handlers, streaming, runtime, typescript, python, go]
status: complete
last_updated: 2025-01-16
last_updated_by: dex
last_updated_note: "Removed Ruby analysis, clarified unified Rust AbortController approach for all languages"
implementation_notes: |
  - Ruby implementation is being ignored/deprioritized
  - All languages will use the same Rust-based AbortController implementation exposed via FFI
  - Python will NOT use task.cancel() since BAML intentionally doesn't create tasks
  - Custom AbortController will be implemented in Rust and wrapped by each language
---

# Research: Add support for abort handlers into the BAML runtime and expose it to every language client

**Date**: 2025-08-16T13:11:09-07:00
**Researcher**: dex
**Git Commit**: 4d80a47208a98378037c6abce7efa45e32707841
**Branch**: canary
**Repository**: baml

## Research Question
Add an AbortController similar to that of JavaScript with the following proposed solution:
```typescript
import { b } from 'baml_client'
import { AbortController, AbortError } from '@boundaryml/baml'

const controller = new AbortController()
b.CallLlmFunction("Input", { controller }) 
    .then(response => console.log(response))
    .catch(err => {
        if (err instanceof AbortError) {
            console.log("Request Aborted")  
        }
    })

if (somethingHappened) {
    controller.abort()
}
```

Focus primarily on understanding the code base, all the file paths that matter, and how information flows through the system for all language clients, with initial implementation focus on TypeScript.

## Summary

The BAML runtime is well-architected for adding abort handler support. The system uses a Rust runtime core with language-specific clients (TypeScript, Python, Ruby, Go) connected via FFI. Streaming is already designed with cancellation in mind - the `stream-cancel` crate is integrated, and `FunctionResultStream` is intentionally decoupled from runtime lifetime for easier cancellation. 

**The primary integration point for abort handlers is at the orchestrator level**, not at the HTTP or stream processing sites. This is critical because:

1. **Orchestrators manage retry and fallback chains** - Aborting at the orchestrator level ensures all retry attempts and fallback providers are cancelled, not just the current HTTP request
2. **Unified control flow** - Both `orchestrate()` and `orchestrate_stream()` iterate through provider nodes, making it the natural place for abort logic
3. **Prevents wasted resources** - Stops the orchestrator from attempting additional providers after abort is triggered

The key integration points are:
1. **Orchestrator functions** (`engine/baml-runtime/src/internal/llm_client/orchestrator/call.rs:45-135` and `stream.rs:27-196`) - PRIMARY integration point
2. **Rust AbortController** - Unified cancellation token implementation in Rust, exposed to all languages
3. **Language client bridges** - TypeScript via NAPI, Python via PyO3, Go via CFFI (Ruby deprioritized)
4. **Client streaming classes** - `BamlStream` implementations in each language

## Detailed Findings

### TypeScript Client Implementation

#### Core Components & Data Flow

**Key Files:**
- `engine/language_client_typescript/typescript_src/stream.ts` - BamlStream class
- `integ-tests/typescript/baml_client/async_client.ts` - Generated client code
- `engine/language_client_typescript/src/runtime.rs` - Rust-TypeScript bridge
- `engine/baml-runtime/src/types/stream.rs` - Core streaming implementation

**Data Flow:**
1. TypeScript Client → Generated `BamlAsyncClient` methods call `runtime.callFunction()` or `runtime.streamFunction()`
2. Rust Bridge → TypeScript calls bridged to Rust via NAPI at `engine/language_client_typescript/src/runtime.rs:198`
3. Core Runtime → Rust runtime creates `FunctionResultStream` at `engine/baml-runtime/src/types/stream.rs:29`
4. Orchestrator → Calls `orchestrate_stream()` at `engine/baml-runtime/src/internal/llm_client/orchestrator/stream.rs:27`

#### BamlStream Interface & Event Handling

**Core Methods (`engine/language_client_typescript/typescript_src/stream.ts`):**
- `async *[Symbol.asyncIterator]()` - AsyncIterator for streaming partial results (lines 48-67)
- `getFinalResponse()` - Get final result after stream completion (lines 69-73)
- `toStreamable()` - Convert to Next.js ReadableStream (lines 83-128)

**Event Processing:**
- Events queued in `eventQueue` via `ffiStream.onEvent()` callback (lines 22-30)
- Background task drives stream completion via `driveToCompletion()` (lines 20-38)
- AsyncIterator polls `eventQueue` with 100ms timeout (lines 54-56)

### Runtime Core Architecture

#### Function Execution Entry Points

**Main Runtime Interface** (`engine/baml-runtime/src/runtime_interface.rs`):
- Core methods: `render_prompt()`, `get_function()`, `orchestration_graph()`

**Function Execution Methods** (`engine/baml-runtime/src/runtime_methods/`):
- `call_function.rs:46-69` - Main synchronous function execution
- `stream_function.rs:44-106` - Streaming function execution

#### Orchestration Layer - PRIMARY ABORT INTEGRATION POINT

**Orchestration Engine** (`engine/baml-runtime/src/internal/llm_client/orchestrator/`):
- `call.rs:45-135` - Manages synchronous LLM calls with retry/fallback logic
- `stream.rs:27-196` - Manages streaming LLM calls with real-time event processing

**This is the correct integration point for abort handlers** because orchestrators control the entire execution flow including retries and fallbacks.

##### Synchronous Orchestration (`call.rs`)
```rust
pub async fn orchestrate(
    iter: OrchestratorNodeIterator,
    ir: &IntermediateRepr,
    ctx: &RuntimeContext,
    prompt: &PromptRenderer,
    params: &BamlValue,
    parse_fn: impl Fn(&str) -> Result<ResponseBamlValue>,
    // ADD: abort_token: Option<Arc<tokio_util::sync::CancellationToken>>,
) -> (Vec<(OrchestrationScope, LLMResponse, Option<Result<ResponseBamlValue>>)>, Duration) {
    let mut results = Vec::new();
    
    for node in iter {
        // ADD: Check abort before each provider attempt
        // if let Some(ref token) = abort_token {
        //     if token.is_cancelled() {
        //         results.push((node.scope, LLMResponse::Cancelled, None));
        //         break;
        //     }
        // }
        
        let response = node.single_call(&ctx, &prompt).await;
        // ... existing logic
        
        // Lines 123-131: Break on success or sleep between retries
        if matches!(response, LLMResponse::Success(_)) {
            break;
        } else if let Some(duration) = sleep_duration {
            // ADD: Abort-aware sleep
            // tokio::select! {
            //     _ = async_std::task::sleep(duration) => {},
            //     _ = abort_token.cancelled() => break,
            // }
        }
    }
}
```

##### Streaming Orchestration (`stream.rs`)
```rust
pub async fn orchestrate_stream<F, G>(
    iter: OrchestratorNodeIterator,
    // ... existing params
    on_event: Option<F>,
    // ADD: abort_token: Option<Arc<tokio_util::sync::CancellationToken>>,
) -> (Vec<(OrchestrationScope, LLMResponse, Option<Result<ResponseBamlValue>>)>, Duration) {
    
    for node in iter {
        // ADD: Check abort before each provider attempt
        // if let Some(ref token) = abort_token {
        //     if token.is_cancelled() {
        //         results.push((node.scope, LLMResponse::Cancelled, None));
        //         break;
        //     }
        // }
        
        let stream_res = node.stream(&ctx, &prompt).await;
        
        // Lines 70-94: Process stream with abort checking
        let final_response = match stream_res {
            Ok(response) => response
                .take_while(|_| {
                    // ADD: Check abort during streaming
                    // abort_token.as_ref().map_or(true, |t| !t.is_cancelled())
                })
                .map(|stream_part| { /* existing logic */ })
                .fold(None, |_, current| Some(current))
                .await,
            Err(response) => response,
        };
    }
}
```

#### Why NOT HTTP Request or Stream Processing Sites

##### HTTP Request Execution (`request.rs`) - NOT the right place
**File**: `engine/baml-runtime/src/internal/llm_client/primitive/request.rs`

**Why this is NOT the primary integration point:**
1. **Too low-level** - This only cancels a single HTTP request, not the entire orchestration chain
2. **Misses retries** - Orchestrator would continue with retry attempts even after one request is aborted
3. **Misses fallbacks** - Would still attempt fallback providers after primary provider is aborted
4. **Incomplete cancellation** - User expects abort to stop all LLM attempts, not just the current one

##### Stream Processing (`stream_request.rs`) - NOT the right place
**File**: `engine/baml-runtime/src/internal/llm_client/primitive/stream_request.rs`

**Why this is NOT the primary integration point:**
1. **Only affects streaming** - Doesn't handle non-streaming calls
2. **Too granular** - Only cancels individual stream processing, not the orchestration
3. **Timing issues** - Stream might already be established before abort is triggered
4. **Inconsistent behavior** - Different cancellation points for streaming vs non-streaming

### Cross-Language Client Architecture

#### Python Client (PyO3 Bridge)

**Interface** (`engine/language_client_python/src/runtime.rs:177-285`):
- Async support via `pyo3_async_runtimes::tokio::future_into_py`
- Streaming via `FunctionResultStream` and `SyncFunctionResultStream`

**Stream Implementation** (`engine/language_client_python/python_src/baml_py/stream.py:19-99`):
- `BamlStream` class with async iterator pattern
- Background threads with `concurrent.futures.Future`
- Event queue via `queue.Queue` for streaming partial results

**Cancellation Approach**:
- **NOT using `task.cancel()`** - BAML intentionally doesn't create tasks
- **Rust-based AbortController** exposed via PyO3
- Python wrapper will call into Rust cancellation token
- Clean up resources on cancellation (lines 62-64)
- Compatible with both sync and async Python code

**Why not native Python cancellation**:
- `asyncio.Task.cancel()` requires task creation, which BAML avoids
- Python's cancellation patterns are inconsistent between sync/async
- Rust implementation provides unified behavior across all languages

#### Go Client (CFFI)

**Stream Implementation** (`integ-tests/go/baml_client/functions_stream.go`):
```go
func (*stream) FunctionName(ctx context.Context, ...) (<-chan StreamValue[StreamType, FinalType], error) {
    channel := make(chan StreamValue[StreamType, FinalType])
    go func() {
        for {
            select {
            case <-ctx.Done(): // CANCELLATION MECHANISM
                close(channel)
                return
            case result, ok := <-internal_channel:
                // Process and forward events
            }
        }
    }()
    return channel, nil
}
```

**Already supports context cancellation** via `context.Context`

### Interface Definitions and Protocols

#### CFFI Protocol Buffer

**File**: `engine/language_client_cffi/types/cffi.proto`

Key structures for control signal integration:
- `CFFIValueHolder` (lines 11-36) - Main wrapper for values
- `CFFIFunctionArguments` (lines 207-216) - Function call parameters
- `CFFIObjectType` enum (lines 249-275) - Object type definitions

**Proposed Addition**:
```protobuf
// Add to CFFIObjectType enum
OBJECT_ABORT_SIGNAL = 22;

message CFFIAbortSignal {
  string call_id = 1;
  optional string reason = 2;
}
```

#### Event System

**File**: `engine/baml-lib/baml-types/src/tracing/events.rs`

**TraceEvent** structure (lines 14-33):
- `call_id` - Unique function call identifier
- `function_event_id` - Unique event identifier
- `content` - Event payload

**Proposed Event Type Addition**:
```rust
// Add to TraceData enum
AbortSignal(AbortSignalEvent),

pub struct AbortSignalEvent {
    pub target_call_id: FunctionCallId,
    pub reason: Option<String>,
}
```

## Code References

### Critical Integration Points
- `engine/baml-runtime/src/internal/llm_client/orchestrator/call.rs:45-135` - Sync orchestration (PRIMARY)
- `engine/baml-runtime/src/internal/llm_client/orchestrator/stream.rs:27-196` - Stream orchestration (PRIMARY)
- `engine/baml-runtime/src/runtime_methods/stream_function.rs:44-106` - Function-level streaming
- `engine/language_client_typescript/src/runtime.rs:198` - TypeScript bridge entry
- `engine/language_client_typescript/typescript_src/stream.ts:48-128` - BamlStream implementation

### Language Client Bridges
- `engine/language_client_typescript/src/runtime.rs` - TypeScript NAPI bridge
- `engine/language_client_python/src/runtime.rs` - Python PyO3 bridge
- `engine/language_client_cffi/src/ffi/functions.rs` - Go/CFFI bridge

### Stream Implementations
- `engine/language_client_typescript/typescript_src/stream.ts` - TypeScript BamlStream
- `engine/language_client_python/python_src/baml_py/stream.py` - Python BamlStream
- `integ-tests/go/baml_client/functions_stream.go` - Go streaming

### Test Patterns
- `integ-tests/go/test_functions_streaming_test.go` - Go streaming tests
- `integ-tests/go/test_retries_fallbacks_test.go:120-139` - Context cancellation tests
- `integ-tests/python/tests/test_functions.py:642-677` - Python async streaming
- `integ-tests/typescript/tests/input-output.test.ts` - TypeScript streaming

## Architecture Insights

### Existing Infrastructure Ready for Abort Handlers
1. **Stream-cancel crate integrated** - Already in `Cargo.toml` dependencies
2. **Decoupled stream lifetime** - `FunctionResultStream` designed for cancellation
3. **Event-driven architecture** - Callbacks and event queues already in place
4. **Go context cancellation** - Already implemented and tested

### Unified Rust AbortController Design

All languages will use the same Rust-based `AbortController` implementation:

```rust
// In engine/baml-runtime/src/abort_controller.rs (new file)
pub struct AbortController {
    token: Arc<tokio_util::sync::CancellationToken>,
    signal: Arc<AbortSignal>,
}

pub struct AbortSignal {
    token: Arc<tokio_util::sync::CancellationToken>,
}

impl AbortController {
    pub fn new() -> Self {
        let token = Arc::new(tokio_util::sync::CancellationToken::new());
        Self {
            token: token.clone(),
            signal: Arc::new(AbortSignal { token }),
        }
    }
    
    pub fn signal(&self) -> Arc<AbortSignal> {
        self.signal.clone()
    }
    
    pub fn abort(&self) {
        self.token.cancel();
    }
}

impl AbortSignal {
    pub fn is_aborted(&self) -> bool {
        self.token.is_cancelled()
    }
    
    pub async fn wait_for_abort(&self) {
        self.token.cancelled().await
    }
}
```

This will be exposed to each language via FFI with language-specific wrappers.

### Implementation Strategy

1. **Create Rust AbortController** (Foundation):
   - Implement `AbortController` and `AbortSignal` in Rust
   - Use `tokio_util::sync::CancellationToken` internally
   - Ensure thread-safe operation across FFI boundary

2. **Integrate with Orchestration Layer** (PRIMARY):
   - Add `AbortSignal` parameter to `orchestrate()` and `orchestrate_stream()` functions
   - Check cancellation before each provider node iteration
   - Make sleep operations abort-aware using `tokio::select!`
   - Ensure both retry and fallback chains respect abort signals

3. **Function-Level Integration**: 
   - Add abort parameters to `call_function_impl`/`stream_function_impl`
   - Pass signals down to orchestration layer
   - Create AbortController at runtime entry point

4. **FFI Interface Updates**: 
   - Expose `AbortController` creation and manipulation via FFI
   - Add methods: `create_abort_controller()`, `abort()`, `is_aborted()`
   - Return opaque handles to language clients

5. **Language Client Wrappers**:
   - **TypeScript**: Wrap Rust AbortController to match Web API
     ```typescript
     class AbortController {
       private rustController: RustAbortController;
       get signal(): AbortSignal { return this.rustController.signal(); }
       abort(): void { this.rustController.abort(); }
     }
     ```
   - **Python**: Expose as Python class via PyO3
     ```python
     class AbortController:
         def __init__(self):
             self._rust_controller = create_rust_abort_controller()
         @property
         def signal(self): return self._rust_controller.signal()
         def abort(self): self._rust_controller.abort()
     ```
   - **Go**: Map to context.Context cancellation

6. **Optional HTTP-level cancellation** (Secondary):
   - Once orchestration is cancelled, optionally cancel in-flight HTTP requests
   - Use for immediate resource cleanup, but not as primary abort mechanism

### Language-Specific Integration Examples

**TypeScript Integration**:
```typescript
// User-facing API matches Web standards
import { b } from 'baml_client'
import { AbortController } from '@boundaryml/baml'

const controller = new AbortController() // Creates Rust controller via FFI
const result = await b.CallFunction("input", { 
  signal: controller.signal // Passes Rust AbortSignal
})

// Wrapper implementation
class AbortController {
  private rustController: RustAbortController;
  
  constructor() {
    this.rustController = runtime.createAbortController();
  }
  
  get signal(): AbortSignal {
    return new AbortSignal(this.rustController.getSignal());
  }
  
  abort(): void {
    this.rustController.abort();
  }
}
```

**Python Integration**:
```python
# Async example
from baml_client import b
from baml_py import AbortController

controller = AbortController()  # Creates Rust controller via PyO3

# Pass abort signal via baml_options
result = await b.ExtractResume(
    resume_text,
    baml_options={"abort_signal": controller.signal}
)

# Streaming with abort
stream = b.stream.PromptTestStreaming(
    input="Tell me a story",
    baml_options={"abort_signal": controller.signal}
)

# In another context/thread:
controller.abort()  # Calls into Rust to cancel

# Sync example
from baml_client.sync_client import b as sync_b

controller = AbortController()
result = sync_b.TestFnNamedArgsSingleClass(
    myArg=NamedArgsSingleClass(key="key", key_two=True, key_three=52),
    baml_options={"abort_signal": controller.signal}
)
```

**Go Integration**:
```go
import (
    "context"
    b "example.com/integ-tests/baml_client"
    baml "github.com/boundaryml/baml/engine/language_client_go/pkg"
)

// Go leverages existing context cancellation
ctx, cancel := context.WithCancel(context.Background())

// Non-streaming call
result, err := b.TestOpenAIGPT4oMini(
    ctx,  // Context already supports cancellation
    "test input",
    b.WithCollector(collector),
)

// Streaming call
stream, err := b.Stream.PromptTestStreaming(ctx, "Tell me a story")

// Cancel from another goroutine
go func() {
    time.Sleep(5 * time.Second)
    cancel()  // This aborts the call via context
}()

// Stream automatically closes on context cancellation
for value := range stream {
    if value.IsError {
        // Check if error is due to context cancellation
        if errors.Is(value.Error, context.Canceled) {
            log.Println("Stream aborted")
            break
        }
    }
    // Process stream values
}

// Alternative: Using custom AbortController for consistency
controller := baml.NewAbortController()
result, err := b.TestFunction(
    ctx,
    "input",
    b.WithAbortSignal(controller.Signal()),  // New option
)

// Abort from another goroutine
controller.Abort()
```

## Orchestrator Integration Rationale

### Why Orchestrator Level is Correct

The orchestrator functions (`orchestrate()` and `orchestrate_stream()`) are the correct integration points for abort handlers for several critical reasons:

1. **Complete Control Over Execution Flow**
   - Orchestrators iterate through multiple provider nodes (retries and fallbacks)
   - A single abort signal can stop the entire chain, not just one attempt
   - Prevents resource waste from unnecessary retry/fallback attempts

2. **Unified Abort Semantics**
   - Both streaming and non-streaming calls go through orchestrators
   - Single integration point ensures consistent abort behavior
   - Users expect abort to stop all LLM attempts, which orchestrators control

3. **Clean Integration with Existing Loop Structure**
   - Orchestrators already have a for loop over nodes (lines 63 and 53 respectively)
   - Natural place to check cancellation at each iteration
   - Sleep operations between retries can be made abort-aware

4. **Proper Error Propagation**
   - Orchestrators return structured results with `LLMResponse` enum
   - Can add `LLMResponse::Cancelled` variant for clean abort handling
   - Maintains error context and tracing through abort

### Why NOT Lower-Level Integration Points

**HTTP Request Level (`execute_request`):**
- Would only cancel individual HTTP requests
- Orchestrator would continue with retries/fallbacks
- Requires abort logic duplication across all provider implementations
- Misses non-HTTP providers or future transport mechanisms

**Stream Processing Level (`make_stream_request`):**
- Only handles streaming, not synchronous calls
- Too late in the pipeline - stream already established
- Doesn't prevent retry/fallback attempts
- Complex to coordinate with orchestration loop

## Historical Context

The codebase already has the `stream-cancel` crate integrated (version 0.8.2) in both `baml-runtime` and `cli` Cargo.toml files. The streaming architecture was explicitly designed with cancellation in mind, as noted in the comment: "We decouple its lifetime from that of BamlRuntime because we want to make it easy for users to cancel the stream" (`engine/baml-runtime/src/types/stream.rs`).

## Related Research

This is the first comprehensive research document on abort handlers in the BAML runtime. Future research should explore:
- Performance implications of abort handlers
- Best practices for cleanup in each language
- Integration with existing timeout mechanisms

## Open Questions

1. Should abort handlers be synchronous or asynchronous in each language?
2. How should partial results be handled when a stream is aborted?
3. Should there be a grace period before hard cancellation of HTTP requests?
4. How to handle abort during retry/fallback sequences?
5. Should abort reasons be structured (enum) or free-form strings?
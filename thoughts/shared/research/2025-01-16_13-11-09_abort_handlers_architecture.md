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

### Unified Rust Cancellation Token with Language-Specific Controllers

While the Rust runtime uses a unified cancellation token internally, each language exposes cancellation in its idiomatic way:
- **TypeScript**: Uses native Web API `AbortController`  
- **Python**: Custom `AbortController` via PyO3
- **Go**: Native `context.Context` cancellation

The Rust runtime bridges these different patterns to a common implementation:

#### TypeScript-to-Rust Bridge (NAPI)
```rust
// In engine/language_client_typescript/src/runtime.rs
fn js_abort_signal_to_rust_token(
    env: Env, 
    signal: JsObject
) -> napi::Result<Arc<tokio_util::sync::CancellationToken>> {
    let token = Arc::new(tokio_util::sync::CancellationToken::new());
    
    // Check if signal is already aborted
    let aborted: bool = signal.get_named_property("aborted")?;
    if aborted {
        token.cancel();
        return Ok(token);
    }
    
    // Listen to 'abort' event on JS signal
    let token_clone = token.clone();
    let callback = env.create_function_from_closure("abort_handler", move |_| {
        token_clone.cancel();
        Ok(())
    })?;
    
    // signal.addEventListener('abort', callback)
    let add_event_listener: JsFunction = signal.get_named_property("addEventListener")?;
    add_event_listener.call(Some(&signal), &[
        env.create_string("abort")?.into_unknown(),
        callback.into_unknown()
    ])?;
    
    Ok(token)
}
```

#### Python AbortController (PyO3)
```rust
// In engine/language_client_python/src/abort_controller.rs
#[pyclass]
pub struct AbortController {
    token: Arc<tokio_util::sync::CancellationToken>,
}

#[pymethods]
impl AbortController {
    #[new]
    fn new() -> Self {
        Self {
            token: Arc::new(tokio_util::sync::CancellationToken::new()),
        }
    }
    
    fn abort(&self) {
        self.token.cancel();
    }
    
    #[getter]
    fn signal(&self) -> AbortSignal {
        AbortSignal { 
            token: self.token.clone() 
        }
    }
}
```

#### Go Context Bridge
```go
// Go's context.Context is directly passed through to Rust
// The Rust FFI layer converts context.Done() to CancellationToken
```

### Implementation Strategy

1. **Language-Specific Controller Implementation**:
   - **TypeScript**: Bridge native `AbortController` to Rust via NAPI
     - Listen to native 'abort' event
     - Convert to `tokio_util::sync::CancellationToken`
   - **Python**: Create custom `AbortController` in Rust exposed via PyO3
     - Implement Python class wrapping Rust token
   - **Go**: Use existing `context.Context` cancellation
     - Already works, minimal changes needed

2. **Integrate with Orchestration Layer** (PRIMARY):
   - Add cancellation token parameter to `orchestrate()` and `orchestrate_stream()` functions
   - Check cancellation before each provider node iteration
   - Make sleep operations abort-aware using `tokio::select!`
   - Ensure both retry and fallback chains respect abort signals

3. **Function-Level Integration**: 
   - Add abort parameters to `call_function_impl`/`stream_function_impl`
   - Pass tokens down to orchestration layer
   - Extract cancellation token from language-specific sources

4. **FFI Bridge Updates**: 
   - **TypeScript**: Accept native `JsObject` (AbortSignal) via NAPI
   - **Python**: Expose `AbortController` class via PyO3
   - **Go**: Continue using context.Context

5. **Generated Code Updates**:
   - **TypeScript**: Extract signal from native AbortController
   - **Python**: Extract signal from custom AbortController
   - **Go**: No changes needed (context already works)

6. **Optional HTTP-level cancellation** (Secondary):
   - Once orchestration is cancelled, optionally cancel in-flight HTTP requests
   - Use for immediate resource cleanup, but not as primary abort mechanism

### Language-Specific Integration Examples

**TypeScript Integration (Native AbortController)**:
```typescript
// User-facing API uses NATIVE Web API AbortController
import { b } from 'baml_client'
import { BamlAbortError } from '@boundaryml/baml'  // Cross-platform error type

// Use native AbortController - no custom import needed!
const controller = new AbortController() // Native Web API (works in Node.js 15+ and browsers)

// Pass native controller directly
const result = await b.CallFunction("input", { 
  abortController: controller // Native AbortController
})

// Streaming with native controller
const stream = b.stream.PromptTestStreaming("Tell me a story", {
  abortController: controller
})

// Abort using standard Web API
controller.abort() // or controller.abort("Custom reason")

// Error handling with BamlError
try {
  const result = await b.CallFunction(input, { abortController: controller })
} catch (error) {
  if (error instanceof BamlAbortError) {
    console.log('Request was aborted:', error.message)
  }
}

// Advanced: Using native AbortSignal.timeout()
const result = await b.CallFunction(input, {
  abortController: { signal: AbortSignal.timeout(5000) }
})

// In generated client code:
async CallFunction(input: string, options?: { abortController?: AbortController }) {
  const signal = options?.abortController?.signal;
  
  // Check if already aborted
  if (signal?.aborted) {
    throw new BamlAbortError('Operation was aborted', signal.reason);
  }
  
  // Pass signal to Rust runtime via NAPI
  return this.runtime.callFunction("FunctionName", input, signal);
}

// Rust bridge converts JS AbortSignal to CancellationToken:
// - Listens to 'abort' event on JS signal
// - Cancels Rust token when JS signal fires
// - Checks signal.aborted property for pre-aborted signals

// BamlAbortError definition (cross-platform):
class BamlAbortError extends BamlError {
  constructor(message: string, reason?: any) {
    super(message);
    this.name = 'BamlAbortError';
    this.reason = reason;
  }
  reason?: any;
}
```

**Python Integration**:
```python
# Async example
from baml_client import b
from baml_py import AbortController

controller = AbortController()  # Creates Rust controller via PyO3

# Pass entire controller via baml_options
result = await b.ExtractResume(
    resume_text,
    baml_options={"abort_controller": controller}  # Pass controller, not signal
)

# Streaming with abort
stream = b.stream.PromptTestStreaming(
    input="Tell me a story",
    baml_options={"abort_controller": controller}
)

# In another context/thread:
controller.abort()  # Calls into Rust to cancel

# Sync example
from baml_client.sync_client import b as sync_b

controller = AbortController()
result = sync_b.TestFnNamedArgsSingleClass(
    myArg=NamedArgsSingleClass(key="key", key_two=True, key_three=52),
    baml_options={"abort_controller": controller}
)

# Generated code internally extracts signal:
# signal = baml_options.get("abort_controller").signal if "abort_controller" in baml_options else None
```

**Go Integration (Idiomatic Context Approach)**:
```go
import (
    "context"
    b "example.com/integ-tests/baml_client"
)

// Go's context.Context provides cancellation semantics equivalent to AbortController
ctx, cancel := context.WithCancel(context.Background())
defer cancel()

// Non-streaming call
result, err := b.TestOpenAIGPT4oMini(ctx, "test input")
if errors.Is(err, context.Canceled) {
    // Handle cancellation
}

// Streaming call
stream, err := b.Stream.PromptTestStreaming(ctx, "Tell me a story")

// Cancel from another goroutine
go func() {
    time.Sleep(5 * time.Second)
    cancel()  // This aborts the call AND propagates to runtime
}()

// Stream automatically closes on context cancellation
for value := range stream {
    // Handles ctx.Done() and runtime cancellation
}

// Note: User API requires no changes - context already works idiomatically
```

## Go Context Integration and CFFI Propagation

**Important**: Go's `context.Context` already provides cancellation semantics equivalent to AbortController. The BAML Go client properly handles context cancellation at the Go level, but **context cancellation must be propagated to the BAML runtime through CFFI** to actually abort the underlying LLM requests.

### Current State vs Required Changes

**What Works Today:**
1. ✅ User API accepts `context.Context` idiomatically
2. ✅ Generated Go code checks `ctx.Done()` in streaming functions
3. ✅ Channels close properly on context cancellation

**What Needs Implementation:**
1. ❌ Context cancellation not propagated to Rust runtime via CFFI
2. ❌ Streaming functions use `context.Background()` instead of user context
3. ❌ CFFI interface lacks abort signal mechanism

### Go Callback Architecture (from `callbacks.go`)

The Go-Rust bridge uses a callback system with unique IDs:
1. Each BAML function call gets a unique callback ID (`create_unique_id`)
2. The ID maps to a `CallbackData` struct containing the context, channel, and tick callback
3. Rust calls exported Go functions (`trigger_callback`, `error_callback`) with the ID
4. Go uses the ID to look up the context and channel for that specific call
5. **Critical Gap**: When `ctx.Done()` fires, Go closes its channel but doesn't notify Rust

### Required Implementation Changes

#### 1. Callback Cancellation Propagation
**File**: `engine/language_client_go/pkg/callbacks.go:147`
- Replace TODO with actual Rust cancellation call
- Add new exported function for Rust to handle cancellation

#### 2. Rust Cancellation Handler
**File**: `engine/language_client_cffi/src/ffi/functions.rs`
- Add `cancel_operation(id: u32)` function
- Maintain map of callback IDs to cancellation tokens
- Cancel token when Go signals cancellation

#### 3. Generated Code Fix
**File**: `engine/generators/languages/go/src/_templates/function.stream.go.j2:39`
- Change from `context.Background()` to user's context
- Ensure context flows through to callback system

#### 4. Orchestrator Integration
**Files**: As specified in primary integration point
- Pass cancellation tokens from CFFI layer to orchestrator
- Check tokens during retry/fallback iterations

### Key Integration Points

**Critical Finding from `callbacks.go:144-148`:**
```go
// Current implementation in trigger_callback
select {
case <-callback.ctx.Done():
    force_close = true
    callback.channel <- ResultCallback{Error: callback.ctx.Err()}
    // TODO: Somehow tell rust to die  <-- THIS IS THE MISSING PIECE!
    break
case callback.channel <- res:
    break
}
```

The callback system already detects context cancellation but **doesn't propagate it to Rust**. The TODO comment confirms this is a known gap.

### Implementation Solution

The callback system stores contexts with IDs in a map (`dynamicCallbacks`). When context cancels, we need to:

1. **Add a new exported function for cancellation**:
```go
//export cancel_rust_operation
func cancel_rust_operation(id uint32) {
    // This will be called by Go when context cancels
    // Rust will handle the cancellation for this callback ID
}

// In trigger_callback, replace the TODO:
case <-callback.ctx.Done():
    force_close = true
    callback.channel <- ResultCallback{Error: callback.ctx.Err()}
    // Tell Rust to cancel this operation
    baml_go.CancelOperation(id_uint)  // NEW: Propagate to Rust
    break
```

2. **Rust side needs to handle the cancellation**:
```rust
// In engine/language_client_cffi/src/ffi/functions.rs
#[no_mangle]
pub extern "C" fn cancel_operation(id: u32) {
    // Find the operation by ID and cancel its token
    if let Some(token) = OPERATION_TOKENS.get(&id) {
        token.cancel();
    }
}
```

3. **Store cancellation tokens by callback ID**:
```rust
// Track active operations
static OPERATION_TOKENS: Lazy<DashMap<u32, Arc<CancellationToken>>> = 
    Lazy::new(|| DashMap::new());

// When starting an operation:
let token = Arc::new(CancellationToken::new());
OPERATION_TOKENS.insert(id, token.clone());

// Pass token to orchestrator
let result = orchestrate(..., Some(token)).await;

// Clean up when done
OPERATION_TOKENS.remove(&id);
```

Once implemented, Go context cancellation will properly abort the entire orchestration chain (retries, fallbacks) in the Rust runtime, not just the Go routine.

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
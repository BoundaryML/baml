# Abort Handlers Implementation Plan

## Implementation Status
| Phase | Status | Date | Notes |
|-------|--------|------|-------|
| Phase 1: Rust Core Infrastructure | ✅ Complete | 2025-01-16 | Using stream-cancel crate |
| Phase 2: Go Language Support | ⏳ Pending | - | Ready to implement |
| Phase 3: TypeScript Language Support | ⏳ Pending | - | Ready to implement |
| Phase 4: Python Language Support | ⏳ Pending | - | Ready to implement |

## Overview

Implement comprehensive abort handler support across the BAML runtime, enabling users to cancel in-flight LLM operations via language-idiomatic cancellation patterns (AbortController for TypeScript, custom AbortController for Python, context.Context for Go). The primary integration point is at the orchestrator level to ensure both retries and fallbacks are properly cancelled.

## Current State Analysis

The BAML runtime orchestrators (`call.rs` and `stream.rs`) control retry and fallback chains through simple `for node in iter` loops. Currently, there's no cancellation mechanism - operations run to completion or fail naturally. The `stream-cancel` crate is already integrated, and streaming architecture is designed for cancellation with decoupled stream lifetimes.

### Key Discoveries:
- Orchestrator is the correct integration point (not HTTP or stream processing) - `engine/baml-runtime/src/internal/llm_client/orchestrator/call.rs:45-135`
- Go context cancellation exists but doesn't propagate to Rust - `engine/language_client_go/pkg/callbacks.go:147` 
- TypeScript uses NAPI threadsafe functions for callbacks - `engine/language_client_typescript/src/runtime.rs:425-513`
- Python uses PyO3 with async runtime integration - `engine/language_client_python/src/runtime.rs:177-285`
- Integration tests follow consistent patterns across languages - `integ-tests/*/tests/`

## What We're NOT Doing

- Not implementing Ruby support (deprioritized)
- Not adding HTTP-level cancellation as primary mechanism (only as secondary cleanup)
- Not using Python's native task.cancel() (BAML doesn't create tasks)
- Not modifying git config or pushing to remote
- Not creating new documentation files unless explicitly requested

## Implementation Approach

A phased approach implementing shared Rust infrastructure first, then language-specific bridges in separate phases. Each phase is independently testable with integration tests.

---

## Phase 1: Rust Core Infrastructure & Orchestrator Integration ✅ COMPLETE

### Overview
Add cancellation token support to the Rust runtime orchestrators and create the foundation for language bridges.

**Status**: Completed on 2025-01-16
**Implementation Notes**: 
- Used existing `stream-cancel` crate instead of tokio-util CancellationToken
- Leveraged `Tripwire` type from stream-cancel for cancellation
- All match statements updated to handle new `Cancelled` variant

### Changes Required:

#### 1. Add Required Dependencies
**File**: `engine/baml-runtime/Cargo.toml`
**Changes**: Add futures dependency for `futures::future::pending()`

```toml
[dependencies]
# ... existing dependencies ...
futures = "0.3"  # For futures::future::pending()
tokio-util = { version = "0.7", features = ["sync"] }  # For CancellationToken
```

#### 2. Add LLMResponse Variant for Cancellation
**File**: `engine/baml-runtime/src/internal/llm_client/mod.rs`
**Changes**: Add cancelled variant to LLMResponse enum (around line 130)

```rust
pub enum LLMResponse {
    Success(LLMCompleteResponse),
    LLMFailure(LLMErrorResponse),
    UserFailure(String),
    InternalFailure(String),
    Cancelled(String), // NEW: Cancellation with optional reason
}
```

#### 3. Update Synchronous Orchestrator
**File**: `engine/baml-runtime/src/internal/llm_client/orchestrator/call.rs`
**Changes**: Add cancellation token parameter and checks (lines 45-135)

```rust
pub async fn orchestrate(
    iter: OrchestratorNodeIterator,
    ir: &IntermediateRepr,
    ctx: &RuntimeContext,
    prompt: &PromptRenderer,
    params: &BamlValue,
    parse_fn: impl Fn(&str) -> Result<ResponseBamlValue>,
    cancel_token: Option<Arc<tokio_util::sync::CancellationToken>>, // NEW parameter
) -> (Vec<(OrchestrationScope, LLMResponse, Option<Result<ResponseBamlValue>>)>, Duration) {
    let mut results = Vec::new();
    let mut total_sleep_duration = std::time::Duration::from_secs(0);

    // Create a future that either waits for cancellation or never completes
    let cancel_future = match &cancel_token {
        Some(token) => token.cancelled_owned(),
        None => futures::future::pending().boxed(), // Never completes
    };
    tokio::pin!(cancel_future);

    for node in iter {
        // Check for cancellation at the start of each iteration
        tokio::select! {
            biased; // Check cancellation first for immediate response
            
            _ = &mut cancel_future => {
                results.push((
                    node.scope.clone(),
                    LLMResponse::Cancelled("Operation cancelled".to_string()),
                    None,
                ));
                break;
            }
            result = async {
                // Original loop body unchanged
                let prompt = match node.render_prompt(ir, prompt, ctx, params).await {
                    Ok(p) => p,
                    Err(e) => {
                        return Some((
                            node.scope,
                            LLMResponse::InternalFailure(e.to_string()),
                            Some(Err(anyhow::anyhow!(e.to_string()))),
                        ));
                    }
                };

                let ctx = CtxWithHttpRequestId::from(ctx);
                let response = node.single_call(&ctx, &prompt).await;
                
                // ... existing response parsing logic (lines 78-117) ...
                
                let sleep_duration = node.error_sleep_duration().cloned();
                let result = (node.scope, response, parsed_response);
                
                // Return None to signal success and break
                if matches!(result.1, LLMResponse::Success(_)) {
                    return Some(result); // Will break after pushing
                }
                
                // Sleep if needed
                if let Some(duration) = sleep_duration {
                    total_sleep_duration += duration;
                    async_std::task::sleep(duration).await;
                }
                
                Some(result)
            } => {
                if let Some(result) = result {
                    results.push(result);
                    // Check if we should break
                    if results.last().is_some_and(|(_, r, _)| matches!(r, LLMResponse::Success(_))) {
                        break;
                    }
                }
            }
        }
    }

    (results, total_sleep_duration)
}
```

#### 4. Update Streaming Orchestrator
**File**: `engine/baml-runtime/src/internal/llm_client/orchestrator/stream.rs`
**Changes**: Add cancellation token parameter and checks (lines 27-196)

```rust
pub async fn orchestrate_stream<F, G>(
    iter: OrchestratorNodeIterator,
    ir: &IntermediateRepr,
    ctx: &RuntimeContext,
    prompt: &PromptRenderer,
    params: &BamlValue,
    on_tick: Option<baml_runtime::on_log_event::LogEventCallbackSync>,
    parse_streaming_fn: F,
    parse_final_fn: G,
    on_event: Option<baml_runtime::FunctionResult>,
    cancel_token: Option<Arc<tokio_util::sync::CancellationToken>>, // NEW parameter
) -> (Vec<(OrchestrationScope, LLMResponse, Option<Result<ResponseBamlValue>>)>, Duration)
where
    F: Fn(&str) -> Result<ResponseBamlValue>,
    G: Fn(&str) -> Result<ResponseBamlValue>,
{
    let mut results = Vec::new();
    let mut total_sleep_duration = std::time::Duration::from_secs(0);

    // Create a future that either waits for cancellation or never completes
    let cancel_future = match &cancel_token {
        Some(token) => token.cancelled_owned(),
        None => futures::future::pending().boxed(), // Never completes
    };
    tokio::pin!(cancel_future);

    for node in iter {
        // Check for cancellation at the start of each iteration
        tokio::select! {
            biased; // Check cancellation first for immediate response
            
            _ = &mut cancel_future => {
                results.push((
                    node.scope.clone(),
                    LLMResponse::Cancelled("Operation cancelled".to_string()),
                    None,
                ));
                break;
            }
            result = async {
                // Original loop body unchanged
                let prompt = match node.render_prompt(ir, prompt, ctx, params).await {
                    Ok(p) => p,
                    Err(e) => {
                        return Some((
                            node.scope,
                            LLMResponse::InternalFailure(e.to_string()),
                            Some(Err(anyhow::anyhow!(e.to_string()))),
                        ));
                    }
                };

                let ctx = CtxWithHttpRequestId::from(ctx);
                let stream_res = node.stream(&ctx, &prompt).await;
                
                // Process stream (lines 70-94 in original)
                let final_response = match stream_res {
                    Ok(response) => {
                        // ... existing stream processing with fold ...
                        response
                            .take_while(|_| /* existing logic */)
                            .map(|stream_part| /* existing logic */)
                            .fold(None, |_, current| Some(current))
                            .await
                    },
                    Err(response) => response,
                };
                
                // ... existing result handling and parsing ...
                let sleep_duration = node.error_sleep_duration().cloned();
                let result = (node.scope, final_response, parsed_response);
                
                // Return to signal completion
                if matches!(result.1, LLMResponse::Success(_)) {
                    return Some(result); // Will break after pushing
                }
                
                // Sleep if needed
                if let Some(duration) = sleep_duration {
                    total_sleep_duration += duration;
                    async_std::task::sleep(duration).await;
                }
                
                Some(result)
            } => {
                if let Some(result) = result {
                    results.push(result);
                    // Check if we should break
                    if results.last().is_some_and(|(_, r, _)| matches!(r, LLMResponse::Success(_))) {
                        break;
                    }
                }
            }
        }
    }

    (results, total_sleep_duration)
}
```

#### 5. Thread Cancellation Token Through Runtime Methods
**File**: `engine/baml-runtime/src/runtime_methods/call_function.rs`
**Changes**: Add optional cancel_token parameter (lines 46-69)

```rust
pub async fn call_function_impl(
    baml_runtime: &BamlRuntime,
    function_name: &str,
    params: &BamlValue,
    ctx: &RuntimeContext,
    cancel_token: Option<Arc<tokio_util::sync::CancellationToken>>, // NEW
) -> (Result<FunctionResult>, FunctionCallId) {
    // ... existing logic ...
    
    let (history, sleep_duration) = match provider {
        CallableProvider::LLM(llm_chat) => {
            orchestrate_call(
                local_orchestrator,
                ir,
                &rctx,
                &renderer,
                params,
                |content| renderer.parse(ir, &rctx, content, false),
                cancel_token, // Pass through
            )
            .await
        }
        // ... other providers ...
    };
}
```

**File**: `engine/baml-runtime/src/runtime_methods/stream_function.rs`
**Changes**: Add optional cancel_token parameter (lines 44-106)

### Success Criteria:

#### Automated Verification:
- [x] Rust compilation succeeds: `make -C engine/baml-runtime check` ✅
- [x] Core runtime tests pass: `make -C engine/baml-runtime test` ✅
- [x] No clippy warnings: `make -C engine/baml-runtime lint` ✅

**Actual Implementation Details**:
- Used `stream_cancel::Tripwire` instead of `tokio_util::sync::CancellationToken`
- No additional dependencies needed (stream-cancel was already present)
- Added `Cancelled(String)` variant to `LLMResponse` enum
- Updated all match statements across 8 files to handle the new variant
- Cancellation checks added at orchestrator level (both sync and streaming)
- Currently passing `None` for cancellation tokens - ready for language bridges

---

## Phase 2: Go Language Support

### Overview
Implement context cancellation propagation from Go to Rust via CFFI bridge.

### Changes Required:

#### 1. Add Cancellation Handler to CFFI
**File**: `engine/language_client_cffi/src/ffi/functions.rs`
**Changes**: Add new cancellation function and task tracking

```rust
use dashmap::DashMap;
use tokio_util::sync::CancellationToken;
use once_cell::sync::Lazy;

// Track active operations
static OPERATION_TOKENS: Lazy<DashMap<u32, Arc<CancellationToken>>> = 
    Lazy::new(|| DashMap::new());

#[no_mangle]
pub extern "C" fn cancel_function_call(id: u32) -> *const libc::c_void {
    if let Some((_, token)) = OPERATION_TOKENS.remove(&id) {
        token.cancel();
    }
    std::ptr::null()
}

// Modify existing call functions to store tokens
#[no_mangle]
pub unsafe extern "C" fn call_function_from_c_inner(
    // ... existing params ...
    id: u32,
) -> *const libc::c_void {
    let token = Arc::new(CancellationToken::new());
    OPERATION_TOKENS.insert(id, token.clone());
    
    RUNTIME.spawn(async move {
        // ... existing logic ...
        let result = runtime.call_function(
            // ... params ...
            Some(token), // Pass cancellation token
        ).await;
        
        // Clean up token
        OPERATION_TOKENS.remove(&id);
        
        // ... existing callback logic ...
    });
}
```

#### 2. Propagate Cancellation from Go
**File**: `engine/language_client_go/pkg/callbacks.go`
**Changes**: Call Rust cancellation function (line 147)

```go
// In trigger_callback function
select {
case <-callback.ctx.Done():
    force_close = true
    callback.channel <- ResultCallback{Error: callback.ctx.Err()}
    // Tell Rust to cancel this operation
    baml_go.CancelFunctionCall(id_uint) // NEW: Replace TODO
    break
case callback.channel <- res:
    break
}
```

#### 3. Export Cancellation Function
**File**: `engine/language_client_go/go_client/baml_go.go`
**Changes**: Add export for cancellation function

```go
// #include "../cffi_defs.h"
import "C"

func CancelFunctionCall(id uint32) {
    C.cancel_function_call(C.uint32_t(id))
}
```

#### 4. Fix Generated Streaming Code
**File**: `engine/generators/languages/go/src/_templates/function.stream.go.j2`
**Changes**: Pass user context to CallFunctionStream instead of creating new background context (lines 39-48)

```go
// Change line 39 from:
internal_ctx := context.Background()
// To:
// Remove internal_ctx entirely and use the user's ctx directly

// Change line 40 from:
internal_channel, err := bamlRuntime.CallFunctionStream(internal_ctx, "{{ fn.name }}", encoded, callOpts.onTick)
// To:
internal_channel, err := bamlRuntime.CallFunctionStream(ctx, "{{ fn.name }}", encoded, callOpts.onTick)

// Remove lines 47-49 (the defer with internal_ctx.Done() which doesn't make sense)
defer func() {
    internal_ctx.Done()
}()
// This entire defer block should be removed
```

#### 5. Integration Tests for Go
**File**: `integ-tests/go/test_abort_handlers_test.go` (NEW)
**Changes**: Add comprehensive abort handler tests

```go
package main

import (
    "context"
    "testing"
    "time"
    b "github.com/your/baml_client"
    "github.com/stretchr/testify/assert"
    "github.com/stretchr/testify/require"
)

func TestAbortHandlerCancellation(t *testing.T) {
    t.Run("ManualCancellation", func(t *testing.T) {
        ctx, cancel := context.WithCancel(context.Background())
        
        go func() {
            time.Sleep(100 * time.Millisecond)
            cancel()
        }()
        
        _, err := b.TestRetryExponential(ctx)
        assert.Error(t, err)
        assert.Contains(t, err.Error(), "context canceled")
    })
    
    t.Run("StreamingCancellation", func(t *testing.T) {
        ctx, cancel := context.WithCancel(context.Background())
        
        stream, err := b.Stream.TestFallbackClient(ctx)
        require.NoError(t, err)
        
        go func() {
            time.Sleep(50 * time.Millisecond)
            cancel()
        }()
        
        count := 0
        for range stream {
            count++
        }
        
        // Should have stopped early
        assert.Less(t, count, 10)
    })
    
    t.Run("RetryAbort", func(t *testing.T) {
        ctx, cancel := context.WithTimeout(context.Background(), 200*time.Millisecond)
        defer cancel()
        
        _, err := b.TestRetryConstant(ctx)
        assert.Error(t, err)
        assert.Contains(t, err.Error(), "deadline exceeded")
    })
}
```

#### 6. BAML Test Configuration
**File**: `integ-tests/baml_src/test-files/abort-handlers/abort-handlers.baml` (NEW)
**Changes**: Define test functions

```baml
function TestRetryConstant() -> string {
  client RetryClientConstant
  prompt #"This should fail and retry with constant delay"#
}

function TestRetryExponential() -> string {
  client RetryClientExponential
  prompt #"This should fail and retry with exponential backoff"#  
}

function TestFallbackClient() -> string {
  client FallbackClient
  prompt #"Test fallback chain cancellation"#
}
```

### Success Criteria:

#### Automated Verification:
- [ ] Go client compiles: `make -C engine/language_client_go check`
- [ ] CFFI builds successfully: `make -C engine/language_client_cffi check`
- [ ] Integration tests pass: `make -C integ-tests/go test`
- [ ] New abort handler tests pass: `go test ./integ-tests/go -run TestAbortHandler`

#### Manual Verification:
- [ ] Context cancellation propagates to Rust runtime
- [ ] Streaming operations stop when context is cancelled
- [ ] Retry loops are interrupted on cancellation
- [ ] No goroutine or memory leaks after cancellation

---

## Phase 3: TypeScript Language Support

### Overview
Bridge native JavaScript AbortController to Rust CancellationToken via NAPI.

### Changes Required:

#### 1. Add AbortController Bridge
**File**: `engine/language_client_typescript/src/abort_controller.rs` (NEW)
**Changes**: Create NAPI bridge for AbortController

```rust
use napi::*;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

pub fn js_abort_signal_to_rust_token(
    env: Env,
    signal: Option<JsObject>,
) -> napi::Result<Option<Arc<CancellationToken>>> {
    let Some(signal) = signal else {
        return Ok(None);
    };
    
    let token = Arc::new(CancellationToken::new());
    
    // Check if already aborted
    let aborted: bool = signal.get_named_property("aborted")?;
    if aborted {
        token.cancel();
        return Ok(Some(token));
    }
    
    // Listen to 'abort' event
    let token_clone = token.clone();
    let callback = env.create_function_from_closure("abort_handler", move |_| {
        token_clone.cancel();
        Ok(())
    })?;
    
    // signal.addEventListener('abort', callback)
    let add_event_listener: JsFunction = signal.get_named_property("addEventListener")?;
    add_event_listener.call(Some(&signal), &[
        env.create_string("abort")?.into_unknown(),
        callback.into_unknown(),
    ])?;
    
    Ok(Some(token))
}
```

#### 2. Update Runtime Bridge
**File**: `engine/language_client_typescript/src/runtime.rs`
**Changes**: Accept AbortSignal in function calls (around line 198)

```rust
#[napi]
impl BamlRuntime {
    #[napi]
    pub fn call_function(
        &self,
        env: Env,
        function_name: String,
        args: JsObject,
        ctx: &RuntimeContextManager,
        tb: Option<&TypeBuilder>,
        cb: Option<&ClientRegistry>,
        signal: Option<JsObject>, // NEW: AbortSignal parameter
    ) -> napi::Result<JsObject> {
        let cancel_token = js_abort_signal_to_rust_token(env, signal)?;
        
        let future = async move {
            let result = baml_runtime.call_function(
                // ... params ...
                cancel_token,
            ).await;
            // ... existing logic ...
        };
        
        // ... existing promise creation ...
    }
    
    #[napi]
    pub fn stream_function(
        &self,
        env: Env,
        function_name: String,
        args: JsObject,
        ctx: &RuntimeContextManager,
        tb: Option<&TypeBuilder>,
        cb: Option<&ClientRegistry>,
        signal: Option<JsObject>, // NEW: AbortSignal parameter
    ) -> napi::Result<FunctionResultStream> {
        // Similar implementation
    }
}
```

#### 3. Update TypeScript Stream Class
**File**: `engine/language_client_typescript/typescript_src/stream.ts`
**Changes**: Accept AbortController in constructor (around line 17)

```typescript
export class BamlStream<PartialOutputType, FinalOutputType> {
  private ffiStream: FunctionResultStream;
  private controller?: AbortController;
  
  constructor(
    ffiStream: FunctionResultStream,
    controller?: AbortController,
  ) {
    this.ffiStream = ffiStream;
    this.controller = controller;
    
    // Listen for abort to clean up
    if (controller?.signal) {
      controller.signal.addEventListener('abort', () => {
        this.eventQueue.push({ type: 'error', error: new BamlAbortError('Stream aborted') });
      });
    }
  }
}
```

#### 4. Create Error Types
**File**: `engine/language_client_typescript/typescript_src/errors.ts`
**Changes**: Add BamlAbortError class

```typescript
export class BamlAbortError extends BamlError {
  public readonly reason?: any;
  
  constructor(message: string, reason?: any) {
    super(message);
    this.name = 'BamlAbortError';
    this.reason = reason;
  }
}
```

#### 5. Update Generated Client Code
**File**: `engine/generators/languages/typescript/src/_templates/async_client.ts.j2`
**Changes**: Accept abort controller in options

```typescript
interface BamlOptions {
  abortController?: AbortController;
  // ... other options
}

async {{ func.name }}(
  {{- arg_list }}
  options?: BamlOptions,
): Promise<{{ func.return_type }}> {
  const signal = options?.abortController?.signal;
  
  if (signal?.aborted) {
    throw new BamlAbortError('Operation was aborted', signal.reason);
  }
  
  return this.runtime.callFunction(
    "{{ func.name }}",
    args,
    ctx,
    tb,
    cb,
    signal,
  );
}
```

#### 6. Integration Tests for TypeScript
**File**: `integ-tests/typescript/tests/abort-handlers.test.ts` (NEW)
**Changes**: Add comprehensive abort handler tests

```typescript
import { describe, it, expect } from 'vitest';
import { b, BamlAbortError } from '../baml_client';

describe('Abort Handlers', () => {
  it('manual cancellation', async () => {
    const controller = new AbortController();
    
    const promise = b.TestRetryExponential({
      abortController: controller,
    });
    
    setTimeout(() => controller.abort(), 100);
    
    await expect(promise).rejects.toThrow(BamlAbortError);
  });
  
  it('streaming cancellation', async () => {
    const controller = new AbortController();
    
    const stream = b.stream.TestFallbackClient('test', {
      abortController: controller,
    });
    
    setTimeout(() => controller.abort(), 50);
    
    const values = [];
    try {
      for await (const value of stream) {
        values.push(value);
      }
    } catch (e) {
      expect(e).toBeInstanceOf(BamlAbortError);
    }
    
    expect(values.length).toBeLessThan(10);
  });
  
  it('timeout using AbortSignal.timeout', async () => {
    const promise = b.TestRetryConstant({
      abortController: { signal: AbortSignal.timeout(200) },
    });
    
    await expect(promise).rejects.toThrow();
  });
});
```

### Success Criteria:

#### Automated Verification:
- [ ] TypeScript client builds: `make -C engine/language_client_typescript check`
- [ ] Type checking passes: `npm run typecheck --prefix integ-tests/typescript`
- [ ] Integration tests pass: `npm test --prefix integ-tests/typescript`
- [ ] New abort tests pass: `npm test --prefix integ-tests/typescript -- abort-handlers`

#### Manual Verification:
- [ ] Native AbortController works seamlessly
- [ ] AbortSignal.timeout() works correctly
- [ ] Streaming stops on abort
- [ ] Error types are properly propagated

---

## Phase 4: Python Language Support

### Overview
Create custom AbortController class in Rust exposed via PyO3 for Python.

### Changes Required:

#### 1. Create AbortController PyClass
**File**: `engine/language_client_python/src/types/abort_controller.rs` (NEW)
**Changes**: Define AbortController for Python

```rust
use pyo3::prelude::*;
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};
use tokio_util::sync::CancellationToken;

#[pyclass(module = "baml_py.baml_py")]
pub struct AbortController {
    token: Arc<CancellationToken>,
    aborted: AtomicBool,
}

#[pymethods]
impl AbortController {
    #[new]
    fn new() -> Self {
        Self {
            token: Arc::new(CancellationToken::new()),
            aborted: AtomicBool::new(false),
        }
    }
    
    fn abort(&self) -> PyResult<()> {
        self.aborted.store(true, Ordering::Relaxed);
        self.token.cancel();
        Ok(())
    }
    
    #[getter]
    fn aborted(&self) -> bool {
        self.aborted.load(Ordering::Relaxed)
    }
}

impl AbortController {
    pub fn token(&self) -> Arc<CancellationToken> {
        self.token.clone()
    }
}
```

#### 2. Register AbortController with Module
**File**: `engine/language_client_python/src/lib.rs`
**Changes**: Add AbortController to module (around line 100)

```rust
#[pymodule]
fn baml_py(m: &Bound<'_, PyModule>) -> PyResult<()> {
    // ... existing registrations ...
    m.add_class::<types::abort_controller::AbortController>()?;
    Ok(())
}
```

#### 3. Update Runtime Methods
**File**: `engine/language_client_python/src/runtime.rs`
**Changes**: Accept AbortController in function calls (around line 177)

```rust
#[pymethods]
impl BamlRuntime {
    #[pyo3(signature = (function_name, args, ctx, tb, cb, collectors, env_vars, abort_controller=None))]
    fn call_function(
        &self,
        py: Python<'_>,
        function_name: String,
        args: PyObject,
        ctx: &RuntimeContextManager,
        tb: Option<&TypeBuilder>,
        cb: Option<&ClientRegistry>,
        collectors: &Bound<'_, PyList>,
        env_vars: HashMap<String, String>,
        abort_controller: Option<&AbortController>, // NEW
    ) -> PyResult<PyObject> {
        let cancel_token = abort_controller.map(|ac| ac.token());
        
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let future = baml_runtime.call_function(
                // ... params ...
                cancel_token,
            );
            
            if let Some(token) = cancel_token {
                tokio::select! {
                    result = future => result,
                    _ = token.cancelled() => {
                        Err(anyhow::anyhow!("Operation was aborted"))
                    }
                }
            } else {
                future.await
            }
            .map(FunctionResult::from)
            .map_err(BamlError::from_anyhow)
        })
        .map(pyo3::Bound::into)
    }
    
    // Similar for stream_function and sync versions
}
```

#### 4. Update Python Stream Wrapper
**File**: `engine/language_client_python/python_src/baml_py/stream.py`
**Changes**: Handle abort controller (around line 19)

```python
from typing import Optional
from baml_py import AbortController

class BamlStream(Generic[PartialOutputType, FinalOutputType]):
    def __init__(
        self,
        ffi_stream: FunctionResultStream,
        ctx_manager: BamlCtxManager,
        abort_controller: Optional[AbortController] = None,
    ):
        self.__ffi_stream = ffi_stream
        self.__ctx_manager = ctx_manager
        self.__abort_controller = abort_controller
        
        # ... existing initialization ...
```

#### 5. Update Generated Client Code
**File**: `engine/generators/languages/python/src/_templates/async_client.py.j2`
**Changes**: Accept abort_controller in baml_options

```python
async def {{ func.name }}(
    self,
    {{- arg_list -}}
    baml_options: Optional[BamlCallOptions] = None,
) -> {{ func.return_type }}:
    abort_controller = (
        baml_options.get("abort_controller") 
        if baml_options 
        else None
    )
    
    if abort_controller and abort_controller.aborted:
        raise BamlAbortError("Operation was aborted")
    
    return await self.runtime.call_function(
        "{{ func.name }}",
        args,
        ctx,
        tb,
        cb,
        collectors,
        env_vars,
        abort_controller,
    )
```

#### 6. Integration Tests for Python
**File**: `integ-tests/python/tests/test_abort_handlers.py` (NEW)
**Changes**: Add comprehensive abort handler tests

```python
import pytest
import asyncio
import time
from baml_client import b
from baml_py import AbortController, BamlAbortError

@pytest.mark.asyncio
async def test_manual_cancellation():
    abort_controller = AbortController()
    
    async def abort_after_delay():
        await asyncio.sleep(0.1)
        abort_controller.abort()
    
    task = asyncio.create_task(
        b.TestRetryExponential(
            baml_options={"abort_controller": abort_controller}
        )
    )
    asyncio.create_task(abort_after_delay())
    
    with pytest.raises(BamlAbortError):
        await task

@pytest.mark.asyncio  
async def test_streaming_cancellation():
    abort_controller = AbortController()
    
    stream = b.stream.TestFallbackClient(
        "test",
        baml_options={"abort_controller": abort_controller}
    )
    
    async def abort_after_delay():
        await asyncio.sleep(0.05)
        abort_controller.abort()
    
    asyncio.create_task(abort_after_delay())
    
    values = []
    try:
        async for value in stream:
            values.append(value)
    except BamlAbortError:
        pass
    
    assert len(values) < 10

def test_sync_cancellation():
    from baml_client.sync_client import b as sync_b
    abort_controller = AbortController()
    
    def abort_after_delay():
        time.sleep(0.1)
        abort_controller.abort()
    
    import threading
    threading.Thread(target=abort_after_delay).start()
    
    with pytest.raises(BamlAbortError):
        sync_b.TestRetryConstant(
            baml_options={"abort_controller": abort_controller}
        )
```

### Success Criteria:

#### Automated Verification:
- [ ] Python client builds: `make -C engine/language_client_python check`
- [ ] Type checking passes: `make -C integ-tests/python typecheck`
- [ ] Integration tests pass: `make -C integ-tests/python test`
- [ ] New abort tests pass: `pytest integ-tests/python/tests/test_abort_handlers.py`

#### Manual Verification:
- [ ] AbortController works in both sync and async contexts
- [ ] Streaming operations stop on abort
- [ ] Thread safety is maintained
- [ ] Memory is properly cleaned up

---

## Testing Strategy

### Unit Tests:
- Test CancellationToken propagation in orchestrators
- Test abort signal conversion in each language bridge
- Test error type creation and propagation
- Test cleanup after cancellation

### Integration Tests:
- Test manual cancellation during function calls
- Test timeout-based cancellation
- Test cancellation during retries (verify partial retries)
- Test cancellation during fallback chains
- Test streaming cancellation (partial results)
- Test concurrent cancellations
- Test cleanup and resource management

### Manual Testing Steps:
1. Start a long-running LLM call with many retries
2. Cancel it mid-execution and verify immediate termination
3. Check logs to ensure no further provider attempts after cancellation
4. Verify memory and resource cleanup
5. Test rapid successive cancellations
6. Test cancellation with real LLM providers (not just test clients)

## Performance Considerations

- CancellationToken checks are lightweight (atomic bool check)
- Token storage uses DashMap for concurrent access without global locks
- Cleanup happens automatically when operations complete
- No polling loops - event-driven cancellation propagation

## Migration Notes

This is a new feature with no breaking changes to existing code. Users can opt-in by providing abort controllers/contexts. Existing code without abort controllers continues to work unchanged.

## References

- Original research: `thoughts/shared/research/2025-01-16_13-11-09_abort_handlers_architecture.md`
- Orchestrator implementation: `engine/baml-runtime/src/internal/llm_client/orchestrator/call.rs:45-135`
- Go callback TODO: `engine/language_client_go/pkg/callbacks.go:147`
- TypeScript NAPI bridge: `engine/language_client_typescript/src/runtime.rs:425-513`
- Python PyO3 bridge: `engine/language_client_python/src/runtime.rs:177-285`
# Collector Design

## Overview

The Collector is a user-facing object that captures trace events from BAML function calls. Users pass it via `baml_options={"collector": collector}` and later inspect `.logs`, `.usage`, etc.

The new Collector builds on top of the existing `bex_events::event_store` track/untrack/events_for_span infrastructure.

## Architecture

```
Python user code
  |
  v
baml_py.Collector  (PyO3 shim in bridge_python)
  |
  v
bridge_cffi::Collector  (language-agnostic shim)
  |
  v
bex_events::Collector   (core Rust implementation)
  |
  v
bex_events::event_store  (track / untrack / events_for_span)
```

The core Rust `Collector` lives in `bex_events` alongside the event store it wraps. The CFFI and PyO3 layers are thin shims.

## Core Rust: `bex_events::Collector`

```rust
// bex_events/src/collector.rs

pub struct Collector {
    name: String,
    /// Root span IDs this collector is tracking, in insertion order.
    tracked_roots: Mutex<IndexSet<SpanId>>,
}

impl Collector {
    pub fn new(name: String) -> Self { ... }

    /// Start tracking a root span. Called by the engine when a function
    /// is invoked with this collector attached.
    pub fn track(&self, root_span_id: &SpanId) {
        self.tracked_roots.lock().unwrap().insert(root_span_id.clone());
        event_store::track(root_span_id);  // increment ref count
    }

    /// Get all function logs (one per tracked root span), in insertion order.
    pub fn logs(&self) -> Vec<FunctionLog> {
        let roots = self.tracked_roots.lock().unwrap();
        roots.iter().filter_map(|root| {
            let events = event_store::events_for_span(root)?;
            Some(FunctionLog::from_events(root.clone(), &events))
        }).collect()
    }

    /// Get the most recent function log.
    pub fn last(&self) -> Option<FunctionLog> {
        let roots = self.tracked_roots.lock().unwrap();
        let last_root = roots.last()?;
        let events = event_store::events_for_span(last_root)?;
        Some(FunctionLog::from_events(last_root.clone(), &events))
    }

    /// Aggregate usage across all tracked calls.
    pub fn usage(&self) -> Usage {
        self.logs().iter().fold(Usage::default(), |acc, log| acc + &log.usage)
    }

    /// Clear all tracked logs and release event store references.
    pub fn clear(&self) -> usize {
        let mut roots = self.tracked_roots.lock().unwrap();
        let count = roots.len();
        for root in roots.drain(..) {
            event_store::untrack(&root);
        }
        count
    }

    /// Look up a specific log by its root span ID string.
    pub fn id(&self, span_id_str: &str) -> Option<FunctionLog> { ... }
}

impl Drop for Collector {
    fn drop(&mut self) {
        self.clear();
    }
}
```

## View Types: `FunctionLog`, `LLMCall`, etc.

These are read-only views materialized from `Vec<RuntimeEvent>`. They live in `bex_events/src/collector.rs` (or a submodule).

```rust
pub struct FunctionLog {
    pub id: SpanId,
    pub function_name: String,
    pub log_type: LogType,  // "call" or "stream"
    pub timing: Timing,
    pub usage: Usage,
    pub calls: Vec<LLMCall>,
    pub tags: HashMap<String, String>,
    pub args: Vec<BexExternalValue>,
    pub result: Option<BexExternalValue>,
}

pub enum LogType { Call, Stream }

pub struct Timing {
    pub start_time_utc_ms: i64,
    pub duration_ms: Option<i64>,
}

pub struct Usage {
    pub input_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
    pub cached_input_tokens: Option<i64>,
}

pub struct LLMCall {
    pub function_name: String,
    pub provider: Option<String>,
    pub timing: Timing,
    pub usage: Usage,
    // HTTP details deferred to later milestone
}
```

`FunctionLog::from_events(root, events)` walks the event list:
- Root `FunctionStart` -> function_name, args, tags, start time
- Root `FunctionEnd` -> result, duration
- Child `FunctionStart`/`FunctionEnd` pairs -> `LLMCall` entries
- `SetTags` -> merged into tags map

## Bridge CFFI: `bridge_cffi::Collector`

Thin wrapper in `bridge_cffi/src/collector.rs`:

```rust
pub struct Collector {
    inner: bex_events::Collector,
}
```

Delegates all methods. This layer exists so `bridge_python` and future `bridge_typescript` share the same abstraction.

## Bridge Python: PyO3 Shim

`bridge_python/src/types/collector.rs`:

```rust
#[pyclass]
pub struct Collector {
    inner: bridge_cffi::Collector,
}

#[pymethods]
impl Collector {
    #[new]
    fn new(name: String) -> Self { ... }

    #[getter]
    fn logs(&self, py: Python) -> PyResult<Vec<FunctionLog>> { ... }

    #[getter]
    fn last(&self, py: Python) -> PyResult<Option<FunctionLog>> { ... }

    #[getter]
    fn usage(&self) -> Usage { ... }

    fn clear(&self) -> usize { ... }

    fn id(&self, function_log_id: String) -> Option<FunctionLog> { ... }
}
```

`FunctionLog`, `LLMCall`, `Timing`, `Usage` are also `#[pyclass]` with `#[getter]` properties.

Register in `bridge_python/src/lib.rs`:
```rust
m.add_class::<types::Collector>()?;
m.add_class::<types::collector::FunctionLog>()?;
// etc.
```

## Integration: Passing Collector to call_function

The collector needs to be wired into `call_function`. Two options:

**Option A: Pass collector(s) alongside host_ctx**

Add to `BexEngine::call_function`:
```rust
pub async fn call_function(
    &self,
    function_name: &str,
    args: Vec<BexExternalValue>,
    host_ctx: Option<HostSpanContext>,
    collectors: &[&bex_events::Collector],  // NEW
) -> Result<BexExternalValue, EngineError>
```

Inside, before the event loop:
```rust
for collector in collectors {
    collector.track(&effective_root_span_id);
}
```

The engine doesn't untrack — the Collector's `Drop` or `clear()` does that.

**Option B: Track externally before calling**

The bridge layer calls `collector.track(root_span_id)` before `call_function`, which requires the root span ID to be known before the call. This is awkward since the engine generates it.

**Recommendation: Option A.** It's cleaner — the engine knows the root span ID and wires up tracking atomically.

## Python API Changes

In `bridge_python/src/runtime.rs`, update `call_function` / `call_function_sync` signatures:

```rust
#[pyo3(signature = (function_name, args, ctx=None, collectors=None))]
fn call_function(
    &self,
    py: Python<'_>,
    function_name: String,
    args: PyObject,
    ctx: Option<&HostSpanManager>,
    collectors: Option<Vec<PyRef<Collector>>>,
) -> PyResult<...>
```

The Python-side `baml_client` (generated code) resolves `baml_options={"collector": ...}` into the collectors list and passes it through.

## Files to Create / Modify

### New Files
| File | Contents |
|------|----------|
| `bex_events/src/collector.rs` | Core `Collector`, `FunctionLog`, `LLMCall`, `Timing`, `Usage`, `from_events()` |
| `bridge_cffi/src/collector.rs` | CFFI `Collector` shim |
| `bridge_python/src/types/collector.rs` | PyO3 `Collector`, `FunctionLog`, `LLMCall`, `Timing`, `Usage` classes |

### Modified Files
| File | Change |
|------|--------|
| `bex_events/src/lib.rs` | `pub mod collector;` + re-exports |
| `bex_engine/src/lib.rs` | Add `collectors: &[&Collector]` param to `call_function`, wire up tracking |
| `bridge_cffi/src/lib.rs` | `pub mod collector;` + re-export |
| `bridge_python/src/types/mod.rs` | `pub mod collector;` |
| `bridge_python/src/runtime.rs` | Accept collectors param, extract inner refs, pass to engine |
| `bridge_python/src/lib.rs` | Register new pyclass types |
| `bridge_python/python_src/baml_py/__init__.py` | Export `Collector`, `FunctionLog`, etc. |

## Test Plan

All tests go in `bridge_python/tests/test_collector.py`. They use the same mock infrastructure as `test_tracing.py` (compile BAML source inline, create engine, mock LLM HTTP server).

### Category 1: Basic Collection

| # | Test | Description |
|---|------|-------------|
| 1 | `test_collector_basic_sync` | Sync call with collector, verify `.logs` has 1 entry, `.last` matches |
| 2 | `test_collector_basic_async` | Async call with collector, same verification |
| 3 | `test_collector_captures_function_name` | `log.function_name` matches the called BAML function |
| 4 | `test_collector_captures_args` | `log.args` contains the arguments passed |
| 5 | `test_collector_captures_result` | `log.result` contains the return value |
| 6 | `test_collector_captures_timing` | `log.timing.start_time_utc_ms` > 0, `duration_ms` > 0 |
| 7 | `test_collector_empty_before_call` | Fresh collector has empty `.logs`, `.last` is None |

### Category 2: Multiple Calls

| # | Test | Description |
|---|------|-------------|
| 8 | `test_collector_multiple_sequential_calls` | Two sequential calls, `.logs` has 2 entries in order |
| 9 | `test_collector_parallel_async_calls` | `asyncio.gather` with same collector, both calls tracked |
| 10 | `test_collector_mixed_sync_async` | One sync + one async call on same collector |

### Category 3: Multiple Collectors

| # | Test | Description |
|---|------|-------------|
| 11 | `test_multiple_collectors_same_call` | Pass `[coll1, coll2]`, both see the same log |
| 12 | `test_collectors_independent` | Two collectors track different calls, each sees only its own |
| 13 | `test_collector_list_single_element` | `[collector]` works same as `collector` |

### Category 4: LLM Call Details

| # | Test | Description |
|---|------|-------------|
| 14 | `test_collector_llm_call_captured` | Call an LLM function (mocked), verify `log.calls` has entries |
| 15 | `test_collector_llm_call_function_name` | `call.function_name` matches the LLM function |
| 16 | `test_collector_llm_call_timing` | LLM call has its own timing separate from parent |
| 17 | `test_collector_nested_llm_calls` | Pipeline function calling 2 LLM functions -> 2 entries in `.calls` |

### Category 5: Usage Aggregation

| # | Test | Description |
|---|------|-------------|
| 18 | `test_collector_usage_single_call` | `collector.usage` reflects single call's token counts |
| 19 | `test_collector_usage_aggregated` | Multiple calls, usage sums across all |
| 20 | `test_collector_usage_zero_when_empty` | Empty collector has zeroed usage |

### Category 6: Tags

| # | Test | Description |
|---|------|-------------|
| 21 | `test_collector_tags_from_trace` | Tags set via `@trace` / `upsert_tags` appear in `log.tags` |
| 22 | `test_collector_tags_from_options` | Tags passed via `baml_options` appear in `log.tags` |

### Category 7: GC and Cleanup

| # | Test | Description |
|---|------|-------------|
| 23 | `test_collector_clear` | After `.clear()`, `.logs` is empty, `.last` is None |
| 24 | `test_collector_clear_releases_events` | After `.clear()`, `events_for_span` returns None (memory freed) |
| 25 | `test_collector_drop_releases_events` | Delete collector + gc.collect(), event store memory freed |
| 26 | `test_collector_drop_with_multiple_refs` | Two collectors track same root; dropping one doesn't free (ref count) |
| 27 | `test_collector_clear_then_reuse` | After `.clear()`, collector can track new calls |
| 28 | `test_collector_gc_cycle` | Create collector in loop, verify no memory leak via event store size |

### Category 8: Error Cases

| # | Test | Description |
|---|------|-------------|
| 29 | `test_collector_on_failed_call` | Function raises error; collector still has the log (partial trace) |
| 30 | `test_collector_on_nonexistent_function` | Call to missing function; collector may or may not have a log (TBD) |

### Category 9: Cross-boundary Tracing

| # | Test | Description |
|---|------|-------------|
| 31 | `test_collector_with_trace_decorator` | Collector + `@trace` together; collector captures engine spans nested under host spans |
| 32 | `test_collector_without_trace_decorator` | Collector works without `@trace` (no host context) |

## Implementation Order

1. **Core types** (`bex_events/src/collector.rs`): `Collector`, `FunctionLog`, `from_events()`, `Timing`, `Usage`, `LLMCall`
2. **Rust tests** for `from_events` logic
3. **Engine integration**: add collectors param to `call_function`
4. **CFFI shim** (`bridge_cffi/src/collector.rs`)
5. **PyO3 shim** (`bridge_python/src/types/collector.rs`) + register in module
6. **Python tests** (categories 1-3 first, then 4-9)

## Open Questions

- **Streaming**: `LLMStreamCall` deferred to a future PR? The current engine doesn't have streaming yet.
- **HTTP details**: `HTTPRequest`/`HTTPResponse` on `LLMCall` — depends on whether the engine exposes raw HTTP data through events. May need to add `EventKind::LLMRequest`/`LLMResponse` variants. Defer to later.
- **`log.id()`**: Return string representation of SpanId UUID? Or keep as SpanId?

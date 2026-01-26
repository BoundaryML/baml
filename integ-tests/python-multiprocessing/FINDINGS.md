# Fork Safety Investigation Findings

## Problem Summary

BAML async calls were crashing with SIGSEGV (signal 11) when executed in forked child processes, specifically in RQ (Redis Queue) worker contexts.

## Root Cause Analysis

### Issue 1: Tokio Runtime Corruption After Fork

The original issue was that `pyo3_async_runtimes::tokio::future_into_py` uses a global tokio runtime that becomes corrupted after `fork()`. Unix fork only copies the main thread, leaving the tokio worker threads non-existent in the child process.

**Solution**: Implemented `fork_safe_future_into_py` in `engine/language_client_python/src/lib.rs` that:
1. Uses BAML's fork-safe tokio singleton instead of pyo3-async-runtimes' global runtime
2. Detects fork via PID comparison
3. Creates a fresh tokio runtime in forked children

### Issue 2: DashMap Not Fork-Safe

After fixing the tokio runtime issue, crashes persisted. Investigation revealed the crash was occurring when accessing `self.clients` (a `DashMap<String, CachedClient>`) in forked child processes.

**Why DashMap crashes after fork:**
- DashMap uses internal shards with RwLocks
- When parent process creates/accesses a DashMap, the lock state is copied to child
- The threads that might hold locks don't exist in the child
- Accessing the DashMap in the child can trigger undefined behavior (SIGSEGV)

**Crash location traced via debug logging:**
```
[get_llm_provider_impl] Got clients reference
<CRASH - signal 11>
```

The crash was happening in `clients.contains_key()`.

### Issue 3: Fork Detection Reset Too Early

Initial fix attempt used `is_forked_child()` which compares `RUNTIME_INIT_PID` to current PID. However, `RUNTIME_INIT_PID` gets updated when the tokio runtime is reinitialized in `get_tokio_singleton()`, causing subsequent `is_forked_child()` calls to return `false`.

**Solution**: Added a new flag `IS_FORKED_PROCESS` that gets set when fork is first detected and is NEVER reset. This allows the system to always know if the current process was ever detected as forked.

## Implementation

### Key Changes

1. **Added `IS_FORKED_PROCESS` flag** (`engine/baml-runtime/src/lib.rs`):
```rust
static IS_FORKED_PROCESS: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);
```

2. **Set flag when fork detected** (in `get_tokio_singleton()`):
```rust
if previous_pid != 0 && previous_pid != current_pid {
    mark_as_forked();
}
```

3. **Skip DashMap cache in forked children** (in `get_llm_provider_impl()`):
```rust
if was_forked() {
    // Create fresh client without using DashMap cache
    let walker = self.ir().find_client(client_name)?;
    let new_client = LLMProvider::try_from((&walker, ctx)).map(Arc::new)?;
    return Ok(new_client);
}
```

### Files Modified

- `engine/baml-runtime/src/lib.rs`:
  - Added `IS_FORKED_PROCESS` atomic bool
  - Added `was_forked()` and `mark_as_forked()` functions
  - Modified `get_tokio_singleton()` to detect fork and call `mark_as_forked()`
  - Modified `get_llm_provider_impl()` to skip DashMap cache when `was_forked()` is true

- `engine/language_client_python/src/lib.rs`:
  - Implemented `fork_safe_future_into_py()` for fork-safe async Python integration

- `engine/language_client_python/src/runtime.rs`:
  - Updated to use `fork_safe_future_into_py()` instead of `pyo3_async_runtimes::tokio::future_into_py`

## Other DashMaps That May Need Similar Treatment

The following DashMaps in `BamlRuntime` may also need fork-safety considerations if accessed in forked children:

- `retry_policies: DashMap<String, CallablePolicy>`
- `tracers: DashMap<String, Arc<BamlTracer>>`

Currently only `clients` has been confirmed to cause crashes. The others should be monitored.

## Testing

### Test Environment
- macOS (Darwin)
- Python 3.12 with RQ (Redis Queue)
- OBJC_DISABLE_INITIALIZE_FORK_SAFETY=YES (required for macOS)

### Test Routine

**Step 1: Start RQ Worker**
```bash
cd /path/to/baml/integ-tests/python-multiprocessing
pkill -f "rqworker" 2>/dev/null; sleep 1
PYTHONPATH=. OBJC_DISABLE_INITIALIZE_FORK_SAFETY=YES uv run python -m app.manage rqworker async
```

**Step 2: Run Test (separate terminal)**
```bash
cd /path/to/baml/integ-tests/python-multiprocessing
PYTHONPATH=. OBJC_DISABLE_INITIALIZE_FORK_SAFETY=YES uv run python -c "
from app.manage import test_async_worker
test_async_worker()
"
```

### Results

After implementing the fix, all tests pass consistently:
- Fork detection triggers correctly: `[get_tokio_singleton] Fork detected!`
- DashMap cache is skipped: `[get_llm_provider_impl] Forked child detected, skipping cache...`
- Jobs complete without SIGSEGV

The only error is the expected `ANTHROPIC_API_KEY not set` (no real API calls made in tests).

## Debug Logging Added

Extensive `eprintln!` debug logging was added throughout the call chain to pinpoint crash locations. These should be removed before merging to production.

## Recommendations

1. **Remove debug logging** before production deployment
2. **Consider similar treatment for `retry_policies` DashMap** if retry policies are used in forked workers
3. **Document fork safety requirements** for users of BAML in multi-processing contexts
4. **Consider making all DashMaps fork-safe by default** or providing a runtime option

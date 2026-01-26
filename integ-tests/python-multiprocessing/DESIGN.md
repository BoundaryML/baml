# BAML Runtime Fork-Safety Design Document

## Design Goal

**Users should never have to think about BAML when using multiprocessing, RQ workers, Celery, Gunicorn, or any forking mechanism.** BAML should "just work" - import it anywhere, use it anywhere, and it handles fork-safety transparently.

```python
# This should just work - no special configuration needed
from baml_client import b

def my_rq_job():
    # BAML automatically handles fork-safety
    return b.MyFunction(arg="value")
```

---

## Problem Summary

RQ workers crash with "work-horse terminated unexpectedly" (waitpid returned 11 = SIGSEGV) when executing BAML async calls. This is caused by fork-safety issues with the BAML runtime's tokio async runtime and global state.

### Symptoms
- Worker process crashes immediately when calling any BAML function
- `waitpid` returns signal 11 (SIGSEGV)
- No useful error message or stack trace
- Works fine in main process, fails only in forked workers

---

## Root Cause Analysis

### How Unix Fork Works

When a process calls `fork()`:
1. Child process gets a **copy** of parent's memory (including all static variables)
2. **Only the calling thread** is duplicated - other threads don't exist in child
3. Mutexes, channels, and thread handles become **invalid/corrupted**

### Fork-Unsafe Components in BAML

| Component | File | Line | Issue |
|-----------|------|------|-------|
| Tokio Runtime Singleton | `engine/baml-runtime/src/lib.rs` | 121 | Multi-threaded runtime with worker threads that don't exist after fork |
| Publishing Channel | `engine/baml-runtime/src/tracingv2/publisher/publisher.rs` | 62 | `mpsc::Sender` connected to dead receiver in parent |
| Publishing Task | `engine/baml-runtime/src/tracingv2/publisher/publisher.rs` | 64 | `JoinHandle` to task running in parent process |
| Blob Uploader Task | `engine/baml-runtime/src/tracingv2/publisher/publisher.rs` | 65-66 | Same as publishing task |

### The Crash Sequence

```
1. Parent process initializes BAML
   └── Creates tokio runtime with N worker threads
   └── Spawns publisher task on runtime
   └── Stores in static OnceCell/OnceLock

2. RQ/Celery/Gunicorn forks worker process
   └── Child inherits parent's memory
   └── Static variables point to parent's runtime
   └── Worker threads DON'T EXIST in child

3. Worker calls BAML function
   └── get_tokio_singleton() returns parent's runtime
   └── runtime.block_on() or runtime.spawn() called
   └── Tokio tries to use non-existent worker threads
   └── SIGSEGV (signal 11)
```

---

## Solution Architecture

### Core Principle: PID-Based Fork Detection

Every time BAML accesses the tokio runtime or publisher, it checks:
- "Is my current PID the same as when I was initialized?"
- If NO → We're in a forked child → Create fresh state

This is **completely transparent to users** - no configuration, no special imports, no `reset()` calls needed.

### Architecture Diagram

```
┌─────────────────────────────────────────────────────────────────┐
│                     PARENT PROCESS (PID 1234)                    │
│                                                                  │
│  ┌──────────────────┐    ┌────────────────────────────────────┐ │
│  │ RUNTIME_INIT_PID │    │ TOKIO RUNTIME                      │ │
│  │      = 1234      │    │  ├── worker thread 1               │ │
│  └──────────────────┘    │  ├── worker thread 2               │ │
│                          │  └── worker thread N               │ │
│  ┌──────────────────┐    └────────────────────────────────────┘ │
│  │PUBLISHER_INIT_PID│    ┌────────────────────────────────────┐ │
│  │      = 1234      │    │ PUBLISHER                          │ │
│  └──────────────────┘    │  ├── channel: tx ←→ rx (task)      │ │
│                          │  └── blob uploader task            │ │
│                          └────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────────┘
                                    │
                                    │ fork()
                                    ▼
┌─────────────────────────────────────────────────────────────────┐
│                     CHILD PROCESS (PID 5678)                     │
│                                                                  │
│  ┌──────────────────┐    ┌────────────────────────────────────┐ │
│  │ RUNTIME_INIT_PID │    │ OLD TOKIO RUNTIME (CORRUPTED!)     │ │
│  │      = 1234      │    │  └── thread handles point nowhere  │ │
│  └────────┬─────────┘    └────────────────────────────────────┘ │
│           │                                                      │
│           │  get_tokio_singleton() called                        │
│           │                                                      │
│           ▼                                                      │
│  ┌──────────────────────────────────────────────────────────┐   │
│  │ CHECK: current_pid (5678) != init_pid (1234)             │   │
│  │        → FORK DETECTED!                                   │   │
│  │                                                           │   │
│  │ ACTION: Create NEW tokio runtime for this process        │   │
│  │         Skip publisher (can't reinitialize OnceCell)     │   │
│  └──────────────────────────────────────────────────────────┘   │
│                                                                  │
│  ┌────────────────────────────────────────────────────────────┐ │
│  │ NEW TOKIO RUNTIME (FRESH!)                                 │ │
│  │  ├── new worker thread 1                                   │ │
│  │  ├── new worker thread 2                                   │ │
│  │  └── new worker thread N                                   │ │
│  └────────────────────────────────────────────────────────────┘ │
│                                                                  │
│  User's BAML call executes successfully!                         │
└─────────────────────────────────────────────────────────────────┘
```

---

## Implementation Details

### Files Modified

#### 1. `engine/baml-runtime/src/lib.rs`

**Purpose**: Fork-safe tokio runtime management

```rust
// New imports
use std::{
    cell::RefCell,
    sync::atomic::{AtomicU32, Ordering},
};

// Track PID when runtime was first created
static RUNTIME_INIT_PID: AtomicU32 = AtomicU32::new(0);

// Thread-local cache with PID for fork detection
thread_local! {
    static RUNTIME_CACHE: RefCell<Option<(u32, Arc<tokio::runtime::Runtime>)>> =
        const { RefCell::new(None) };
}

/// Check if we're in a forked child process
pub fn is_forked_child() -> bool {
    let init_pid = RUNTIME_INIT_PID.load(Ordering::Relaxed);
    init_pid != 0 && init_pid != std::process::id()
}

/// Reset global state after fork (called automatically, but exposed for manual use)
pub fn reset_after_fork() {
    RUNTIME_CACHE.with(|cache| {
        *cache.borrow_mut() = None;
    });
    tracingv2::publisher::reset_publisher_after_fork();
}

// Method to get fork-safe runtime for each call
fn get_runtime(&self) -> Result<Arc<tokio::runtime::Runtime>> {
    if is_forked_child() {
        // We're in a forked child - use the singleton which creates fresh runtime
        Self::get_tokio_singleton()
    } else {
        // Normal case - use the stored runtime
        Ok(self.async_runtime.clone())
    }
}

// Modified: get_tokio_singleton() now creates fresh runtime after fork
pub(crate) fn get_tokio_singleton() -> Result<Arc<tokio::runtime::Runtime>> {
    let current_pid = std::process::id();

    // Check thread-local cache (includes fork detection via PID)
    let cached = RUNTIME_CACHE.with(|cache| {
        if let Some((pid, rt)) = cache.borrow().as_ref() {
            if *pid == current_pid {
                return Some(rt.clone());
            }
        }
        None
    });

    if let Some(rt) = cached {
        return Ok(rt);
    }

    // Create new runtime (first time OR after fork)
    let rt = Arc::new(tokio::runtime::Runtime::new()?);

    RUNTIME_INIT_PID.store(current_pid, Ordering::Relaxed);
    RUNTIME_CACHE.with(|cache| {
        *cache.borrow_mut() = Some((current_pid, rt.clone()));
    });

    Ok(rt)
}

// Modified: call_function_sync uses get_runtime() instead of self.async_runtime
pub fn call_function_sync(...) -> (Result<FunctionResult>, FunctionCallId) {
    // Get fork-safe runtime
    let rt = match self.get_runtime() {
        Ok(rt) => rt,
        Err(e) => return (Err(e), FunctionCallId::new()),
    };
    rt.block_on(self.call_function(...))
}
```

**Note**: Similar changes were made to:
- `BamlAsyncVmRuntime::call_function_sync` in `async_vm_runtime.rs`
- `BamlAsyncInterpreterRuntime::call_function_sync` in `async_interpreter_runtime.rs`
- All `tokio::spawn()` calls now use the current runtime context instead of `self.async_runtime.spawn()`

#### 2. `engine/baml-runtime/src/tracingv2/publisher/publisher.rs`

**Purpose**: Prevent use of corrupted publisher channels in forked children

```rust
use std::sync::atomic::{AtomicU32, Ordering};

// Track PID when publisher was initialized
static PUBLISHER_INIT_PID: AtomicU32 = AtomicU32::new(0);

fn is_publisher_in_forked_child() -> bool {
    let init_pid = PUBLISHER_INIT_PID.load(Ordering::Relaxed);
    init_pid != 0 && init_pid != std::process::id()
}

pub fn reset_publisher_after_fork() {
    // OnceCell can't be reset, but PID check prevents usage
    log::debug!("reset_publisher_after_fork called");
}

// Modified: get_publish_channel() returns None for forked children
fn get_publish_channel(allow_missing: bool) -> Option<&'static mpsc::Sender<PublisherMessage>> {
    if is_publisher_in_forked_child() {
        log::debug!("Skipping publish channel in forked child");
        return None;  // Don't use corrupted parent channel
    }
    // ... rest of function
}

// Modified: start_publisher() skips initialization in forked children
pub fn start_publisher(...) {
    let init_pid = PUBLISHER_INIT_PID.load(Ordering::Relaxed);
    if init_pid != 0 && init_pid != std::process::id() {
        log::debug!("Skipping publisher in forked child");
        return;  // Can't reinitialize OnceCell
    }
    // ... rest of function (sets PUBLISHER_INIT_PID on first init)
}
```

#### 3. `engine/baml-runtime/src/tracingv2/publisher/mod.rs`

**Purpose**: Export the reset function

```rust
#[cfg(not(target_arch = "wasm32"))]
pub use publisher::reset_publisher_after_fork;
pub use publisher::{flush, publish_trace_event, start_publisher};
```

#### 4. `engine/language_client_python/src/lib.rs`

**Purpose**: Expose reset_after_fork to Python (for advanced users)

```rust
/// Reset BAML runtime state after a fork.
///
/// This is called automatically when BamlRuntime is unpickled, but can be
/// called manually if needed (e.g., os.register_at_fork).
#[pyfunction]
fn reset_after_fork() -> PyResult<()> {
    baml_runtime::reset_after_fork();
    Ok(())
}

// In baml_py module:
m.add_wrapped(wrap_pyfunction!(reset_after_fork))?;
```

#### 5. `engine/language_client_python/src/runtime.rs`

**Purpose**: Auto-reset when BamlRuntime is unpickled (common in RQ/Celery)

```rust
/// Recreate BamlRuntime from pickle state.
/// Called when deserializing in forked worker - auto-resets runtime.
#[staticmethod]
fn _create_from_state(
    root_path: String,
    env_vars: HashMap<String, String>,
    files: HashMap<String, String>,
) -> PyResult<Self> {
    // KEY: Reset state before creating runtime in forked child
    baml_runtime::reset_after_fork();

    let core = CoreBamlRuntime::from_file_content(&root_path, &files, env_vars.clone())?;
    Ok(BamlRuntime { inner: Arc::new(core), root_path, env_vars, files })
}
```

#### 6. `engine/language_client_python/python_src/baml_py/__init__.py`

**Purpose**: Export reset_after_fork for Python users

```python
from .baml_py import (
    # ... existing exports ...
    reset_after_fork,
)

__all__ = [
    # ... existing exports ...
    "reset_after_fork",
]
```

#### 7. `integ-tests/python-multiprocessing/app/manage.py`

**Purpose**: Add test CLI command

```python
if __name__ == "__main__":
    from django.core.management import execute_from_command_line

    if len(sys.argv) > 1 and sys.argv[1] == "test":
        test_async_worker()
    else:
        execute_from_command_line(sys.argv)
```

---

## Testing Instructions

### Prerequisites

```bash
# Ensure Redis is installed
brew install redis  # macOS
# or
apt-get install redis-server  # Ubuntu

# Navigate to test directory
cd /Users/aaronvillalpando/Projects/baml/integ-tests/python-multiprocessing
```

### Build the Rust Extension

```bash
# Rebuild after any Rust changes
uv run maturin develop --uv \
  --manifest-path /Users/aaronvillalpando/Projects/baml/engine/language_client_python/Cargo.toml
```

### Run Tests

**Terminal 1: Start Redis**
```bash
redis-server
```

**Terminal 2: Start RQ Worker**
```bash
cd /Users/aaronvillalpando/Projects/baml/integ-tests/python-multiprocessing

# With debug output (recommended for verification)
PYTHONPATH=. RUST_BACKTRACE=1 RUST_LOG=debug \
  uv run python app/manage.py rqworker async

# Or without debug output
PYTHONPATH=. uv run python app/manage.py rqworker async
```

**Terminal 3: Submit Test Job**
```bash
cd /Users/aaronvillalpando/Projects/baml/integ-tests/python-multiprocessing
PYTHONPATH=. uv run python app/manage.py test
```

### Expected Output

**Before Fix:**
```
Worker (PID 12345) started
work-horse terminated unexpectedly; waitpid returned 11 (signal 11)
```

**After Fix:**
```
Worker (PID 12345) started
Job abc123: _async_worker_job()
Creating new tokio runtime for PID 12345 (previous init PID: 12340)
Skipping publisher initialization in forked child
Result: Hello2
```

---

## Verification Checklist

| Test Case | Expected | Status |
|-----------|----------|--------|
| Single RQ job executes without crash | Job returns result | ⬜ |
| Multiple sequential jobs work | All jobs complete | ⬜ |
| Worker stays alive after job | Worker waits for next job | ⬜ |
| Sync BAML calls work in worker | Result returned | ⬜ |
| Async BAML calls work in worker | Result returned | ⬜ |
| Streaming BAML calls work in worker | Stream completes | ⬜ |
| Celery worker (if applicable) | Jobs execute | ⬜ |
| Gunicorn prefork workers | Requests handled | ⬜ |
| Python multiprocessing.Pool | Tasks complete | ⬜ |

---

## Known Limitations (Phase 1)

### 1. Tracing Disabled in Forked Children

**Why**: `OnceCell` cannot be reset after initialization. The publisher channels and tasks are stored in `OnceCell` statics.

**Impact**: BAML function calls work, but traces won't appear in Boundary Studio for forked workers.

**Workaround**: None in Phase 1. See Phase 2 roadmap.

### 2. First Call Slightly Slower in Worker

**Why**: Each forked worker creates a new tokio runtime on first BAML call.

**Impact**: ~10-50ms overhead on first call in each worker.

**Mitigation**: Runtime is cached for subsequent calls.

### 3. Async Client Not Supported in Forked Workers

**Why**: The Python async client uses `pyo3_async_runtimes::tokio::future_into_py` which maintains its own internal tokio runtime via a global `OnceLock`. This runtime becomes corrupted after fork, and pyo3-async-runtimes provides no way to reset it.

**Impact**: Only the **sync client** (`from baml_client.sync_client import b`) works in forked workers. The async client will crash.

**Workaround**: Use the sync client in forked workers:
```python
# In RQ/Celery jobs, use sync client:
from baml_client.sync_client import b

def my_rq_job():
    return b.MyFunction(arg="value")  # Sync call - works!
```

### 4. macOS-Specific: Objective-C Runtime Fork Safety

**Why**: On macOS, system frameworks (used by HTTP/TLS libraries) initialize Objective-C classes lazily. If a fork occurs during class initialization, macOS's runtime detects this and aborts the child process.

**Symptoms**:
```
objc[PID]: +[NSMutableString initialize] may have been in progress in another
thread when fork() was called. We cannot safely call it or ignore it in the
fork() child process. Crashing instead.
```

**Workaround**: Set the environment variable before starting workers:
```bash
export OBJC_DISABLE_INITIALIZE_FORK_SAFETY=YES

# Then start your worker:
rq worker
# or
celery worker
# or
gunicorn --preload
```

**Note**: This is a macOS-specific issue. Linux workers should not encounter this problem.

### 5. Pre-imported Modules

**Why**: If `baml_client` is imported in the parent process before fork, the `BamlRuntime` object is already created with a reference to the parent's tokio runtime. The `get_runtime()` fix handles this by checking the PID on every runtime access, but some edge cases may still exist.

**Mitigation**: The sync client's `call_function_sync` method calls `get_runtime()` which detects the fork and creates a fresh runtime.

---

## Phase 2 Roadmap

### 2.1 Enable Tracing in Forked Children

**Approach**: Replace `OnceCell` with resettable storage

```rust
// Current (cannot reset):
static PUBLISHING_CHANNEL: OnceCell<mpsc::Sender<...>> = OnceCell::new();

// Phase 2 (can reset):
static PUBLISHING_STATE: Mutex<Option<PublisherState>> = Mutex::new(None);

pub fn reset_publisher_after_fork() {
    *PUBLISHING_STATE.lock().unwrap() = None;
}
```

**Files to modify**:
- `engine/baml-runtime/src/tracingv2/publisher/publisher.rs`

### 2.2 Automatic Fork Handler (Python)

**Approach**: Use `os.register_at_fork` for automatic reset

```python
# In baml_py/__init__.py
import os

def _post_fork_child():
    from .baml_py import reset_after_fork
    reset_after_fork()

try:
    os.register_at_fork(after_in_child=_post_fork_child)
except AttributeError:
    pass  # Python < 3.7 or non-Unix
```

**Files to modify**:
- `engine/language_client_python/python_src/baml_py/__init__.py`

### 2.3 Automatic Fork Handler (Rust)

**Approach**: Use `pthread_atfork` for process-level hook

```rust
#[cfg(unix)]
pub fn register_fork_handlers() {
    use std::sync::Once;
    static REGISTERED: Once = Once::new();

    REGISTERED.call_once(|| {
        extern "C" fn child_handler() {
            crate::reset_after_fork();
        }
        unsafe {
            libc::pthread_atfork(None, None, Some(child_handler));
        }
    });
}
```

**Files to modify**:
- `engine/baml-runtime/src/lib.rs`

### 2.4 Pre-fork Cleanup (Optional)

**Approach**: Flush traces before fork to avoid data loss

```rust
extern "C" fn prepare_handler() {
    // Flush pending traces before fork
    let _ = crate::tracingv2::publisher::flush();
}

unsafe {
    libc::pthread_atfork(Some(prepare_handler), None, Some(child_handler));
}
```

### 2.5 Async Client Support in Forked Workers

**Problem**: The Python async client uses `pyo3_async_runtimes::tokio::future_into_py` which maintains its own tokio runtime in a global `OnceLock`:

```rust
// From pyo3-async-runtimes/src/tokio.rs:68
static TOKIO_RUNTIME: OnceLock<Pyo3Runtime> = OnceLock::new();

pub fn get_runtime<'a>() -> &'a Runtime {
    TOKIO_RUNTIME.get_or_init(|| {
        // Creates runtime on first access - can't be reset after fork!
    })
}
```

After fork, `TOKIO_RUNTIME` contains the corrupted parent runtime, and `OnceLock` provides no way to reset it.

**Approach Options**:

#### Option A: Custom Async Wrapper (Recommended)

Replace `pyo3_async_runtimes::tokio::future_into_py` with a custom implementation that uses our fork-safe runtime:

```rust
// In engine/language_client_python/src/runtime.rs

/// Fork-safe alternative to pyo3_async_runtimes::tokio::future_into_py
fn fork_safe_future_into_py<F, T>(py: Python, fut: F) -> PyResult<Bound<PyAny>>
where
    F: Future<Output = PyResult<T>> + Send + 'static,
    T: for<'py> IntoPyObject<'py> + Send + 'static,
{
    // Get fork-safe runtime
    let rt = BamlRuntime::get_tokio_singleton()?;

    // Create Python asyncio future
    let asyncio = py.import("asyncio")?;
    let loop_ = asyncio.call_method0("get_running_loop")?;
    let py_future = loop_.call_method0("create_future")?;
    let py_future_ref = py_future.clone().unbind();

    // Spawn on our fork-safe runtime
    rt.spawn(async move {
        let result = fut.await;
        Python::with_gil(|py| {
            let future = py_future_ref.bind(py);
            match result {
                Ok(val) => {
                    let _ = future.call_method1("set_result", (val,));
                }
                Err(e) => {
                    let _ = future.call_method1("set_exception", (e,));
                }
            }
        });
    });

    Ok(py_future)
}
```

**Files to modify**:
- `engine/language_client_python/src/runtime.rs`

#### Option B: Fork pyo3-async-runtimes

Create a modified version of pyo3-async-runtimes that supports runtime reset:

```rust
// Modified pyo3-async-runtimes with reset support
static TOKIO_RUNTIME: Mutex<Option<Pyo3Runtime>> = Mutex::new(None);

pub fn reset_runtime() {
    *TOKIO_RUNTIME.lock().unwrap() = None;
}

pub fn get_runtime<'a>() -> &'a Runtime {
    // ... with Mutex instead of OnceLock
}
```

**Trade-offs**: Requires maintaining a fork, but provides drop-in compatibility.

#### Option C: Block-on Wrapper for Forked Children

Detect fork and use sync execution in forked children:

```rust
fn call_function(py: Python, ...) -> PyResult<Bound<PyAny>> {
    if is_forked_child() {
        // Use sync execution with fork-safe runtime
        let result = get_runtime()?.block_on(async_call(...))?;
        // Wrap in already-resolved asyncio future
        wrap_in_future(py, result)
    } else {
        // Normal async path
        pyo3_async_runtimes::tokio::future_into_py(py, async_call(...))
    }
}
```

**Trade-offs**: Simpler but loses true async behavior in forked workers.

---

## References

### GitHub Issues
- [PyO3 #4215: Multiprocessing Fork Issue](https://github.com/PyO3/pyo3/issues/4215)
- [Tokio #5532: SIGSEGV after fork](https://github.com/tokio-rs/tokio/issues/5532)
- [Gunicorn #2761: Fork SIGSEGV](https://github.com/benoitc/gunicorn/issues/2761)

### Key Learnings
1. **Tokio is explicitly not fork-safe** - documented behavior
2. **PID tracking is industry standard** - used by Python stdlib, many Rust crates
3. **`OnceCell`/`OnceLock` cannot be reset** - by design for thread safety
4. **Pickle deserialization is the common path** - RQ, Celery, multiprocessing all use it

---

## Architecture Decision Records

| Decision | Rationale | Trade-offs |
|----------|-----------|------------|
| PID tracking over `pthread_atfork` | Simpler, no unsafe code, cross-platform | Small overhead per runtime access |
| Thread-local runtime cache | Avoids mutex contention | Memory per thread |
| Auto-reset on unpickle | Covers 90% of fork use cases | May reset when pickle used without fork |
| Disable tracing in forked children (Phase 1) | OnceCell limitation, ship fix faster | Loss of observability |
| Expose `reset_after_fork()` to Python | Power users can call manually | API surface increase |

---

## Summary

**The fix ensures BAML "just works" in forked processes by:**

1. **Detecting forks automatically** via PID comparison
2. **Creating fresh tokio runtimes** in forked children
3. **Bypassing corrupted publisher channels** to prevent crashes
4. **Auto-resetting on pickle deserialize** (common fork pattern)

**Users don't need to:**
- Call any special functions
- Configure any settings
- Import BAML differently
- Worry about fork safety at all

**It just works.**

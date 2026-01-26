# Testing BAML Fork Safety

## Docker Testing (Linux)

This is the recommended way to test fork safety on Linux.

### Prerequisites

- Docker and Docker Compose installed
- No local Redis running on port 6379 (or stop it first: `redis-cli shutdown`)

### Step 1: Build the Docker Image

```bash
cd integ-tests/python-multiprocessing
docker compose build
```

This builds a single-stage image that:
- Installs Rust, uv, and maturin
- Builds the baml-py Rust extension from source
- Installs all Python dependencies

Build time: ~3 minutes (with caching, rebuilds are faster)

### Step 2: Start Services

```bash
docker compose up -d
```

This starts:
- Redis (on port 6379)
- RQ worker listening on the "async" queue

### Step 3: Verify Worker is Running

```bash
docker compose logs rqworker
```

You should see:
```
rqworker-1  | [BAML] SIGSEGV handler installed for debugging, PID=1
rqworker-1  | Worker ...: started with PID 1, version 2.6.1
rqworker-1  | *** Listening on async...
```

### Step 4: Run Test

**Option A: One-liner**
```bash
docker compose exec rqworker python -c "from app.manage import test_async_worker; test_async_worker()"
```

**Option B: Interactive Python shell**
```bash
docker compose exec rqworker python
```

Then in Python:
```python
>>> from app.manage import test_async_worker
>>> test_async_worker()
```

### Expected Output

Success:
```
Test 2: Async in RQ worker...
Job ID: <uuid>
Result: None
```

Failure (SIGSEGV):
```
Test 2: Async in RQ worker...
Job ID: <uuid>
Failed: Work-horse terminated unexpectedly; waitpid returned 11 (signal 11);
```

### Step 5: Run Multiple Tests

```bash
for i in 1 2 3 4 5; do
  echo "Test $i:"
  docker compose exec rqworker python -c "from app.manage import test_async_worker; test_async_worker()" 2>&1 | grep -E "(Result:|Failed:|Job ID:)"
  echo ""
done
```

### Step 6: View Worker Logs

```bash
docker compose logs -f rqworker
```

### Step 7: Cleanup

```bash
docker compose down
```

### Rebuilding After Code Changes

If you modify Rust code in `engine/language_client_python`:
```bash
docker compose down
docker compose build --no-cache
docker compose up -d
```

---

## Local Testing (macOS)

### Prerequisites

1. Redis server running
2. Rust extension built

### Build the Rust Extension

```bash
cd integ-tests/python-multiprocessing
uv run maturin develop --uv --manifest-path ../../engine/language_client_python/Cargo.toml
```

### Start Redis (if not running)

```bash
redis-server
```

Verify:
```bash
redis-cli ping
```

### Test Routine

#### Step 1: Start RQ Worker

```bash
cd integ-tests/python-multiprocessing
pkill -f "rqworker" 2>/dev/null; sleep 1
PYTHONPATH=. OBJC_DISABLE_INITIALIZE_FORK_SAFETY=YES uv run python -m app.manage rqworker async
```

#### Step 2: Run Test (Separate Terminal)

```bash
cd integ-tests/python-multiprocessing
PYTHONPATH=. OBJC_DISABLE_INITIALIZE_FORK_SAFETY=YES uv run python -c "
from app.manage import test_async_worker
test_async_worker()
"
```

### Run Multiple Tests

```bash
for i in 1 2 3 4 5; do
  PYTHONPATH=. OBJC_DISABLE_INITIALIZE_FORK_SAFETY=YES uv run python -c "
from app.manage import test_async_worker
test_async_worker()
" 2>&1 | grep -E "(Result:|Failed:)"
done
```

### Environment Variables

- `OBJC_DISABLE_INITIALIZE_FORK_SAFETY=YES` - Required on macOS for fork safety
- `PYTHONPATH=.` - Required for local module imports
- `RUST_LOG=debug` - Optional: enables Rust debug logging
- `RUST_BACKTRACE=1` - Optional: enables backtraces on crash

### Cleanup

```bash
pkill -f "rqworker"
redis-cli shutdown
```

---

## Troubleshooting

### SIGSEGV in Forked Child

If you see `signal 11` crashes, the issue is likely:
1. DashMap access in forked child (fixed by `was_forked()` check)
2. Tokio runtime corruption (fixed by `fork_safe_future_into_py`)

Check debug output to see which component is failing.

### Job Hangs

If jobs hang, check:
1. Redis is running: `redis-cli ping` (or `docker compose exec redis redis-cli ping`)
2. Worker is listening: Look for `*** Listening on async...`
3. No Python import errors in worker output

### Port 6379 Already in Use

If Docker fails to start with "address already in use":
```bash
# Stop local Redis
redis-cli shutdown

# Then start Docker services
docker compose up -d
```

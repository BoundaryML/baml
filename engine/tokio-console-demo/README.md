# Tokio Console Demo

This is a demonstration application showing how to use `tokio-console` to debug async Rust applications, specifically featuring an intentional deadlock scenario.

## What This Demo Shows

- Basic Axum web server with multiple endpoints
- Integration with `console-subscriber` for tokio-console monitoring  
- Intentional deadlock scenario for debugging practice
- Various async patterns (spawning tasks, delays, shared state)

## Prerequisites

1. Install `tokio-console` CLI tool:
   ```bash
   cargo install --locked tokio-console
   ```

## Running the Demo

1. **Start the server** (from the `engine/tokio-console-demo` directory):
   ```bash
   cargo run
   ```

2. **In another terminal, start tokio-console**:
   ```bash
   tokio-console
   ```

3. **Test the endpoints** (in a third terminal):
   ```bash
   # Basic hello world
   curl http://127.0.0.1:3000/
   
   # Counter operations
   curl http://127.0.0.1:3000/counter
   curl -X POST http://127.0.0.1:3000/counter/increment
   
   # User operations
   curl http://127.0.0.1:3000/users
   curl -X POST http://127.0.0.1:3000/users/add \
     -H "Content-Type: application/json" \
     -d '{"name":"Alice","age":30}'
   
   # Slow endpoint (simulates async work)
   curl "http://127.0.0.1:3000/slow?delay=3000"
   
   # 🚨 DEADLOCK TRIGGER (use with caution!)
   curl http://127.0.0.1:3000/deadlock
   ```

## Using Tokio Console to Debug the Deadlock

### Step 1: Trigger the Deadlock
```bash
curl http://127.0.0.1:3000/deadlock
```

This endpoint will hang indefinitely because it creates two tasks that acquire the same locks in different orders.

### Step 2: Observe in Tokio Console

In the tokio-console interface, you'll see:

1. **Tasks Tab**: Shows the two spawned tasks that are blocked
   - Look for tasks with high "Busy" time but not completing
   - Status will show as "Running" but they're actually deadlocked

2. **Resources Tab**: Shows the mutexes and their contention
   - You can see which tasks are waiting on which resources

3. **Async Ops Tab**: Shows pending async operations

### What to Look For

- **Blocked Tasks**: Tasks that have been "running" for an unusually long time
- **Resource Contention**: Multiple tasks waiting on the same resources
- **No Progress**: Tasks that aren't making forward progress despite being active

### Key Tokio Console Features Demonstrated

- **Task Inspection**: See individual task states and lifetimes
- **Resource Monitoring**: Monitor mutexes, channels, and other async primitives  
- **Performance Metrics**: CPU usage, wake counts, and poll counts per task
- **Real-time Updates**: Live view of your async runtime

## Understanding the Deadlock

The deadlock occurs in the `/deadlock` endpoint:

- **Task 1**: Acquires `counter` lock → sleeps → tries to acquire `users` lock
- **Task 2**: Acquires `users` lock → sleeps → tries to acquire `counter` lock

This creates a classic deadlock scenario where each task is waiting for a resource held by the other.

## Cleaning Up

When you trigger the deadlock, you'll need to kill the server process manually:
```bash
Ctrl+C  # or kill the process
```

## Learning Exercise

Try modifying the deadlock scenario to:
1. Use a timeout to break the deadlock
2. Acquire locks in the same order to prevent deadlock  
3. Use async-aware primitives like `tokio::sync::Mutex` instead of `std::sync::Mutex`

## Environment Variables

The demo uses `console_subscriber::init()` which respects these environment variables:

- `TOKIO_CONSOLE_BIND`: Address to bind the console server (default: 127.0.0.1:6669)  
- `TOKIO_CONSOLE_RETENTION`: How long to retain completed tasks (default: 6s)
- `TOKIO_CONSOLE_ENABLE`: Enable/disable console subscriber

Example:
```bash
TOKIO_CONSOLE_RETENTION=10s cargo run
```
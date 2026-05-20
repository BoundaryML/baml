# GC park deadlock in `HeapPermitManager::request_park`

## TL;DR

`HeapPermitManager::request_park` (in
`baml_language/crates/bex_heap/src/heap_guard.rs:294`) takes the `holders`
mutex **before** awaiting `acquire_many(MAX_PERMITS)` on the semaphore.
`new_permit` (line 282) needs that same mutex.

Result: classic AB-BA deadlock when one thread is mid-`spawn` (holds an
active permit, wants the mutex) while another is parking for GC (holds the
mutex, wants every permit). The process stays alive but every BAML thread
that needs a new permit hangs forever.

Introduced in **#3386 "New garbage collector"** (`5d8f2f631`, 2026-04-20).
It's been latent on canary since then — BEP-034's concurrent spawn workload
just makes it reproducible in seconds.

## The buggy code

`baml_language/crates/bex_heap/src/heap_guard.rs:282-306`:

```rust
pub async fn new_permit<T: RootHaver + 'static>(&self, with_roots: T) -> InactiveHeapPermit<T> {
    let mut guard = self.holders.lock().await;
    debug_assert!(guard.len() < MAX_PERMITS as usize);
    let holder = Arc::new(PermitCell::new(with_roots));
    guard.push(Arc::downgrade(&holder) as Weak<PermitCell<dyn RootHaver>>);
    let permit = InactiveHeapPermit {
        active: self.active.clone(),
        holder,
    };
    drop(guard);
    permit
}

pub async fn request_park(&self) -> HeapGuard<'_> {
    let mut guard = self.holders.lock().await;          // 1. takes mutex
    let permits = self
        .active
        .acquire_many(MAX_PERMITS)                       // 2. long await,
        .await                                           //    mutex still held
        .unwrap_or_else(|_| unreachable!("We do not close the semaphore"));
    guard.retain(|holder| holder.strong_count() > 0);
    HeapGuard { guard, _permits: permits }
}
```

The `holders` mutex is held across the `acquire_many` await. Nothing else
can take that mutex until every active permit is released.

## Deadlock scenario

Two BAML threads, A and B, running concurrently:

1. **Thread B** hits a GC safepoint, releases its own permit, calls
   `BexEngine::collect_garbage` (`bex_engine/src/lib.rs:908`) →
   `request_park` → takes the `holders` mutex → awaits
   `acquire_many(MAX_PERMITS)`. B is now waiting for every other thread's
   permit to drop.

2. **Thread A** is in the middle of executing a `spawn` opcode. It holds
   its `ActiveHeapPermit` (its permit is alive across the `.await` at
   `bex_engine/src/lib.rs:1974`). The spawn path:
   `spawn_thread` → `spawn_thread_inner` → `spawn_thread_setup`
   (`bex_engine/src/lib.rs:1608`) → `new_permit(())` at line 1618. A
   blocks on `holders.lock()`.

State:
- A: holds a semaphore permit, waiting for the mutex.
- B: holds the mutex, waiting for A's semaphore permit.

Neither thread can make progress. The accept loop wedges; the process
stays alive but stops servicing requests.

## Why it surfaces now

Pre-BEP-034 there was no workload that ran two BAML threads concurrently
in a tight allocate-and-spawn loop, so the race window never opened. The
deadlock pattern itself is unchanged on this branch — `git diff
canary..feature/bep-034-spawn-await -- baml_language/crates/bex_heap/src/heap_guard.rs`
is empty.

BEP-034 adds a concurrent scheduler that lets each accepted TCP connection
run as its own BAML thread. Under load the GC heuristic
(`should_collect()` in `bex_engine/src/lib.rs:1398, 1422`) fires constantly,
so at any moment some thread is in `request_park` while another is
mid-`spawn` — and the AB-BA closes.

## Reproduce — Rust test (preferred)

Drop the following into
`baml_language/crates/bex_heap/tests/heap_guard_deadlock.rs`. It pokes
`HeapPermitManager` directly, no BAML / VM / GC needed, and isolates the
exact AB-BA cycle described above.

`bex_heap`'s dev-dependencies currently only pull `tokio` for `sync`. You
need a tokio runtime + timeouts for the test, so add:

```toml
[dev-dependencies]
tokio = { workspace = true, features = ["macros", "rt-multi-thread", "time"] }
```

Test body:

```rust
//! Regression test for the GC-park / new-permit AB-BA deadlock in
//! `HeapPermitManager`. See `bex_heap/src/heap_guard.rs:294`.

use std::sync::Arc;
use std::time::Duration;

use bex_heap::HeapPermitManager;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn request_park_must_not_block_new_permit() {
    let mgr = Arc::new(HeapPermitManager::new());

    // Thread A: simulate a VM in the middle of executing a `spawn` opcode —
    // it is still holding its active heap permit.
    let inactive_a = mgr.new_permit(()).await;
    let active_a = inactive_a.acquire().await;

    // Thread B: the GC. Calls request_park. With the buggy ordering it
    // takes the `holders` mutex and then awaits `acquire_many` forever
    // (because A still holds a permit). The mutex stays held the whole
    // time.
    let mgr_b = Arc::clone(&mgr);
    let park = tokio::spawn(async move {
        let _guard = mgr_b.request_park().await;
    });

    // Give B time to enter request_park and grab the holders mutex.
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Thread C: another VM hitting a `spawn` opcode — it tries to allocate
    // a new permit. With the buggy ordering this blocks on the holders
    // mutex (held by B) forever. With the fix this returns immediately
    // because B is waiting on the semaphore, not on the mutex.
    let mgr_c = Arc::clone(&mgr);
    let new_permit = tokio::spawn(async move {
        let _ = mgr_c.new_permit(()).await;
    });

    let result = tokio::time::timeout(Duration::from_secs(1), new_permit).await;

    // Drop A's permit so B (and the runtime) can shut down cleanly even on
    // the buggy path — otherwise the test process would leak the park task.
    drop(active_a);
    let _ = park.await;

    assert!(
        result.is_ok(),
        "new_permit deadlocked waiting on the holders mutex while \
         request_park was holding it across acquire_many — see \
         baml_language/crates/bex_heap/src/heap_guard.rs:294"
    );
}
```

Run it with:

```bash
cargo test -p bex_heap --test heap_guard_deadlock -- --nocapture
```

Verified locally on `feature/bep-034-spawn-await`:

- **Buggy `request_park` (current `canary`)**: test FAILS, the 1s timeout
  elapses and the assertion fires.

  ```
  test request_park_must_not_block_new_permit ... FAILED
  thread 'request_park_must_not_block_new_permit' panicked at
  crates/bex_heap/tests/heap_guard_deadlock.rs:50:5:
  new_permit deadlocked waiting on the holders mutex while request_park
  was holding it across acquire_many — see
  baml_language/crates/bex_heap/src/heap_guard.rs:294
  test result: FAILED. 0 passed; 1 failed; … finished in 1.06s
  ```

- **With the fix** (drain semaphore before taking mutex): test PASSES in
  ~60 ms.

  ```
  test request_park_must_not_block_new_permit ... ok
  test result: ok. 1 passed; 0 failed; … finished in 0.06s
  ```

## Reproduce — end-to-end (BAML HTTP server)

This is how the bug was originally hit; useful if you want to see the
realistic accept-loop wedge rather than the isolated synchronization
primitive. Tested on macOS with `ab` from `apache2-utils`
(`brew install httpd`).

### 1. Build a baml binary with BEP-034 spawn

```bash
git checkout feature/bep-034-spawn-await
cargo build --release -p baml_cli
```

### 2. Minimal BAML server

In an empty directory, drop the following at `baml_src/main.baml`. Note
that `baml.sys.sleep(500)` from the standard demo has been **removed** —
the sleep gives the runtime breathing room and hides the bug; without it
the spawn-per-request rate is high enough for the deadlock to close
quickly under load.

```baml
function build_response(body: string) -> string {
    "HTTP/1.1 200 OK\r\n"
        + "Content-Type: application/json\r\n"
        + "Content-Length: " + baml.unstable.string(body.length()) + "\r\n"
        + "Connection: close\r\n"
        + "\r\n"
        + body
}

function handle(sock: baml.net.Socket, conn_id: int) -> null {
    let label = "[conn " + baml.unstable.string(conn_id) + "]";
    baml.io.println(label + " accepted");
    let _raw = sock.read() catch_all (e) {
        _ => { baml.io.println(label + " read failed"); return null; },
    };
    baml.io.println(label + " req");
    let body = "{\"hello\":\"world\",\"conn\":" + baml.unstable.string(conn_id) + "}";
    let _ = sock.write(build_response(body)) catch_all (e) {
        _ => { baml.io.println(label + " write failed"); null },
    };
    let _ = sock.close() catch_all (e) { _ => null };
    baml.io.println(label + " done");
    null
}

// Per-connection helper so each spawn captures its own sock/id by value.
function spawn_handler(sock: baml.net.Socket, id: int) -> baml.future.Future<null, null> {
    spawn { handle(sock, id) }
}

function serve() -> null {
    let listener = baml.net.listen("127.0.0.1:8080");
    baml.io.println("listening on http://127.0.0.1:8080");
    let conn_id = 0;
    while (true) {
        let sock = listener.accept();
        conn_id = conn_id + 1;
        let _ = spawn_handler(sock, conn_id);
    }
    null
}
```

### 3. Run

```bash
# Terminal A — start the server. Redirect stdout so println contention
# doesn't dominate the run.
/path/to/baml run --from . --function user.serve > /dev/null 2>&1 &

# Terminal B
# c=5 is stable — completes cleanly at ~14k req/s.
ab -n 5000 -c 5  -s 30 http://127.0.0.1:8080/

# c=10 reproducibly hangs the listener after a few hundred requests.
ab -n 5000 -c 10 -s 30 http://127.0.0.1:8080/

# Confirm the wedge — the LISTEN socket is still there but accept never
# returns. The baml process is still alive in `ps`; only SIGKILL recovers.
curl -m 3 http://127.0.0.1:8080/
lsof -iTCP:8080 -sTCP:LISTEN
```

### 4. What you should see

- `c=5`: clean ~14k req/s.
- `c=10`: `ab` hangs partway through; subsequent `curl` to `:8080` times
  out; `lsof` shows the listener still bound; `ps` shows baml still
  running, not 100% CPU (it's parked in tokio).
- Attaching a debugger (or `sample` on macOS) shows one task parked in
  `Semaphore::acquire_many` inside `request_park` and many tasks parked
  in `Mutex::lock` inside `new_permit` / `spawn_thread_setup`.

At c=5 there's enough breathing room between GC ticks that any in-flight
spawn finishes (releasing the mutex briefly) before another thread enters
`request_park`. At c≥10 the heuristic fires often enough to close the
AB-BA cycle.

## Fix

Drain the semaphore **before** taking the mutex. The mutex is only needed
to splice the holder list (the `retain` call), which is safe to do *after*
`acquire_many` returns because once all `MAX_PERMITS` are drained no new
permit can become active until the `HeapGuard` is dropped — any holder
that `new_permit` adds during the wait is parked on the semaphore and
harmless to GC.

```rust
pub async fn request_park(&self) -> HeapGuard<'_> {
    let permits = self
        .active
        .acquire_many(MAX_PERMITS)
        .await
        .unwrap_or_else(|_| unreachable!("We do not close the semaphore"));
    let mut guard = self.holders.lock().await;
    guard.retain(|holder| holder.strong_count() > 0);
    HeapGuard { guard, _permits: permits }
}
```

Safety argument for the reordering:

- A holder added by a concurrent `new_permit` *after* we drained the
  semaphore is inactive — it cannot call `acquire()` past the drained
  semaphore until our `HeapGuard` drops.
- The holder's `T` is fully constructed before `new_permit` pushes the
  weak ref (`Arc::new(PermitCell::new(with_roots))` runs first), so if GC
  iterates over it via `collect_roots`/`forward_roots` the read is
  well-defined.
- For the `spawn_thread_setup` `()` holder, `RootHaver` is a no-op.
- For the `BexThread` holder, its roots (the closure pointer) were
  allocated under the parent's still-live permit, so they're already in
  the live set the parent contributes.

## Provenance

```
$ git log --follow --diff-filter=A --oneline -- \
    baml_language/crates/bex_heap/src/heap_guard.rs
5d8f2f631 New garbage collector (#3386)
a45b32d1d GC synchronization

$ git log -1 --format='%h %s %ai %n%an <%ae>' 5d8f2f631
5d8f2f631 New garbage collector (#3386) 2026-04-20 16:44:16 -0700
2kai2kai2 <kai@boundaryml.com>
```

The exact `request_park` body shown at the top has been in the file
unchanged since `5d8f2f631`. Subsequent PRs that touched the file
(`#3405 "GC synchronization cleanup"`, `#3427 "Global futures"`) did not
modify this function.

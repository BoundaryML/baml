test "cancelling a completed success preserves its result" {
    let f = spawn { "finished" };
    assert.equal(await f, "finished");
    assert.equal(f.cancel(), false);
    assert.equal(f.is_result(), true);
    assert.equal(await f, "finished");
}

// A one-slot limit and the task holding that slot: anything else spawned with
// the limit waits until the holder is cancelled.
class Gate {
    limit: baml.spawn.Limit,
    holder: baml.future.Future<null, never>,
}

function gate() -> Gate {
    let limit = baml.spawn.Limit.new(1) catch (e) {
        baml.errors.InvalidArgument => baml.sys.panic("one is positive"),
    };
    let holder = spawn "gate holder" with limit {
        baml.sys.sleep(baml.time.Duration.from_seconds(60n))
    };
    Gate { limit: limit, holder: holder }
}

test "cancelling twice fires the token once" {
    let g = gate();
    defer { g.holder.cancel(); }
    let f = spawn with g.limit { "not started" };
    assert.equal(f.cancel(), true);
    assert.equal(f.cancel(), false);
    // The task settles its own future, so its state is readable once it has.
    let value = (await f) catch (e) {
        baml.panics.Cancelled => "cancelled",
    };
    assert.equal(value, "cancelled");
    assert.equal(f.is_cancelled(), true);
}

function completed_or_failed(value: int) -> int throws LookupFailed {
    if (value < 0) { throw LookupFailed { key: value.to_string() }; }
    value
}

test "all_settled preserves failures and waits for later successes" {
    let g = gate();
    let failed = spawn { completed_or_failed(-1) };
    let later = spawn with g.limit { completed_or_failed(2) };
    let release = spawn {
        baml.sys.sleep(baml.time.Duration.from_milliseconds(100));
        g.holder.cancel()
    };
    defer { g.holder.cancel(); }
    let outcomes = await baml.future.all_settled([failed, later]);
    assert.equal(later.is_settled(), true);
    assert.equal(later.is_cancelled(), false);
    let values = outcomes.map((outcome) -> {
        match (outcome) {
            let success: baml.future.Success<int> => success.value.to_string(),
            let failure: baml.future.Failure<LookupFailed> => failure.error.key,
            let panic: baml.future.Panicked => baml.sys.panic("unexpected panic"),
        }
    });
    assert.equal(values, ["-1", "2"]);
    await release;
}

test "all_settled returns every typed error in input order" {
    let failed = spawn { completed_or_failed(-1) };
    let second = spawn { completed_or_failed(-2) };
    let outcomes = await baml.future.all_settled([failed, second]);
    let errors = outcomes.map((outcome) -> {
        match (outcome) {
            let failure: baml.future.Failure<LookupFailed> => failure.error.key,
            _ => baml.sys.panic("expected failure"),
        }
    });
    assert.equal(errors, ["-1", "-2"]);
    assert.equal(second.is_settled(), true);
}

test "explicit cancellation cascades to attached children but not detached children" {
    let attached: baml.future.Future<int, baml.errors.Io>[] = [];
    let detached: baml.future.Future<int, baml.errors.Io>[] = [];
    let parent = spawn {
        attached.push(spawn { baml.sys.sleep(baml.time.Duration.from_seconds(60n)); 1 });
        detached.push(spawn with baml.spawn.Root.new() {
            baml.sys.sleep(baml.time.Duration.from_seconds(60n)); 2
        });
        baml.sys.sleep(baml.time.Duration.from_seconds(60n));
    };
    while (detached.length() == 0) { baml.sys.sleep(baml.time.Duration.from_milliseconds(1)); }
    defer { detached[0].cancel(); }
    parent.cancel();
    let cancelled = (await attached[0]) catch (e) { baml.panics.Cancelled => -1 };
    assert.equal(cancelled, -1);
    assert.equal(detached[0].is_cancelled(), false);
    assert.equal(detached[0].cancel(), true);
}

test "successful parent completion leaves its child running" {
    let g = gate();
    let parent = spawn {
        spawn with g.limit { 42 }
    };
    let child = await parent;
    assert.equal(child.is_settled(), false);
    g.holder.cancel();
    assert.equal(await child, 42);
}

test "a wrapper runs around the task and passes its value through" {
    let traced = Traced {};
    let f = spawn "traced read" with traced { simulate_read("notes.md", 1) };
    assert.equal(await f, "contents of notes.md");
}

test "a fallback replaces an error with its value" {
    assert.equal(read_or_empty("no/such/file.md"), "");
}

test "a fallback does not catch cancellation" {
    let started: bool[] = [];
    let empty = Fallback { value: "" };
    let f = spawn with empty {
        started.push(true);
        baml.sys.sleep(baml.time.Duration.from_seconds(60n));
        "finished"
    };
    // Cancel once the body is running inside the fallback's wrapper.
    while (started.length() == 0) { baml.sys.sleep(baml.time.Duration.from_milliseconds(1)); }
    f.cancel();
    let value = (await f) catch (e) {
        baml.panics.Cancelled => "cancelled",
    };
    assert.equal(value, "cancelled");
}

test "cancelling a pool's token cancels its running and waiting tasks" {
    let pool = ProviderPool {
        limit: baml.spawn.Limit.new(1),
        token: baml.spawn.CancelToken.new(),
    };
    let running = spawn with pool {
        baml.sys.sleep(baml.time.Duration.from_seconds(60n));
        "finished"
    };
    let waiting = spawn with pool { "not started" };
    assert.equal(pool.limit.queued_count(), 1);
    pool.token.cancel();
    let outcomes = [running, waiting].map((f) -> {
        (await f) catch (e) {
            baml.panics.Cancelled => "cancelled",
        }
    });
    assert.equal(outcomes, ["cancelled", "cancelled"]);
}

test "a retry loop that rethrows cancellation stops when cancelled" {
    let f = spawn { read_when_available("no/such/file.md") };
    baml.sys.sleep(baml.time.Duration.from_milliseconds(120));
    f.cancel();
    let outcome = (await f) catch (e) {
        baml.panics.Cancelled => "cancelled",
    };
    assert.equal(outcome, "cancelled");
}

test "all cancels remaining inputs before rethrowing" {
    let g = gate();
    defer { g.holder.cancel(); }
    let failed = spawn { completed_or_failed(-1) };
    let later = spawn with g.limit { completed_or_failed(2) };
    let message = {
        await baml.future.all([failed, later]);
        "unexpected success"
    } catch (e) { LookupFailed => e.key };
    assert.equal(message, "-1");
    // The task settles its own future, so its state is readable once it has.
    let _ = (await later) catch (e) {
        baml.panics.Cancelled => 0,
        LookupFailed => 0,
    };
    assert.equal(later.is_cancelled(), true);
}

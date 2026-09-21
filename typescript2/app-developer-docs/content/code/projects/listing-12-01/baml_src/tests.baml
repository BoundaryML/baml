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
    holder: baml.future.Future<null, baml.errors.Io>,
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
    assert.equal(later.is_cancelled(), true);
}

test "cancelling a completed success preserves its result" {
    let f = spawn { "finished" };
    assert.equal(await f, "finished");
    assert.equal(f.cancel(), false);
    assert.equal(f.is_result(), true);
    assert.equal(await f, "finished");
}

test "cancelling twice reports an already settled cancellation" {
    let group = baml.spawn.TaskGroup.new(0);
    let f = spawn with baml.spawn.options(group = group) { "not started" };
    assert.equal(f.cancel(), true);
    assert.equal(f.cancel(), false);
    assert.equal(f.is_cancelled(), true);
    let value = (await f) catch (e) {
        baml.panics.Cancelled => "cancelled",
    };
    assert.equal(value, "cancelled");
}

function completed_or_failed(value: int) -> int throws LookupFailed {
    if (value < 0) { throw LookupFailed { key: value.to_string() }; }
    value
}

test "all_complete observes later tasks before rethrowing the first input error" {
    let group = baml.spawn.TaskGroup.new(0);
    let failed = spawn { completed_or_failed(-1) };
    let later = spawn with baml.spawn.options(group = group) { completed_or_failed(2) };
    let release = spawn {
        baml.sys.sleep(baml.time.Duration.from_milliseconds(100));
        group.set_limit(1);
    };
    // Always release the parked input, even if an assertion exposes a regression.
    defer { group.set_limit(1); }
    let result = (await baml.future.all_complete([failed, later])) catch (e) {
        LookupFailed => {
            assert.equal(e.key, "-1");
            assert.equal(later.is_settled(), true);
            assert.equal(later.is_cancelled(), false);
            [await later]
        },
    };
    assert.equal(result, [2]);
    await release;
}

test "all_complete returns values in input order and consumes later errors" {
    assert.equal(await baml.future.all_complete([spawn { 3 }, spawn { 1 }]), [3, 1]);
    let failed = spawn { completed_or_failed(-1) };
    let second = spawn { completed_or_failed(-2) };
    let message = {
        await baml.future.all_complete([failed, second]);
        "unexpected success"
    } catch (e) { LookupFailed => e.key };
    assert.equal(message, "-1");
    assert.equal(second.is_settled(), true);
}

test "explicit cancellation cascades to attached children but not detached children" {
    let attached: baml.future.Future<int, baml.errors.Io>[] = [];
    let detached: baml.future.Future<int, baml.errors.Io>[] = [];
    let parent = spawn {
        attached.push(spawn { baml.sys.sleep(baml.time.Duration.from_seconds(60n)); 1 });
        detached.push(spawn with baml.spawn.options(detach = true) {
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
    let group = baml.spawn.TaskGroup.new(0);
    let parent = spawn {
        spawn with baml.spawn.options(group = group) { 42 }
    };
    let child = await parent;
    assert.equal(child.is_settled(), false);
    group.set_limit(1);
    assert.equal(await child, 42);
}

test "all cancels remaining inputs before rethrowing" {
    let group = baml.spawn.TaskGroup.new(0);
    let failed = spawn { completed_or_failed(-1) };
    let later = spawn with baml.spawn.options(group = group) { completed_or_failed(2) };
    let message = {
        await baml.future.all([failed, later]);
        "unexpected success"
    } catch (e) { LookupFailed => e.key };
    assert.equal(message, "-1");
    assert.equal(later.is_cancelled(), true);
}

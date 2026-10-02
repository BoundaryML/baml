// `baml.SpawnLocalStorage`: values set with `run` are visible to the body and
// to every thread spawned from it, and are restored however the body ends.

function request_id() -> baml.SpawnLocalStorage<string> {
    baml.SpawnLocalStorage.new("request_id", "none")
}

function unset_reads_default() -> string {
    request_id().get()
}

test "unset_reads_default" {
    assert.equal(unset_reads_default(), "none")
}

function run_sets_value_for_body() -> string {
    let inside = request_id().run("req-1", () -> string {
        request_id().get()
    });
    `${inside}|${request_id().get()}`
}

test "run_sets_value_for_body" {
    assert.equal(run_sets_value_for_body(), "req-1|none")
}

// A nested spawn sees the value as of its own spawn, not its grandparent's.
function spawn_inherits_value() -> string {
    request_id().run("req-1", () -> string {
        let child = spawn {
            let grandchild = spawn { request_id().get() };
            let overridden = request_id().run("req-2", () -> string {
                let inner = spawn { request_id().get() };
                await inner
            });
            `${await grandchild}|${overridden}|${request_id().get()}`
        };
        await child
    })
}

test "spawn_inherits_value" {
    assert.equal(spawn_inherits_value(), "req-1|req-2|req-1")
}

// A spawned thread keeps the value it was spawned with after `run` returns.
function spawn_outlives_run() -> string throws baml.errors.Io {
    let child = request_id().run("req-1", () -> baml.future.Future<string, baml.errors.Io> {
        spawn {
            baml.sys.sleep(baml.time.Duration.from_milliseconds(10n));
            request_id().get()
        }
    });
    `${request_id().get()}|${await child}`
}

test "spawn_outlives_run" {
    assert.equal(spawn_outlives_run(), "none|req-1")
}

// A child's changes never reach its parent.
function child_changes_are_isolated() -> string {
    request_id().run("parent", () -> string {
        let child = spawn {
            request_id().enter_with("child");
            request_id().get()
        };
        `${await child}|${request_id().get()}`
    })
}

test "child_changes_are_isolated" {
    assert.equal(child_changes_are_isolated(), "child|parent")
}

function run_restores_after_throw() -> string {
    let thrown = request_id().run("req-1", () -> string throws baml.errors.Io {
        throw baml.errors.Io { message: "boom" }
    }) catch (_) {
        baml.errors.Io => request_id().get(),
    };
    thrown
}

test "run_restores_after_throw" {
    assert.equal(run_restores_after_throw(), "none")
}

function exit_unsets_for_body() -> string {
    request_id().run("req-1", () -> string {
        let exited = request_id().exit(() -> string {
            let child = spawn { request_id().get() };
            `${request_id().get()}|${await child}`
        });
        `${exited}|${request_id().get()}`
    })
}

test "exit_unsets_for_body" {
    assert.equal(exit_unsets_for_body(), "none|none|req-1")
}

// `enter_with` and `disable` last until the enclosing `run` finishes.
function enter_with_and_disable() -> string {
    let inside = request_id().run("req-1", () -> string {
        request_id().enter_with("req-2");
        let entered = request_id().get();
        request_id().disable();
        `${entered}|${request_id().get()}`
    });
    `${inside}|${request_id().get()}`
}

test "enter_with_and_disable" {
    assert.equal(enter_with_and_disable(), "req-2|none|none")
}

// Storages with different names are independent; the same name is shared.
function storages_are_keyed_by_name() -> string {
    let a = baml.SpawnLocalStorage.new("a", 0);
    let b = baml.SpawnLocalStorage.new("b", 0);
    let a_again = baml.SpawnLocalStorage.new("a", -1);
    a.run(1, () -> string {
        b.run(2, () -> string {
            `${a.get()}|${b.get()}|${a_again.get()}`
        })
    })
}

test "storages_are_keyed_by_name" {
    assert.equal(storages_are_keyed_by_name(), "1|2|1")
}

// A value of the wrong type for a storage sharing its name reads as default.
function mismatched_type_reads_default() -> int {
    let text = baml.SpawnLocalStorage.new("shared", "text");
    let number = baml.SpawnLocalStorage.new("shared", 7);
    text.run("hello", () -> int {
        number.get()
    })
}

test "mismatched_type_reads_default" {
    assert.equal(mismatched_type_reads_default(), 7)
}

class RequestInfo {
    id: string,
    tags: string[],
}

// Reference values are shared, not copied, like any other BAML value.
function stores_class_values() -> string {
    let info = baml.SpawnLocalStorage.new("info", RequestInfo { id: "none", tags: [] });
    info.run(RequestInfo { id: "req-1", tags: ["a", "b"] }, () -> string {
        let child = spawn { info.get() };
        let seen = await child;
        `${seen.id}|${seen.tags.length()}`
    })
}

test "stores_class_values" {
    assert.equal(stores_class_values(), "req-1|2")
}

// Many concurrent children each see their own value.
function concurrent_children_see_own_values() -> string throws baml.errors.Io {
    let futures: baml.future.Future<string, baml.errors.Io>[] = [];
    for (let i in [1, 2, 3, 4, 5]) {
        futures.push(
            request_id().run(`req-${i}`, () -> baml.future.Future<string, baml.errors.Io> {
                spawn {
                    baml.sys.sleep(baml.time.Duration.from_milliseconds(5n));
                    request_id().get()
                }
            }),
        );
    }
    let seen: string[] = [];
    for (let f in futures) {
        seen.push(await f);
    }
    seen.join(",")
}

test "concurrent_children_see_own_values" {
    assert.equal(concurrent_children_see_own_values(), "req-1,req-2,req-3,req-4,req-5")
}

// The context survives collection while only a spawned thread still holds it.
function survives_garbage_collection() -> string throws baml.errors.Io {
    let child = request_id().run(`req-${1}`, () -> baml.future.Future<string, baml.errors.Io> {
        spawn {
            baml.sys.sleep(baml.time.Duration.from_milliseconds(5n));
            baml.sys.collect_garbage();
            request_id().get()
        }
    });
    baml.sys.collect_garbage();
    await child
}

test "survives_garbage_collection" {
    assert.equal(survives_garbage_collection(), "req-1")
}

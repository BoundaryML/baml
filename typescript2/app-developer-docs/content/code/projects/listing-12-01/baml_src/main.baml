// docs:start excerpt-44
function read_two_documents_linear(first_path: string, second_path: string) -> string[] {
    let first = baml.fs.read(first_path);
    let second = baml.fs.read(second_path);
    [first, second]
}
// docs:end excerpt-44



// docs:start excerpt-45
function read_one_document_concurrently(first_path: string, second_path: string) -> string[] {
    let first_future = spawn { baml.fs.read(first_path) };
    let second = baml.fs.read(second_path);
    let first = await first_future;
    [first, second]
}
// docs:end excerpt-45



// docs:start excerpt-46
function read_two_documents(first_path: string, second_path: string) -> string[] {
    let first_future = spawn { baml.fs.read(first_path) };
    let second_future = spawn { baml.fs.read(second_path) };
    let first = await first_future;
    let second = await second_future;
    [first, second]
}
// docs:end excerpt-46



// docs:start excerpt-47
function read_two_documents_all(first_path: string, second_path: string) -> string[] {
    let first_future = spawn { baml.fs.read(first_path) };
    let second_future = spawn { baml.fs.read(second_path) };
    await baml.future.all([first_future, second_future])
}
// docs:end excerpt-47



function read_two_documents_named(first_path: string, second_path: string) -> string[] {
    let first_future = spawn `read ${first_path}` { baml.fs.read(first_path) };
    let second_future = spawn `read ${second_path}` { baml.fs.read(second_path) };
    await baml.future.all([first_future, second_future])
}


// docs:start excerpt-48
function simulate_read(path: string, delay_ms: int) -> string {
    baml.sys.sleep(baml.time.Duration.from_milliseconds(delay_ms));
    `contents of ${path}`
}

function main() -> string[] {
    let notes_delay = int.random(100, 800);
    let outline_delay = int.random(100, 800);
// docs:start excerpt-49
    let notes_future = spawn `read notes.md (${notes_delay} ms)` { simulate_read("notes.md", notes_delay) };
// docs:end excerpt-49

    let outline_future = spawn `read outline.md (${outline_delay} ms)` { simulate_read("outline.md", outline_delay) };
    let notes = await notes_future;
    let outline = await outline_future;
    [notes, outline]
}
// docs:end excerpt-48



// docs:start excerpt-55
function read_documents_in_one_thread(first_path: string, second_path: string) -> string[] {
    let documents_future = spawn "read documents" {
        let first = baml.fs.read(first_path);
        let second = baml.fs.read(second_path);
        [first, second]
    };

    // Do other work while the documents are being read.

    await documents_future
}
// docs:end excerpt-55



// docs:start excerpt-50
function read_all_linear(paths: string[]) -> string[] {
    paths.map((path) -> {
        baml.fs.read(path)
    })
}
// docs:end excerpt-50



// docs:start excerpt-51
function read_all(paths: string[]) -> string[] {
    let futures = paths.map((path) -> {
        spawn `read ${path}` { baml.fs.read(path) }
    });
    await baml.future.all(futures)
}
// docs:end excerpt-51



// docs:start excerpt-52
function read_all_limited(paths: string[], limit: int) -> string[] {
    let group = baml.spawn.TaskGroup.new(limit, name = "document reads");
    let futures = paths.map((path) -> {
        spawn `read ${path}` with baml.spawn.options(group = group) { baml.fs.read(path) }
    });
    await baml.future.all(futures)
}
// docs:end excerpt-52



// docs:start excerpt-57
function read_all_with_dynamic_limit(
    paths: string[],
    initial_limit: int,
    new_limit: int,
) -> string[] {
    let group = baml.spawn.TaskGroup.new(initial_limit, name = "document reads");
    let futures = paths.map((path) -> {
        spawn `read ${path}` with baml.spawn.options(group = group) { baml.fs.read(path) }
    });
    group.set_limit(new_limit);
    await baml.future.all(futures)
}
// docs:end excerpt-57



// docs:start excerpt-54
class EmptyPathList {
    message: string,
}

function read_all_typed(paths: string[]) -> string[] throws EmptyPathList | _ {
    if (paths.length() == 0) {
        throw EmptyPathList { message: "Provide at least one path" };
    }
    let futures: baml.future.Future<string, _>[] = paths.map((path) -> {
        spawn `read ${path}` { baml.fs.read(path) }
    });
    await baml.future.all(futures)
}
// docs:end excerpt-54



// docs:start excerpt-56
class LookupFailed {
    key: string,
}

function lookup(key: string) -> string {
    if (key == "missing") {
        throw LookupFailed { key };
    }
    `value:${key}`
}

function handle_task_error() -> string {
    let lookup_future = spawn "missing lookup" { lookup("missing") };
    (await lookup_future) catch (e) {
        LookupFailed => `not found: ${e.key}`,
    }
}
// docs:end excerpt-56



// docs:start excerpt-53
function cancel_task() -> string {
    let read_future = spawn "slow read" {
        baml.sys.sleep(baml.time.Duration.from_seconds(60n));
        "finished"
    };
    let requested = read_future.cancel();
    (await read_future) catch (e) {
        baml.panics.Cancelled => {
            if (requested) {
                "cancelled"
            } else {
                "already cancelled"
            }
        },
        baml.errors.Io => "sleep failed",
    }
}
// docs:end excerpt-53



function grouped_tasks() -> int[] {
    let group = baml.spawn.TaskGroup.new(0, name = "reads");
    let first_future = spawn "grouped first" with baml.spawn.options(group = group) { 1 };
    let second_future = spawn "grouped second" with baml.spawn.options(group = group) { 2 };
    let queued = group.queued_count();
    group.set_limit(2);
    let values = await baml.future.all([first_future, second_future]);
    [queued, values[0], values[1]]
}


test "concurrency behavior" {
    assert.equal(main(), ["contents of notes.md", "contents of outline.md"]);
    assert.equal(handle_task_error(), "not found: missing");
    assert.equal(cancel_task(), "cancelled");
    assert.equal(grouped_tasks(), [2, 1, 2]);
}

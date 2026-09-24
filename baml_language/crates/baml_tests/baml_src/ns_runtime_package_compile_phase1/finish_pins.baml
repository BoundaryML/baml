// `Package._finish` binds the dependencies the artifact was compiled
// against: the compile pins each package object under its alias, and the
// map handed to `_finish` must be exactly that map.

function dependency() -> reflect.Package throws unknown {
    reflect.Package.compile({ "dep.baml": `function answer() -> int { 42 }` })
}

function consumer_files() -> map<string, string> {
    { "main.baml": `function main() -> int { dep.answer() }` }
}

function finish_is_refused(artifact: reflect.CompileArtifact, packages: map<string, reflect.Package>) -> bool {
    let rejected = false;
    let _ = reflect.Package._finish(artifact, packages) catch (_) {
        _ => {
            rejected = true
        },
    };
    rejected
}

test "finish accepts the packages the artifact was compiled against" {
    let dep = dependency();
    let artifact = reflect.Package._compile(consumer_files(), { "dep": dep });
    let pkg = reflect.Package._finish(artifact, { "dep": dep });
    let main = pkg.get_function<() -> int>("root.main") ?? throw "missing main";
    assert.equal(main(), 42)
}

test "finish refuses a package the artifact was not compiled against" {
    let dep = dependency();
    let other = dependency();
    let artifact = reflect.Package._compile(consumer_files(), { "dep": dep });
    assert.is_true(finish_is_refused(artifact, { "dep": other }))
}

test "finish refuses a map that omits a pinned dependency" {
    let dep = dependency();
    let artifact = reflect.Package._compile(consumer_files(), { "dep": dep });
    assert.is_true(finish_is_refused(artifact, {}))
}

test "finish refuses a map with a package the artifact was not compiled against" {
    let dep = dependency();
    let artifact = reflect.Package._compile(consumer_files(), { "dep": dep });
    assert.is_true(finish_is_refused(artifact, { "dep": dep, "extra": dep }))
}

test "the artifact keeps its dependencies alive across a collection" {
    let dep = dependency();
    let artifact = reflect.Package._compile(consumer_files(), { "dep": dep });
    baml.sys.collect_garbage();
    let pkg = reflect.Package._finish(artifact, { "dep": dep });
    let main = pkg.get_function<() -> int>("root.main") ?? throw "missing main";
    assert.equal(main(), 42)
}

// A package interface locates a TRANSITIVE declaration by an edge path through
// its own dependency table, never by a name. Two libraries each reach a
// differently shaped `Err` under the same edge name `inner`; a consumer of
// their consumer must see the real one whichever order the compile mounts
// them in — the order used to decide which same-named root a program spelling
// resolved to.

function mti_inner() -> reflect.Package throws unknown {
    reflect.Package.compile({
        "i.baml": `
            class Err { message string }
            function fail() -> int { throw Err { message: "real" } }
        `,
    })
}

function mti_lib() -> reflect.Package throws unknown {
    reflect.Package.compile(
        { "l.baml": `function boom() -> int { inner.fail() }` },
        packages = { "inner": mti_inner() },
    )
}

// The same edge name, a different package behind it.
function mti_lib2() -> reflect.Package throws unknown {
    let other = reflect.Package.compile({ "x.baml": `class Err { code int }` });
    reflect.Package.compile(
        { "l2.baml": `function nothing() -> other_ref.Err? { null }` },
        packages = { "inner": other, "other_ref": other },
    )
}

function mti_consumer_lib_first() -> reflect.Package throws unknown {
    reflect.Package.compile(
        { "c1.baml": `function h() -> int { lib.boom() }` },
        packages = { "lib": mti_lib(), "zzz": mti_lib2() },
    )
}

function mti_consumer_lib2_first() -> reflect.Package throws unknown {
    reflect.Package.compile(
        { "c1.baml": `function h() -> int { lib.boom() }` },
        packages = { "zzz": mti_lib2(), "lib": mti_lib() },
    )
}

function mti_probe(c1: reflect.Package, src: string) -> string throws unknown {
    let c2 = reflect.Package.compile({ "c2.baml": src }, packages = { "c1": c1 }) catch (e) {
        reflect.errors.CompilationError => { return "COMPILE " + e.diagnostics[0].code }
    };
    let run = c2.get_function<() -> string>("root.run") ?? throw "missing run";
    "RAN " + run()
}

test "the real Err's field reads through two levels of consumers" {
    let src = `function run() -> string { c1.h().to_string() catch (e) { _ => e.message } }`;
    assert.equal(mti_probe(mti_consumer_lib_first(), src), "RAN real")
    assert.equal(mti_probe(mti_consumer_lib2_first(), src), "RAN real")
}

test "the other package's same-named Err is not this one" {
    let src = `function run() -> string { c1.h().to_string() catch (e) { _ => e.code.to_string() } }`;
    assert.equal(mti_probe(mti_consumer_lib_first(), src), "COMPILE E0007")
    assert.equal(mti_probe(mti_consumer_lib2_first(), src), "COMPILE E0007")
}

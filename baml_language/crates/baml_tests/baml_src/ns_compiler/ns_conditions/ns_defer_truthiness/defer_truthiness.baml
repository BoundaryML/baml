function compile(source: string) -> reflect.Package {
    let package = reflect.Package.compile({ "main.baml": source });
    assert.equal(package.diagnostics().length(), 0);
    package
}

test "deferred writes do not change conditions at registration" {
    let package = compile(
        `
            function main(flag: bool) -> int {
                defer { flag = true; }
                if (flag) { 1 } else { 0 }
            }
        `,
    );
    let main = package.get_function<(bool) -> int>("root.main") ?? baml.sys.panic("missing main");
    assert.equal(main(false), 0);
    assert.equal(main(true), 1);
}

test "deferred conditions do not use registration time facts" {
    let package = compile(
        `
            class Observed { value: int }
            function main() -> int {
                let flag = true;
                flag = true;
                let observed = Observed { value: 0 };
                {
                    defer {
                        if (flag) { observed.value = 1; } else { observed.value = 2; }
                    }
                    flag = false;
                }
                observed.value
            }
        `,
    );
    let main = package.get_function<() -> int>("root.main") ?? baml.sys.panic("missing main");
    assert.equal(main(), 2);
}

test "nested scope deferred writes invalidate outer local facts" {
    let package = compile(
        `
            function main(flag: bool) -> int {
                flag = true;
                { defer { flag = false; } }
                if (flag) { 1 } else { 0 }
            }
        `,
    );
    let main = package.get_function<(bool) -> int>("root.main") ?? baml.sys.panic("missing main");
    assert.equal(main(true), 0);
}

test "diverging defer does not make subsequent statements unreachable" {
    compile(
        `
            function main() -> int {
                defer { baml.sys.panic("deferred panic"); }
                7
            }
        `,
    );
}

test "tail less block with diverging defer still has never type" {
    compile(
        `
            function main() -> int {
                defer { throw "deferred failure"; }
            }
        `,
    );
}

test "deferred writes in both branches stay invalidated after merging" {
    let package = compile(
        `
            function main(flag: bool, branch: bool) -> int {
                flag = true;
                if (branch) {
                    defer { flag = false; }
                } else {
                    defer { flag = false; }
                }
                if (flag) { 1 } else { 2 }
            }
        `,
    );
    let main = package.get_function<(bool, bool) -> int>("root.main") ?? baml.sys.panic("missing main");
    assert.equal(main(true, true), 2);
    assert.equal(main(true, false), 2);
}

test "defer registration preserves current facts until scope exit" {
    let package = reflect.Package.compile(
        {
            "main.baml": `
                function main(flag: bool) -> int {
                    flag = false;
                    defer { flag = true; }
                    if (flag) { 1 } else { 0 }
                }
            `,
        },
    );
    let diagnostics = package.diagnostics();
    assert.equal(diagnostics.length(), 1);
    assert.equal(diagnostics[0].code, "E0167");
    assert.is_true(diagnostics[0].headline.includes("always falsy"));
    let main = package.get_function<(bool) -> int>("root.main") ?? baml.sys.panic("missing main");
    assert.equal(main(true), 0);
}

test "catch arm deferred writes invalidate outer local facts" {
    let package = compile(
        `
            function main(flag: bool) -> int {
                flag = true;
                (throw "caught") catch (error) {
                    _ => { defer { flag = false; } },
                };
                if (flag) { 1 } else { 0 }
            }
        `,
    );
    let main = package.get_function<(bool) -> int>("root.main") ?? baml.sys.panic("missing main");
    assert.equal(main(true), 0);
}

test "match arm deferred writes invalidate outer local facts" {
    let package = compile(
        `
            function main(flag: bool, branch: bool) -> int {
                flag = true;
                match (branch) {
                    true => { defer { flag = false; } },
                    false => {},
                };
                if (flag) { 1 } else { 0 }
            }
        `,
    );
    let main = package.get_function<(bool, bool) -> int>("root.main") ?? baml.sys.panic("missing main");
    assert.equal(main(true, true), 0);
    assert.equal(main(true, false), 1);
}

test "break does not apply false loop facts after deferred writes" {
    let package = compile(
        `
            class Marker {}
            function main(value: Marker?) -> bool {
                while (value) {
                    defer { value = Marker {}; }
                    break;
                }
                if (value) { true } else { false }
            }
            function run() -> bool { main(Marker {}) }
        `,
    );
    let run = package.get_function<() -> bool>("root.run") ?? baml.sys.panic("missing run");
    assert.is_true(run());
}

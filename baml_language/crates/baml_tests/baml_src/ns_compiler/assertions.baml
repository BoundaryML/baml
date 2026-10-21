// Shared assertions for compiler tests that compile isolated source snippets.
// Expectations are interface values so individual compiler suites can add
// richer matching policies without changing the compilation harness.

interface DiagnosticExpectation {
    function assert_matches(self, actual: reflect.Diagnostic) -> void throws never;
}

class ExactDiagnostic {
    code: string,
    message: string,

    implements DiagnosticExpectation {
        function assert_matches(self, actual: reflect.Diagnostic) -> void throws never {
            assert.equal(actual.code, self.code);
            assert.equal(actual.message, self.message);
        }
    }
}

function CompileDiagnostics(
    files: map<string, string>,
    packages: map<string, reflect.Package> = {},
) -> reflect.Diagnostic[] throws never {
    let artifact = reflect.Package._compile(files, packages) catch (error) {
        reflect.errors.CompilationError => {
            // A mounted package may already carry diagnostics of its own.
            // Assertions about this compilation should only see diagnostics
            // emitted for its input files (plus diagnostics without a span).
            return error.diagnostics.filter((diagnostic) -> bool {
                let span = diagnostic.span;
                if (span == null) {
                    true
                } else {
                    let file = span.file;
                    if (file == null) {
                        true
                    } else {
                        files.has(file)
                    }
                }
            });
        },
    };
    []
}

// Use the public compile door for successful diagnostics: `_compile` exposes
// failures through CompilationError, but its success artifact deliberately has
// no public diagnostic accessor. Package retains warnings after `_finish`.
function CompileSuccessDiagnostics(
    files: map<string, string>,
    packages: map<string, reflect.Package> = {},
) -> reflect.Diagnostic[] throws never {
    let package = reflect.Package.compile(files, packages = packages) catch_all (error) {
        _ => {
            baml.sys.panic(`expected compiler fixture to succeed: ${reflect._render_cause(error)}`)
        },
    };
    package.diagnostics()
}

function AssertSuccessHasMessage(
    files: map<string, string>,
    needle: string,
    packages: map<string, reflect.Package> = {},
) -> void throws never {
    for (let diagnostic in CompileSuccessDiagnostics(files, packages = packages)) {
        if (diagnostic.message.includes(needle)) {
            return;
        }
    }
    baml.sys.panic(`expected successful compiler diagnostic containing ${needle}`)
}

function AssertSuccessLacksMessage(
    files: map<string, string>,
    needle: string,
    packages: map<string, reflect.Package> = {},
) -> void throws never {
    for (let diagnostic in CompileSuccessDiagnostics(files, packages = packages)) {
        if (diagnostic.message.includes(needle)) {
            baml.sys.panic(`unexpected successful compiler diagnostic containing ${needle}`)
        }
    }
}

// Build a known-good dependency for mounted-package compiler tests without
// leaking Package._finish's open-ended initialization error into test APIs.
function CompileFixturePackage(
    files: map<string, string>,
    packages: map<string, reflect.Package> = {},
) -> reflect.Package throws never {
    let artifact = reflect.Package._compile(files, packages) catch (error) {
        reflect.errors.CompilationError => {
            baml.sys.panic(`compiler fixture failed to compile: ${error.message}`)
        },
    };
    reflect.Package._finish(artifact, packages) catch (error) {
        _ => {
            baml.sys.panic(`compiler fixture failed to initialize: ${reflect._render_cause(error)}`)
        },
    }
}

function AssertCompiles(
    files: map<string, string>,
    packages: map<string, reflect.Package> = {},
) -> void throws never {
    assert.equal(CompileDiagnostics(files, packages = packages).length(), 0)
}

function AssertHasCode(
    files: map<string, string>,
    code: string,
    packages: map<string, reflect.Package> = {},
) -> void throws never {
    let diagnostics = CompileDiagnostics(files, packages = packages);
    for (let diagnostic in diagnostics) {
        if (diagnostic.code == code) {
            return;
        }
    }
    baml.sys.panic(`expected compiler diagnostic code ${code}`)
}

function AssertHasMessage(
    files: map<string, string>,
    needle: string,
    packages: map<string, reflect.Package> = {},
) -> void throws never {
    let diagnostics = CompileDiagnostics(files, packages = packages);
    for (let diagnostic in diagnostics) {
        if (diagnostic.message.includes(needle)) {
            return;
        }
    }
    baml.sys.panic(`expected compiler diagnostic containing ${needle}`)
}

function AssertLacksMessage(
    files: map<string, string>,
    needle: string,
    packages: map<string, reflect.Package> = {},
) -> void throws never {
    let diagnostics = CompileDiagnostics(files, packages = packages);
    for (let diagnostic in diagnostics) {
        if (diagnostic.message.includes(needle)) {
            baml.sys.panic(`unexpected compiler diagnostic containing ${needle}`)
        }
    }
}

function AssertMessageCount(
    files: map<string, string>,
    needle: string,
    expected: int,
    packages: map<string, reflect.Package> = {},
) -> void throws never {
    let count = 0;
    for (let diagnostic in CompileDiagnostics(files, packages = packages)) {
        if (diagnostic.message.includes(needle)) {
            count += 1;
        }
    }
    assert.equal(count, expected)
}

function AssertRejected(
    files: map<string, string>,
    expected: DiagnosticExpectation,
    packages: map<string, reflect.Package> = {},
) -> void throws never {
    AssertMultiRejected(files, [expected], packages = packages)
}

function AssertMultiRejected(
    files: map<string, string>,
    expected: DiagnosticExpectation[],
    packages: map<string, reflect.Package> = {},
) -> void throws never {
    let actual = CompileDiagnostics(files, packages = packages);
    if (actual.length() != expected.length()) {
        let rendered = actual.map((diagnostic) -> string {
            let location = match (diagnostic.span) {
                null => "<no span>",
                let span: reflect.Span => {
                    `${span.file ?? "<unknown file>"}:${span.start}-${span.end}`
                },
            };
            `[${diagnostic.code}] ${location}: ${diagnostic.message}`
        });
        baml.sys.panic(
            `expected ${expected.length()} compiler diagnostic(s), got ${actual.length()}:\n${rendered.join("\n")}`,
        )
    }
    for (let index = 0; index < expected.length(); index += 1) {
        expected[index].assert_matches(actual[index]);
    }
}

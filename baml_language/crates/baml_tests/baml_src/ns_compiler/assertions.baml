// Shared assertions for compiler tests that compile isolated source snippets.
// Expectations are interface values so individual compiler suites can add
// richer matching policies without changing the compilation harness.

interface DiagnosticExpectation {
    function assert_matches(self, actual: reflect.Diagnostic) -> void throws never
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
    assert.equal(actual.length(), expected.length());
    for (let index = 0; index < expected.length(); index += 1) {
        expected[index].assert_matches(actual[index]);
    }
}

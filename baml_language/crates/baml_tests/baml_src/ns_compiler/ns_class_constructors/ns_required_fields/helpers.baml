// Shared helpers for required-field constructor validation.
//
// Every outer test is an ordinary BAML test. Its embedded source goes through
// the public runtime compiler instead of joining the outer project, which lets
// the test assert whether a snippet is admitted and, when admitted, what its
// constructor produces when executed.

function RunCompiledConstructorFiles(files: map<string, string>) -> string throws unknown {
    let package = reflect.Package.compile(files);
    let run = package.get_function<() -> string>("root.run") ?? throw "missing root.run";
    run()
}

function RunCompiledConstructor(source: string) -> string throws unknown {
    RunCompiledConstructorFiles({ "case.baml": source })
}

function RunMountedConstructor(
    dependency_source: string,
    consumer_source: string,
) -> string throws unknown {
    let dependency = reflect.Package.compile({ "dependency.baml": dependency_source });
    let consumer = reflect.Package.compile({ "consumer.baml": consumer_source }, packages = { "dep": dependency });
    let run = consumer.get_function<() -> string>("root.run") ?? throw "missing root.run";
    run()
}

function ConstructorCaseCompiles(source: string) -> bool throws unknown {
    let package = reflect.Package.compile({ "case.baml": source }) catch (e) {
        reflect.errors.CompilationError => {
            return false;
        },
        _ => throw e,
    };
    package.diagnostics().length() == 0
}

function ConstructorCaseFailsToCompile(source: string) -> bool throws unknown {
    !ConstructorCaseCompiles(source)
}

function ConstructorCaseFailsWith(source: string, message_parts: string[]) -> bool throws unknown {
    let package = reflect.Package.compile({ "case.baml": source }) catch (e) {
        reflect.errors.CompilationError => {
            let messages = e.diagnostics
                .map((diagnostic) -> string {
                    diagnostic.message
                })
                .join("\n");
            for (let part in message_parts) {
                if (!messages.includes(part)) {
                    return false;
                }
            }
            return true;
        },
        _ => throw e,
    };
    false
}

function ConstructorFilesFailWith(
    files: map<string, string>,
    message_parts: string[],
) -> bool throws unknown {
    let package = reflect.Package.compile(files) catch (e) {
        reflect.errors.CompilationError => {
            let messages = e.diagnostics
                .map((diagnostic) -> string {
                    diagnostic.message
                })
                .join("\n");
            for (let part in message_parts) {
                if (!messages.includes(part)) {
                    return false;
                }
            }
            return true;
        },
        _ => throw e,
    };
    false
}

function MountedConstructorCaseFailsWith(
    dependency_source: string,
    consumer_source: string,
    message_parts: string[],
) -> bool throws unknown {
    let dependency = reflect.Package.compile({ "dependency.baml": dependency_source });
    let consumer = reflect.Package.compile({ "consumer.baml": consumer_source }, packages = { "dep": dependency }) catch (e) {
        reflect.errors.CompilationError => {
            let messages = e.diagnostics
                .map((diagnostic) -> string {
                    diagnostic.message
                })
                .join("\n");
            for (let part in message_parts) {
                if (!messages.includes(part)) {
                    return false;
                }
            }
            return true;
        },
        _ => throw e,
    };
    false
}

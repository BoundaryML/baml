// Ask the compiler. This is the only place the tool learns anything about
// BAML semantics, and it learns it from `reflect.Package.compile`.

/// The compiler said what the key said.
class Agrees {
}

/// The compiler rejected the case with codes other than the key's (or the key
/// said it compiles).
class UnexpectedDiagnostics {
    codes: string[],
    messages: string[],
}

/// The key said `Rejected` but the case compiled.
class UnexpectedCompile {
}

/// The codes matched but none sat on a line mentioning the anchor.
class AnchorMismatch {
    anchor: string,
    lines: string[],
}

/// The case compiled but the package's initialisation threw.
class InitThrew {
    message: string,
}

/// The key lists tests the compiled package does not define.
class MissingTests {
    names: string[],
    defined: string[],
}

/// Listed tests that threw or panicked.
class FailedTests {
    names: string[],
}

type Outcome = Agrees
    | UnexpectedDiagnostics
    | UnexpectedCompile
    | AnchorMismatch
    | InitThrew
    | MissingTests
    | FailedTests;

function agrees(outcome: Outcome) -> bool throws never {
    match (outcome) {
        Agrees => true,
        UnexpectedDiagnostics => false,
        UnexpectedCompile => false,
        AnchorMismatch => false,
        InitThrew => false,
        MissingTests => false,
        FailedTests => false,
    }
}

/// The compiler rejected the sources.
class CompileFailed {
    diagnostics: reflect.Diagnostic[],
}

type Compiled = reflect.Package | CompileFailed | InitThrew;

function compile(files: map<string, string>) -> Compiled throws never {
    reflect.Package.compile(files) catch (e) {
        reflect.errors.CompilationError => CompileFailed { diagnostics: e.diagnostics },
        _ => InitThrew { message: `${e}` },
    }
}

/// Compile `case` and, if the key says so, run its tests.
function verify(case: Case) -> Outcome throws never {
    match (compile(case.files)) {
        let failed: CompileFailed => match (case.key) {
            Compiles => unexpected(failed.diagnostics),
            let rejected: Rejected => check_rejection(case.files, rejected, failed.diagnostics),
        },
        let threw: InitThrew => threw,
        let package: reflect.Package => match (case.key) {
            let expected: Compiles => run_tests(package, expected.tests),
            Rejected => UnexpectedCompile {  },
        },
    }
}

function unexpected(diagnostics: reflect.Diagnostic[]) -> Outcome throws never {
    UnexpectedDiagnostics {
        codes: diagnostics.map((d) -> {
            d.code
        }),
        messages: diagnostics.map((d) -> {
            d.message
        }),
    }
}

function sorted(values: string[]) -> string[] throws never {
    values.slice(0, values.length()).sort_by_key((v) -> {
        v
    })
}

function check_rejection(
    files: map<string, string>,
    expected: Rejected,
    diagnostics: reflect.Diagnostic[],
) -> Outcome throws never {
    if (
        sorted(
            diagnostics.map((d) -> {
                d.code
            }),
        )
            != sorted(expected.codes)
    ) {
        return unexpected(diagnostics);
    }
    let lines = diagnostics.filter_map((d) -> {
        line_of(files, d.span)
    });
    if (
        lines.some((line) -> {
            line.includes(expected.anchor)
        })
    ) {
        Agrees {  }
    } else {
        AnchorMismatch { anchor: expected.anchor, lines: lines }
    }
}

/// The source line containing the start of `span`, or `null` when the span
/// carries no file or names one that is not in `files`.
///
/// Diagnostic offsets are bytes while BAML's string methods index by
/// character, so the search runs over the UTF-8 bytes and decodes only the
/// newline-delimited line.
function line_of(files: map<string, string>, span: reflect.Span?) -> string? throws never {
    let s = span
        ?? return null;
    let name = s.file
        ?? return null;
    let bytes = (
        files.get(name)
            ?? return null
    )
        .to_utf8();
    let newline = 10;
    let line_start = match (bytes.slice(0, s.start).reverse().index_of(newline)) {
        let back: int => s.start - back,
        null => 0,
    };
    let line_end = match (bytes.slice(s.start, bytes.length()).index_of(newline)) {
        let ahead: int => s.start + ahead,
        null => bytes.length(),
    };
    // A newline is a single-byte code point, so a slice between newlines of
    // valid UTF-8 is itself valid UTF-8.
    string.from_utf8(bytes.slice(line_start, line_end)) catch (e) {
        baml.errors.InvalidArgument => {
            throw baml.panics.Unreachable {
            message: "a newline-delimited slice of valid UTF-8 failed to decode",
        }
        },
    }
}

type TestTable = map<string, () -> null throws unknown>;

function run_tests(package: reflect.Package, names: string[]) -> Outcome throws never {
    let tests: TestTable | InitThrew = package.tests() catch_all (e) {
        _ => InitThrew { message: `${e}` },
    };
    match (tests) {
        let threw: InitThrew => threw,
        let table: TestTable => {
            let missing = names.filter((n) -> {
                !table.has(n)
            });
            if (missing.length() > 0) {
                return MissingTests { names: missing, defined: table.keys() };
            }
            let failed = names.filter((n) -> {
                !passes(table, n)
            });
            if (failed.length() > 0) {
                FailedTests { names: failed }
            } else {
                Agrees {  }
            }
        },
    }
}

function passes(table: TestTable, name: string) -> bool throws never {
    // BUG: `tests()` is declared as `map<string, () -> null throws unknown>`
    // but its values reflect as `() -> void throws never`, and a run-time
    // membership test on a function value is exact rather than a subtyping
    // check, so `if let run: () -> null throws unknown = table.get(name)` and
    // the matching `is` both fail for a present key. The lookup therefore
    // goes through `has` and a subscript, which do not test the type.
    if (!table.has(name)) {
        return false;
    }
    let run = table[name];
    // Panics are their own channel: `catch_all` covers thrown errors only, so
    // an `assert` failure (`baml.panics.AssertionFailed`) inside the case's
    // test needs the explicit `baml.panics.Panic` arm.
    {
        run();
        true
    } catch (e) {
        baml.panics.Panic => false,
        _ => false,
    }
}

/// One item verified across a sweep of seeds.
class ItemReport {
    id: string,
    /// Seeds that produced a case; the rest ended with an unknown judgment.
    produced: int,
    /// Produced cases the compiler agreed with.
    agreed: int,
    disagreements: Disagreement[],
}

class Disagreement {
    index: int,
    outcome: Outcome,
}

/// Generate and verify `count` seeds of `item`.
function verify_item(item: Item, count: int) -> ItemReport throws never {
    let produced = 0;
    let agreed = 0;
    let disagreements: Disagreement[] = [];
    let i = 0;
    while (i < count) {
        if let candidate: Candidate = item.generate(seed_for(item.id(), i)) {
            produced = produced + 1;
            let outcome = verify(candidate.case);
            if (agrees(outcome)) {
                agreed = agreed + 1;
            } else {
                let _ = disagreements.push(Disagreement { index: i, outcome: outcome });
            }
        }
        i = i + 1;
    }
    ItemReport { id: item.id(), produced: produced, agreed: agreed, disagreements: disagreements }
}

/// Whether `report` is what `status` promises. `Verified` promises that at
/// least one case was produced and every produced case agrees; the other
/// statuses promise the opposite, so a compiler that catches up is noticed.
function conforms(status: Status, report: ItemReport) -> bool throws never {
    let all_agree = report.produced > 0 && report.disagreements.length() == 0;
    match (status) {
        Verified => all_agree,
        CompilerDiverges => !all_agree,
        SpecOnly => !all_agree,
    }
}

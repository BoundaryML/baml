// Ask the compiler. Together with `oracle.baml`, this is where the tool
// learns anything about BAML semantics, and it learns it from
// `reflect.Package.compile`.

/// The compiler said what the key said.
class Agrees {
}

/// The key said the case compiles; the compiler rejected it.
class UnexpectedDiagnostics {
    codes: string[],
    messages: string[],
}

/// The key said the case is rejected; the compiler accepted it.
class UnexpectedCompile {
}

/// The compiler rejected the case, but not with the key's codes.
class OtherCodes {
    expected: string[],
    codes: string[],
    messages: string[],
}

/// The codes matched, but no diagnostic points inside the key's region.
class Mislocated {
    at: Region,
    diagnostics: Located[],
}

/// A diagnostic's code and, when it has a span in a file, where it points.
class Located {
    code: string,
    at: Region?,
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

/// Tests of the compiled package that threw or panicked.
class FailedTests {
    failures: Failure[],
}

class Failure {
    name: string,
    reason: string,
}

type Outcome = Agrees
    | UnexpectedDiagnostics
    | UnexpectedCompile
    | OtherCodes
    | Mislocated
    | InitThrew
    | MissingTests
    | FailedTests;

function agrees(outcome: Outcome) -> bool throws never {
    outcome is Agrees
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

/// Compile `case` and, if the key says it compiles, run its tests.
///
/// # Panics
///
/// - `baml.panics.Unreachable` if the case has no files: an empty package
///   compiles, so a generator that produced one has a bug the verifier must
///   not paper over.
function verify(case: Case) -> Outcome throws never {
    if (case.files.length() == 0) {
        throw baml.panics.Unreachable { message: "a case must have at least one file" }
    }
    match (compile(case.files)) {
        let failed: CompileFailed => match (case.key) {
            Compiles => {
                UnexpectedDiagnostics {
                    codes: codes_of(failed.diagnostics),
                    messages: messages_of(failed.diagnostics),
                }
            },
            let rejected: Rejected => check_rejection(rejected, failed.diagnostics),
        },
        let threw: InitThrew => threw,
        let package: reflect.Package => match (case.key) {
            let expected: Compiles => run_tests(package, expected.tests),
            Rejected => UnexpectedCompile {  },
        },
    }
}

function codes_of(diagnostics: reflect.Diagnostic[]) -> string[] throws never {
    diagnostics.map((d) -> {
        d.code
    })
}

function messages_of(diagnostics: reflect.Diagnostic[]) -> string[] throws never {
    diagnostics.map((d) -> {
        d.message
    })
}

function sorted(values: string[]) -> string[] throws never {
    values.slice(0, values.length()).sort_by_key((v) -> {
        v
    })
}

/// The key's codes must match the diagnostics' as a multiset, and one of the
/// diagnostics must point inside the key's region.
function check_rejection(
    expected: Rejected,
    diagnostics: reflect.Diagnostic[],
) -> Outcome throws never {
    let codes = codes_of(diagnostics);
    if (sorted(codes) != sorted(expected.codes())) {
        return OtherCodes { expected: expected.codes(), codes, messages: messages_of(diagnostics) };
    }
    let inside = diagnostics.some((d) -> {
        within(d, expected.at)
    });
    if (inside) {
        Agrees {  }
    } else {
        Mislocated {
            at: expected.at,
            diagnostics: diagnostics.map((d) -> {
                locate(d)
            }),
        }
    }
}

/// Whether `d` points inside `at`.
function within(d: reflect.Diagnostic, at: Region) -> bool throws never {
    let span = d.span
        ?? return false;
    let file = span.file
        ?? return false;
    file == at.file && span.start >= at.start && span.end <= at.end
}

function locate(d: reflect.Diagnostic) -> Located throws never {
    let at: Region? = match (d.span) {
        let span: reflect.Span => match (span.file) {
            let file: string => Region { file, start: span.start, end: span.end },
            null => null,
        },
        null => null,
    };
    Located { code: d.code, at }
}

type TestTable = map<string, () -> null throws unknown>;

/// Every listed test must exist, and every test the package defines must
/// pass, listed or not: a compiling case is a whole well-behaved program.
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
            let failures = table.keys().filter_map((n) -> {
                failure_of(table, n)
            });
            if (failures.length() > 0) {
                FailedTests { failures }
            } else {
                Agrees {  }
            }
        },
    }
}

/// Why the test `name` failed, or `null` when it passed or does not exist.
function failure_of(table: TestTable, name: string) -> Failure? throws never {
    // BUG: `tests()` is declared as `map<string, () -> null throws unknown>`
    // but its values reflect as `() -> void throws never`, and `void` and
    // `null` function types are one type statically and two at run time, so
    // `if let run: () -> null throws unknown = table.get(name)` fails for a
    // present key. The lookup therefore goes through `has` and a subscript,
    // which do not test the type. See README, compiler issue 2.
    if (!table.has(name)) {
        return null;
    }
    let run = table[name];
    let passed: Failure? = null;
    // Panics are their own channel: `catch_all` covers thrown errors only, so
    // an `assert` failure (`baml.panics.AssertionFailed`) inside the case's
    // test needs the explicit `baml.panics.Panic` arm.
    {
        run();
        passed
    } catch (e) {
        baml.panics.Panic => Failure { name, reason: `${e}` },
        _ => Failure { name, reason: `${e}` },
    }
}

/// One item verified across a sweep of seeds.
class ItemReport {
    id: string,
    /// Seeds that produced a case; the rest ended with an unknown judgment.
    produced: int,
    /// Produced cases the compiler did not agree with.
    disagreements: Checked[],
}

/// A produced case's seed index and what the compiler said about it.
class Checked {
    index: int,
    outcome: Outcome,
}

/// An item is what the compiler says at `seed` only if every program it can
/// put a learner in front of there is. A foil is shown and marked exactly as
/// its case is, so its key is a claim about the compiler exactly as its
/// case's is.
function verify_at(item: Item, seed: Seed) -> Outcome? throws never {
    let candidate = generate(item, seed)
        ?? return null;
    let outcome = verify(candidate.case);
    if (!agrees(outcome)) {
        return outcome;
    }
    match (foil_of(item, seed)) {
        let foil: Foil => verify(foil.case),
        null => Agrees {  },
    }
}

/// Generate and verify `count` seeds of `item`.
function verify_item(item: Item, count: int) -> ItemReport throws never {
    let id = item.id();
    let checked: Checked[] = baml.iter.Range.new(0, count)
        .filter_map((i) -> {
            let outcome = verify_at(item, seed_for(id, i))
                ?? return null;
            Checked { index: i, outcome }
        })
        .collect();
    ItemReport {
        id,
        produced: checked.length(),
        disagreements: checked.filter((c) -> {
            !agrees(c.outcome)
        }),
    }
}

/// Whether `report` is what `status` promises. Every status promises that
/// at least one case was produced. `Verified` promises that every produced
/// case agrees; `CompilerDiverges` that every produced case disagrees in the
/// declared way, so a compiler that catches up, or breaks differently, is
/// noticed; `SpecOnly` that every produced case disagrees.
function conforms(status: Status, report: ItemReport) -> bool throws never {
    if (report.produced == 0) {
        return false;
    }
    let all_disagree = report.disagreements.length() == report.produced;
    match (status) {
        Verified => report.disagreements.length() == 0,
        let d: CompilerDiverges => {
            all_disagree
                && report.disagreements.every((c) -> {
                    contradicts(d.contradiction, c.outcome)
                })
        },
        SpecOnly => all_disagree,
    }
}

/// Whether `outcome` is the disagreement `contradiction` describes.
function contradicts(contradiction: Contradiction, outcome: Outcome) -> bool throws never {
    match (contradiction) {
        Contradiction.Accepts => outcome is UnexpectedCompile,
        Contradiction.Rejects => outcome is UnexpectedDiagnostics,
        Contradiction.Misreports => outcome is OtherCodes || outcome is Mislocated,
    }
}

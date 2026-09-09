// The shapes every layer above the engine speaks in. Nothing here knows what
// a BAML type is: the engine sees source files, a verdict, rules, and prose.

/// A heading in `TYPE_SYSTEM.md`.
class Section {
    heading: string,
}

/// An entry in `SPEC_GAPS.md`: a principle the spec does not yet state.
class Gap {
    id: string,
}

/// Where a rule's statement lives. A rule with no home is unrepresentable: it
/// either cites the spec or cites the ledger that says the spec is missing it.
type SpecRef = Section | Gap;

/// One principle of the type system, quoted from the spec.
///
/// Rules are values: a case's trace holds the rules themselves, so a trace can
/// never name a rule that does not exist. `name` is the identity used by the
/// learner profile and by naive models and must be unique across the bank.
class Rule {
    name: string,
    spec: SpecRef,
    statement: string,
}

/// One line of a case's explanation: a rule, and what it says at this case's
/// types. Rules are the currency of the learner profile and of naive models;
/// instances are the rubric a grader reads. Together, a case's claims
/// determine its verdict, so two cases built from the same rules but flowing
/// in opposite directions read differently.
class Claim {
    rule: Rule,
    /// The rule instantiated at the case's types, as prose with the types in
    /// backticks.
    instance: string,
}

/// The case compiles, and every listed test (by canonical id, `root::name`)
/// passes.
class Compiles {
    tests: string[],
}

/// The compiler rejects the case with exactly these diagnostic codes, at
/// least one of them pointing inside `at`.
class Rejected {
    code: string,
    /// Further codes beyond the first, when the case expects several.
    more: string[],
    /// The region the case's site expects the compiler to point at.
    at: Region,

    /// Every expected code, the first one first.
    function codes(self) -> string[] throws never {
        [self.code].concat(self.more)
    }
}

/// A byte range of one of a case's files. Diagnostics report byte offsets,
/// so regions do too.
class Region {
    /// A key of the case's `files`.
    file: string,
    start: int,
    end: int,
}

type Verdict = Compiles | Rejected;

/// A verdict without its details: what a session schedules and a learner
/// answers.
enum VerdictKind {
    Compiles,
    Rejected,
}

function verdict_kind(verdict: Verdict) -> VerdictKind throws never {
    match (verdict) {
        Compiles => VerdictKind.Compiles,
        Rejected => VerdictKind.Rejected,
    }
}

/// A generated quiz case: what the learner sees, what the compiler must say,
/// and the claims that say why.
class Case {
    /// Source text keyed by file name, exactly as handed to
    /// `reflect.Package.compile`. Keys are plain names: the compiler reports
    /// a normalised path, so a key with directories would never match a
    /// diagnostic's file.
    files: map<string, string>,
    key: Verdict,
    trace: Claim[],
}

/// The files of a case that has just one.
function one_file(name: string, source: string) -> map<string, string> throws never {
    let files: map<string, string> = {};
    files.set(name, source);
    files
}

/// A case together with the numbers the sampler scores it by.
class Candidate {
    case: Case,
    /// Feature values keyed by feature name. The bank defines both.
    features: map<string, float>,
    /// Ids of naive models whose prediction differs from `case.key`.
    traps: string[],
}

/// The compiler agrees with the spec on this item.
class Verified {
}

/// The way the compiler contradicts the spec on a diverging item.
enum Contradiction {
    /// It accepts what the spec rejects.
    Accepts,
    /// It rejects what the spec accepts.
    Rejects,
    /// It rejects as the spec does, but with other codes or elsewhere.
    Misreports,
}

/// The compiler contradicts the spec on this item; `divergence` names the
/// entry in `COMPILER_DIVERGENCE.md` and `contradiction` says how, so the
/// suite can tell the recorded divergence from a different breakage.
class CompilerDiverges {
    divergence: string,
    contradiction: Contradiction,
}

/// The spec describes something the compiler does not implement; `gap` names
/// the entry in `SPEC_GAPS.md`.
class SpecOnly {
    gap: string,
}

/// How an item stands relative to the compiler today. The conformance suite
/// asserts it in both directions: a `Verified` item that disagrees fails, and
/// a diverging item that agrees fails too, so promotion is always explicit.
type Status = Verified | CompilerDiverges | SpecOnly;

/// A generator of cases. Implementations live in the bank.
interface Item {
    /// Unique across the bank; keys seeds, reports, and the learner profile.
    function id(self) -> string throws never

    function status(self) -> Status throws never

    /// The case for `seed`, or `null` when the derivation ends without a
    /// known judgment. Must be a pure function of `seed`.
    function generate(self, seed: Seed) -> Candidate? throws never
}

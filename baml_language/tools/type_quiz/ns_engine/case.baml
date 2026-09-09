// The shapes every layer above the engine speaks in. Nothing here knows what
// a BAML type is: the engine sees source files, a verdict, and opaque ids.

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

/// The case compiles, and every listed test (by canonical id, `root::name`)
/// passes.
class Compiles {
    tests: string[],
}

/// The compiler rejects the case with exactly these diagnostic codes, at
/// least one of them on a line that mentions `anchor`.
class Rejected {
    codes: string[],
    anchor: string,
}

type Verdict = Compiles | Rejected;

/// A generated quiz case: what the learner sees, what the compiler must say,
/// and the rules that say why.
class Case {
    /// Source text keyed by path, exactly as handed to `reflect.Package.compile`.
    files: map<string, string>,
    key: Verdict,
    trace: Rule[],
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

/// The compiler contradicts the spec on this item; `divergence` names the
/// entry in `COMPILER_DIVERGENCE.md`.
class CompilerDiverges {
    divergence: string,
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

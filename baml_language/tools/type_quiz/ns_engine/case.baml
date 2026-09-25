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
///
/// The principle is held as the words `spec` uses for it, never a paraphrase:
/// the conformance suite finds every quote in the cited section or ledger
/// entry, so a rule cannot drift from the text it claims to state. A
/// principle the source spells across separate sentences takes several
/// quotes rather than one sentence of ours.
class Rule {
    name: string,
    spec: SpecRef,
    /// The first quote. Held apart from the rest so a rule without any words
    /// of the spec is unrepresentable.
    quote: string,
    more: string[],

    /// Every quote, in order.
    function quotes(self) -> string[] throws never {
        [self.quote].concat(self.more)
    }

    /// The principle as one line, elisions marked.
    function statement(self) -> string throws never {
        self.quotes().join(" … ")
    }
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
    /// Whether the answer turns on this claim.
    ///
    /// A claim can be part of a true explanation and still be something the
    /// learner did not need: if every value the claim could have taken leads
    /// to the same verdict, they can answer without it, and crediting them
    /// with the rule would be crediting them with reasoning they never had to
    /// do. The explanation keeps every claim, because it is the honest
    /// account of why; the tracer reads only the bearing ones.
    bearing: bool,
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

/// One way a draw could be served, as selection sees it: the rules an answer
/// to it would be evidence about, the naive models it would fool, and the
/// numbers the sampler scores it by. The bank defines the feature names.
class Casting {
    rules: string[],
    traps: string[],
    features: map<string, float>,
}

/// A case, and every way the draw it came from could have been served.
///
/// A draw has hidden coins — which way its pair flows, and for an item of
/// several facts which of them it is — and they decide the answer.
/// Selection reads `castings` and nothing else about the draw, so what it
/// prefers is the same whichever way the coins fell. Were it to read the
/// case itself it would serve whichever casting a naive model gets wrong, or
/// whichever holds the rule the tracer wants evidence on, and the kind of
/// question would give its answer away: sittings asked about function types
/// rejected ten times in eleven, and about unions that compiled seven in
/// ten, from a bank in which each answers both ways.
///
/// What an answer IS evidence about is `served`: this case's own rules and
/// traps.
class Candidate {
    case: Case,
    /// How `case` itself looks. Always one of `castings`.
    served: Casting,
    /// Every way the draw could have been served, `served` among them, in an
    /// order the draw fixes and its coins do not.
    castings: Casting[],

    /// `case`, served as `served`, from a draw that could have been served
    /// as any of `castings`.
    ///
    /// # Panics
    ///
    /// - `baml.panics.Unreachable` if `served` is not among `castings`: a
    ///   case is always one of the ways its own draw could be served, and
    ///   selection, reading only `castings`, would be scoring a draw that
    ///   did not include what it serves.
    function new(case: Case, served: Casting, castings: Casting[]) -> Candidate throws never {
        // `==`, not `includes`: `includes` compares class instances by
        // reference, and `served` is worked out apart from the list.
        let among = castings.some((casting) -> {
            casting == served
        });
        if (!among) {
            throw baml.panics.Unreachable { message: "a candidate served as something its draw could not be" }
        }
        Candidate { case, served, castings }
    }
}

/// A case's opposite: the same program with the one thing that decides its
/// verdict changed, so the compiler answers the other way.
///
/// Put beside the program it came from, a foil makes a question whose whole
/// answer is that one difference. Everything else about the two is the same,
/// so nothing on the surface can be read for the answer, and a learner who
/// knows a rule but not which way it runs cannot be right by luck.
class Foil {
    case: Case,
    /// What differs between the two, in the learner's terms.
    difference: string,
    /// The naive models that would pick the wrong one of the two. A model
    /// whose beliefs accept both programs, or neither, makes no pick at all,
    /// so the choice does not trap it even where one of the programs on its
    /// own does.
    traps: string[],
}

/// Two programs that differ in one thing, exactly one of which the compiler
/// accepts, shown in the order `compiles_first` gives.
///
/// The two are named by their verdicts rather than by their positions, so a
/// choice whose programs answer the same way is unrepresentable. Which is
/// shown first is a separate, seeded fact, and the right answer follows from
/// the two together rather than travelling beside them.
class Choice {
    compiles: Case,
    rejected: Case,
    difference: string,
    traps: string[],
    compiles_first: bool,

    /// The program shown first.
    function first(self) -> Case throws never {
        if (self.compiles_first) {
            self.compiles
        } else {
            self.rejected
        }
    }

    /// The program shown second.
    function second(self) -> Case throws never {
        if (self.compiles_first) {
            self.rejected
        } else {
            self.compiles
        }
    }

    /// Both, in the order they are shown.
    function shown(self) -> Case[] throws never {
        [self.first(), self.second()]
    }

    /// Whether the one the compiler accepts is the second of the two.
    function second_compiles(self) -> bool throws never {
        !self.compiles_first
    }
}

/// What a learner is put in front of: one program, asked whether the
/// compiler accepts it, or two that differ in one thing, asked which of them
/// it accepts.
type Posed = Case | Choice;

/// The programs of a posed question, in the order they are shown.
function cases_of(posed: Posed) -> Case[] throws never {
    match (posed) {
        let case: Case => [case],
        let choice: Choice => choice.shown(),
    }
}

/// Every rule a posed question turns on, in the order its explanation
/// reaches them.
///
/// What the programs of a question agree on: a single program's whole
/// explanation, and for two put side by side the part they share, which is
/// what a learner must read correctly before the difference between them can
/// decide anything. A rule that a case reaches twice is listed twice, which
/// is what makes a case of it deeper than one that reaches it once.
///
/// Only the claims the answer TURNS ON. A claim the verdict would be the
/// same without is part of the explanation and no part of the question: a
/// learner who cannot do that step still gets the case right, so asking it
/// tells us nothing about them and crediting them for it would be crediting
/// reasoning they never had to do. This is what keeps a case from looking
/// deeper than it is.
function rules_of(posed: Posed) -> string[] throws never {
    bearing_rules(
        shared_claims(
            cases_of(posed).map((case) -> {
                case.trace
            }),
        ),
    )
}

/// The rules of `trace` that its answer turns on, in order. What `rules_of`
/// reads a question by, for a trace that has not been made into a program.
function bearing_rules(trace: Claim[]) -> string[] throws never {
    trace
        .filter((claim) -> {
            claim.bearing
        })
        .map((claim) -> {
            claim.rule.name
        })
}

/// The longest run of claims every trace begins with.
function shared_claims(traces: Claim[][]) -> Claim[] throws never {
    let first = traces.at(0)
        ?? return [];
    let differs = baml.iter.Range.new(0, first.length()).find((i) -> {
        !traces.every((trace) -> {
            trace.length() > i && trace[i] == first[i]
        })
    });
    first.slice(0, differs ?? first.length())
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
    function id(self) -> string throws never;

    function status(self) -> Status throws never;

    /// The case for `seed`, or `null` when the derivation ends without a
    /// known judgment. Must be a pure function of `seed`.
    function generate(self, seed: Seed) -> Candidate? throws never;

    /// The foil of the case for `seed`, or `null` when this item has none to
    /// offer there. A pure function of `seed`, as `generate` is.
    ///
    /// Held apart from `generate` rather than carried on the candidate
    /// because a foil is a second whole program and most of them would never
    /// be seen: a sitting draws candidates by the handful and puts one, and
    /// puts that one as a choice only sometimes.
    function foil(self, seed: Seed) -> Foil? throws never;
}

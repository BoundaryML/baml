# stdlib-matrix

Answers, for each symbol in TypeScript's standard library, what a BAML developer
would do instead — or that BAML does not need it, or that nothing in BAML does
the job. The audience is someone who knows TypeScript and does not yet know
BAML, so the questions are asked in that direction and the report is keyed that
way.

The tool is itself a BAML program (`baml_src/`). `run` is a wrapper that
regenerates its two inputs, invokes it, and writes the artifacts.

```sh
tools/stdlib-matrix/run                                       # both surfaces, no judgements, no cost
tools/stdlib-matrix/run --llm                                 # judge everything
tools/stdlib-matrix/run --llm --previous report/matrix.json   # only re-judge what moved
tools/stdlib-matrix/run --llm --only Date                     # one container, for proving a change
tools/stdlib-matrix/run --help                                # every flag
```

Without `--llm` the run extracts both surfaces and concludes nothing: every
symbol comes out unjudged. Nothing pairs the two sides deterministically, by
design — name agreement was tried and removed, and `build_symbol_matrix` says
why.

`--llm` needs `OPENAI_API_KEY`; locally, `infisical run -- tools/stdlib-matrix/run --llm`.

To check a key before spending a run on it — or before putting it in a CI
environment — `check_client` makes one trial call and nothing else:

```sh
cd tools/stdlib-matrix && baml run check_client
```

It goes through `Aligner`, the same client the sessions use, and still answers
immediately: a rejected key arrives as `ai.errors.InvalidRequest`, which the
retry wrapper's default judgment declines to retry, so the provider's own
message comes through on the first attempt. That message is the point — a bad
key and a model this account may not use need different fixes and are otherwise
indistinguishable. A `--llm` run makes this call first, so it fails in seconds
rather than after a session's worth of work.

## How it judges

One session per TypeScript container — `(globals).Date`, `node:fs` — answering
for the container and every member it holds, in chunks of at most thirty. A
container is the unit because its members share a context: whoever has just
worked out that `Date` is `baml.time.Instant` is the cheapest possible judge of
`Date.now`.

A session is a loop, not a call. It is given the BAML skill up front and four
read-only tools, and it decides what to look up based on what it has already
seen:

| tool | what it answers |
|---|---|
| `describe` | `baml describe NAME` — one symbol or namespace, with its members and references |
| `search` | `baml describe --search` — free text over names *and* docstrings, for when the name is unknown |
| `examples` | real call sites from this repository, via `git grep` |
| `docs` | one section of `baml_language/TYPE_SYSTEM.md` |

Nothing is handed to the model as a shortlist. That was the previous design and
it was the ceiling on the answers: a menu built by string similarity cannot
contain `baml.sys.exec` when the question is `child_process.spawn`.

Three things a session must satisfy before its answer is accepted. Every symbol
it was asked about has an answer — an incomplete answer is sent back naming
exactly what is missing. Every BAML symbol it names exists — a claim that does
not resolve is refused and counted. Every code example compiles — the snippet
goes through the real `baml check`, and a failure comes back with the
diagnostic. A session that cannot satisfy them keeps what it got right and the
run records the rest as unjudged, which is honest; it is never recorded as a
finding.

## What it produces

`report/matrix.json` is canonical: every symbol on both sides, and a judgement
per TypeScript symbol recording the verdict — `match`, `divergent`,
`unnecessary`, or `none` — the BAML symbols it names, the reasoning, and
sometimes a compiled example showing both spellings. `report/matrix.md` renders
the same thing, and `report/last-run.json` is the raw result kept before any
post-processing, so a run that cost model time is never lost to a formatting
error.

`unnecessary` is the verdict worth understanding. `Array.isArray` is not missing
from BAML and has no counterpart: BAML checks types by pattern matching, so the
question only arises for a value in a union, and `foo is int[]` answers it
there. Rendering that as "no counterpart" would lose the whole explanation.

Note the asymmetry, because the numbers invite misreading. A TypeScript symbol
can be judged to have no counterpart — something looked, and that absence is a
finding. A BAML symbol that no judgement names is *not* the same claim: nothing
asked, so it is silence. `unmatched` and `baml_unclaimed` count those two
different things and must not be added up.

Nothing here is checked in. These are artifacts: the JSON is what the web view
at `typescript2/app-stdlib-matrix` fetches, and what CI publishes.

## How a run is made cheap

A cold run is roughly two hundred sessions, each up to seven model calls.
`--previous <report>` carries a judgement forward whenever the TypeScript symbol
*and every BAML symbol it names* read exactly as they did, so a release that
touched one namespace re-judges that namespace and nothing else.

A judged *absence* is treated differently, because it is a claim about the whole
BAML surface rather than about one symbol. It expires when that surface **gains**
a symbol — only new surface can falsify "there is nothing" — rather than on any
change at all, which would expire every absence on every run given that we are
the team editing it. `judgement_carries` states the gap this leaves.

`--review-all` keeps the previous report but carries nothing from it, putting
every symbol back to a session with its previous conclusion shown; use it when a
prompt changed, meaning the question moved rather than the surface.

`--check --baseline <report>` writes no report and only compares *inputs* — the
stdlib surface's content hash and the TypeScript release — exiting 1 when they
have moved. That is the cheap gate: it answers "is a rebuild worth paying for"
without paying for one. It refreshes `data/` on the way, since it has to read
the current surface to compare it; pass `--skip-extract` to reuse what is there.
It never calls a model, so `--llm` alongside it is refused rather than ignored.

## Publishing

`.github/workflows/pages.yml` publishes this site. That one workflow owns the
whole of the repository's GitHub Pages site, because a Pages deploy uploads a
complete site and replaces what was there: two workflows deploying
independently would each erase the other. This tool is published at the root
and the type-system quiz under `type-quiz/`, built together and deployed once.

It runs on the canary commits that can change what the matrix says — the
stdlib surface, this tool, the page, and the pinned TypeScript — or on demand.
It reads the report already published, uses `--check` to stop when the stdlib
has not moved, and passes the report as `--previous` when it has, so the
published report is its own cache and a typical commit costs a handful of
sessions. The site is rebuilt and handed over on every run either way, judged
or not: the deploy replaces everything, so a run that judged nothing still has
to publish the matrix as it stands.

It was built from the last release rather than from canary, which meant the
page described the stdlib as it was at the last cut rather than as it is.

A fetch of the published report that fails is now an error rather than a cold
run. After one publish the report is always there, so a failure means the site
is down, and treating that as "nothing to carry forward" would let an
unrelated commit spend a full judging pass. Ask for a cold run deliberately,
by dispatching the workflow with `force`.

The deploy gate is **coverage, not perfection**. At two hundred sessions some
transient failure is close to certain, and a run judging 2557 symbols with six
failed sessions is a good run; refusing on any failure at all would mean never
publishing again. So failures are annotated as warnings, and the run is blocked
only when it leaves *more* symbols unjudged than the report it would replace.

Three things it needs, none of which live in this repo:

1. **GitHub Pages enabled** for the repository, with the source set to *GitHub
   Actions*. That is how it is set today; nothing needs changing.
2. **`OPENAI_API_KEY`** in the `boundary-tools-prod` environment.
3. Optionally **`STDLIB_MATRIX_URL`** as a repository variable, if the site is
   served anywhere other than `https://boundaryml.github.io/baml`. It is what
   the workflow fetches the previous report from; get it wrong and every run
   fails asking for a deliberate cold one.

A published report written in an older format is treated as no report at all: it
cannot be read, and passing it to `--previous` would fail the job rather than
degrade to a cold run.

# stdlib-matrix

Compares the BAML standard library against TypeScript's, symbol by symbol, and
says what each BAML symbol corresponds to — or that it corresponds to nothing,
and why.

The tool is itself a BAML program (`baml_src/`). `run` is a wrapper that
regenerates its two inputs, invokes it, and writes the artifacts.

```sh
tools/stdlib-matrix/run                                   # name matching only, no model, no cost
tools/stdlib-matrix/run --llm                             # + the three model passes
tools/stdlib-matrix/run --llm --previous report/matrix.json   # only re-judge what moved
tools/stdlib-matrix/run --help                            # every flag
```

`--llm` needs `ANTHROPIC_API_KEY`; locally, `infisical run -- tools/stdlib-matrix/run --llm`.

## What it produces

`report/matrix.json` is canonical — every symbol on both sides, and a judgement
per BAML symbol recording the verdict (`match`, `divergent`, `none`), what it
rests on, the reasoning, and whether a second pass checked it. `report/matrix.md`
is a rendered view of the same thing, and `report/last-run.json` is the raw
result kept before any post-processing, so a run that cost model time is never
lost to a formatting error.

Neither is checked in. They are artifacts: the JSON is what the web view at
`typescript2/app-stdlib-matrix` fetches, and what CI publishes.

## How a run is made cheap

Judging the whole surface costs a few hundred model calls. `--previous <report>`
carries a judgement forward whenever the symbol *and every type it names* read
exactly as they did — so a release that touched one namespace re-judges that
namespace and nothing else. `--review-all` keeps the previous report but carries
nothing from it, putting every symbol back to the passes with its previous
conclusion shown; use it when a prompt changed, meaning the question moved
rather than the surface.

`--check --baseline <report>` writes no report and only compares *inputs* — the
stdlib surface's content hash and the TypeScript release — exiting 1 when they
have moved. That is the cheap gate: it answers "is a rebuild worth paying for"
without paying for one. It does refresh `data/` on the way, since it has to read
the current surface to compare it; pass `--skip-extract` to reuse what is there,
and note that it never calls a model, so `--llm` alongside it is refused rather
than ignored.

## Publishing

`.github/workflows/stdlib-matrix.yml` runs after a successful **BAML Language
Release** (or on demand) and deploys the JSON and the site to GitHub Pages. It
fetches the currently-deployed report, uses `--check` to stop when the stdlib
has not moved, and passes the report as `--previous` when it has — so the
deployed site is its own cache and a typical release costs a handful of calls.

Three things it needs, none of which live in this repo:

1. **GitHub Pages enabled** for the repository, with the source set to *GitHub
   Actions*. Until then the deploy step fails.
2. **`ANTHROPIC_API_KEY`** in the `boundary-tools-dev` environment.
3. Optionally **`STDLIB_MATRIX_URL`** as a repository variable, if the site is
   served anywhere other than `https://boundaryml.github.io/baml`. It is what
   the workflow fetches the previous report from; get it wrong and every run is
   a cold one.

A run that records any failed model call does not deploy. A partial report reads
exactly like a complete one — the symbols it never reached are simply unjudged,
which is also what an unreached symbol looks like — so the deployed site is kept
rather than quietly losing coverage.

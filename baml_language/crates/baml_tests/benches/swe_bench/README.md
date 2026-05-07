# Suite B — SWE-bench-style task corpus

This directory holds task specs and graders for **Suite B**: how well
Claude Code can write/fix programs in BAML vs Python vs Go from a spec.

## Layout

```
swe_bench/
├── README.md                     (this file)
├── prices.toml                   model → $/M tokens
└── tasks/
    ├── greenfield/<task>/        write-from-scratch tasks
    │   ├── spec.md               prompt
    │   ├── inputs/               fixtures
    │   ├── tests/{baml,python,go}/   graders (per-language)
    │   ├── reference/{baml,python,go}/   validates the grader before any agent runs
    │   └── qual_form.md          friction questionnaire (second-pass prompt)
    └── bugfix/<task>/<variant>/  edit-existing-code tasks (deferred for v1)
```

## What runs per cell

The harness (`benchmarking2/swe-bench-harness`) per
`(task, language, run_idx)`:

```
1. Stage tempdir at /tmp/staging/<cell-id>:
     inputs/                      ← copied from task
     tests/<lang>/                ← copied from task
     initial_repo/<lang>          ← bugfix only
2. Read spec.md; append a one-line language directive.
3. POST /run-claude (claude-proxy) with prompt + cwd = staging.
4. Parse claude --output-format json output for usage / turns / tool_calls.
5. Compute estimated_cost_usd from prices.toml.
6. POST /run-claude with qual_form.md → first balanced {…} parsed as JSON.
7. Run grader (pytest for python, go test for go).
8. POST one row to /benchmark-runs/<run-id>/results.
```

## Grader conventions

- Greenfield: tests just check the candidate output. Pass = all tests
  pass.
- Bugfix: tests partition into FAIL_TO_PASS / PASS_TO_PASS via name
  prefix. `test_f2p_*` (Python) and `TestF2P*` (Go) are FAIL_TO_PASS;
  everything else is PASS_TO_PASS. Pass = every FAIL_TO_PASS passes AND
  no PASS_TO_PASS regresses.

## Validating a task before merging it

```sh
# Stage as the harness would:
cd <empty tempdir>
cp -r tasks/greenfield/text-stats/inputs ./inputs
cp -r tasks/greenfield/text-stats/tests/python ./tests/python
cp tasks/greenfield/text-stats/reference/python/text_stats.py ./

python3 -m pytest tests/python -q
# Expected: 4 passed
```

For Go:
```sh
cp -r tasks/greenfield/text-stats/tests/go ./tests/go
cp tasks/greenfield/text-stats/reference/go/text_stats.go ./
echo 'module cell' > go.mod && echo 'go 1.24' >> go.mod
go test ./tests/go/...
```

## Tasks in v1

- **greenfield/text-stats** — count bytes/chars/words/lines in a UTF-8
  file, print a compact JSON object. Python + Go graders. BAML deferred.
- (Source plan: 5 more greenfield + 2 bugfix variants × 3 langs is the
  v1 scope; this checkin lands `text-stats` as the working canonical
  example.)

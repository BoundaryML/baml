# Adherence-by-model experiments

One script: `run_models.py`. Each model in `MODELS` builds the same small BAML
project (a receipt extractor) in a tool loop — it gets a real `baml` CLI tool so
it can learn the language with `baml describe` and iterate with `baml check` —
then the packed `baml-bench` grades every project with the SAME grader, and the
results are compared.

```bash
# grader (fixed for all models): the local claude-proxy
export LLM_BASE_URL=http://localhost:19090
export LLM_API_KEY=devproxytoken

# builders (metered APIs)
export OPENAI_API_KEY=sk-...
export ANTHROPIC_API_KEY=sk-ant-...

# no install needed — uv resolves the inline deps
uv run run_models.py          # build + grade everything
uv run run_models.py --report
```

Layout after a run:

```
runs/<provider>-<model>/project/   the model's BAML project
runs/<provider>-<model>/bench/     report.md, report.json, cache/
results.json                       the comparison table
```

Edit `MODELS` at the top of the script to add providers/models — anything with
an OpenAI-compatible chat endpoint works (Gemini line is included, commented).
Runs are resumable: existing projects are skipped unless `--force`, and grading
replays from each project's bench cache.

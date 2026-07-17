#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: run-claude-benchmark.sh PROJECT_DIR QUESTION [OUT_DIR]

Environment:
  REPS=2
  VARIANTS=natural,guided-grep,guided-hybrid-slim

Example:
  REPS=1 VARIANTS=natural,guided-hybrid-slim \
    ./run-claude-benchmark.sh ../baml-demos/bamlcode \
    "For agent.tool_edit_file, identify its contract and implementation dependencies. Include file:line citations."
EOF
}

if [ "$#" -lt 2 ] || [ "$#" -gt 3 ]; then
  usage >&2
  exit 1
fi

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(git -C "$script_dir" rev-parse --show-toplevel)"
lang_dir="$repo_root/baml_language"
project_source="$(cd "$1" && pwd)"
question="$2"
stamp="$(date +%Y%m%d-%H%M%S)"
outdir="${3:-$repo_root/target/describe-dependencies-bench-$stamp}"
reps="${REPS:-2}"
variants="${VARIANTS:-natural,guided-grep,guided-hybrid-slim}"

case "$reps" in
  ''|*[!0-9]*|0)
    echo "FATAL: REPS must be a positive integer" >&2
    exit 1
    ;;
esac

for command in cargo claude jq rsync; do
  command -v "$command" >/dev/null || {
    echo "FATAL: missing required command: $command" >&2
    exit 1
  }
done

mkdir -p "$outdir"
outdir="$(cd "$outdir" && pwd)"
target_dir="$(mktemp -d /tmp/baml-describe-target.XXXXXX)"

cleanup() {
  rm -rf "$target_dir"
}
trap cleanup EXIT

rsync -a \
  --exclude='.git' \
  --exclude='.agents' \
  --exclude='.claude' \
  --exclude='AGENTS.md' \
  --exclude='CLAUDE.md' \
  "$project_source/" "$target_dir/"

echo "==> building release baml-cli"
(cd "$lang_dir" && env RUSTC_WRAPPER= cargo build -q --release -p baml_cli --bin baml-cli)

bench_bin="$outdir/bin"
mkdir -p "$bench_bin"
cp "$lang_dir/target/release/baml-cli" "$bench_bin/baml-cli-frozen"
cat > "$bench_bin/baml" <<EOF
#!/usr/bin/env bash
exec env BAML_WRAPPER_EXEC=1 BAML_WRAPPER_RESOLVED_TOOLCHAIN=local-dev \
  "$bench_bin/baml-cli-frozen" "\$@"
EOF
chmod +x "$bench_bin/baml"

claude_home="$outdir/claude-home"
mkdir -p "$claude_home"
if [ -f "$HOME/.claude/.credentials.json" ]; then
  cp "$HOME/.claude/.credentials.json" "$claude_home/"
elif command -v security >/dev/null && security find-generic-password -s "Claude Code-credentials" -w > "$claude_home/.credentials.json" 2>/dev/null; then
  :
else
  echo "FATAL: Claude Code credentials were not found" >&2
  exit 1
fi
chmod 600 "$claude_home/.credentials.json"
jq '{oauthAccount, hasCompletedOnboarding, userID}' "$HOME/.claude.json" > "$claude_home/.claude.json"

guided_describe='This is a BAML project and the `baml` CLI is on PATH. Choose the cheapest view for the question: default overview for what a symbol is or does, source for implementation or errors, usage for callers/tests, impact for downstream blast radius, dependencies for what the symbol itself relies on, and search when the exact symbol is unknown. Normally use no more than two describe calls before narrow source verification. Do not run `baml describe --help`, dump broad project-wide output, or follow every next hint. Completeness and correctness beat command count.'
guided_grep='This is a BAML project. Use bounded lexical search and narrow source reads. Do not use `baml describe`. Avoid broad project-wide output. Completeness and correctness beat command count.'

materialize() {
  local variant="$1"
  rm -f "$target_dir/CLAUDE.md"
  case "$variant" in
    natural)
      ;;
    guided-grep)
      printf '%s\n' "$guided_grep" > "$target_dir/CLAUDE.md"
      ;;
    guided-hybrid-slim)
      printf '%s\n' "$guided_describe" > "$target_dir/CLAUDE.md"
      ;;
    *)
      echo "FATAL: unknown variant: $variant" >&2
      exit 1
      ;;
  esac
}

run_one() {
  local variant="$1"
  local rep="$2"
  local trace="$outdir/claude-$variant-r$rep.jsonl"
  local errf="$outdir/claude-$variant-r$rep.err"
  local prompt="$question"

  case "$variant" in
    guided-grep)
      prompt="$guided_grep

$question"
      ;;
    guided-hybrid-slim)
      prompt="$guided_describe

$question"
      ;;
  esac

  materialize "$variant"
  echo "==> [claude/$variant r$rep]"
  local start end
  start="$(date +%s)"
  (cd "$target_dir" && env PATH="$bench_bin:$PATH" CLAUDE_CONFIG_DIR="$claude_home" \
    claude -p "$prompt" \
      --strict-mcp-config \
      --output-format stream-json \
      --verbose \
      --allowedTools "Bash,Read,Glob,Grep" \
      --max-turns 40) \
    < /dev/null > "$trace" 2> "$errf"
  end="$(date +%s)"
  echo "$((end - start))" > "$outdir/claude-$variant-r$rep.wall"
  rm -f "$target_dir/CLAUDE.md"
}

summarize() {
  local summary="$outdir/summary.md"
  {
    echo "# Claude dependency benchmark $stamp"
    echo
    echo "Question: $question"
    echo
    echo "| run | wall | calls | BAML calls | cost | context | output |"
    echo "|---|---:|---:|---:|---:|---:|---:|"
    local variant rep trace wall calls baml_calls result
    for variant in ${variants//,/ }; do
      for rep in $(seq 1 "$reps"); do
        trace="$outdir/claude-$variant-r$rep.jsonl"
        wall="$(cat "$outdir/claude-$variant-r$rep.wall")"
        calls="$(jq -s '[.[] | select(.type=="assistant") | .message.content[]? | select(.type=="tool_use")] | length' "$trace")"
        baml_calls="$(jq -r 'select(.type=="assistant") | .message.content[]? | select(.type=="tool_use") | .input.command // ""' "$trace" | grep -c 'baml describe' || true)"
        result="$(jq -c 'select(.type=="result") | {cost:.total_cost_usd,context:(.usage.input_tokens+.usage.cache_creation_input_tokens+.usage.cache_read_input_tokens),output:.usage.output_tokens}' "$trace" | tail -1)"
        echo "| claude/$variant r$rep | ${wall}s | $calls | $baml_calls | \$$(jq -r '.cost' <<<"$result") | $(jq -r '.context' <<<"$result") | $(jq -r '.output' <<<"$result") |"
      done
    done
    echo
    for variant in ${variants//,/ }; do
      for rep in $(seq 1 "$reps"); do
        trace="$outdir/claude-$variant-r$rep.jsonl"
        echo "## claude/$variant r$rep — commands"
        echo '```'
        jq -r 'select(.type=="assistant") | .message.content[]? | select(.type=="tool_use") | "\(.name)\t\(.input.command // .input.file_path // .input.pattern // .input.description // \"\")"' "$trace"
        echo '```'
        echo
        echo "## claude/$variant r$rep — final answer"
        echo
        jq -r 'select(.type=="result") | .result' "$trace"
        echo
      done
    done
  } > "$summary"
  echo "==> summary: $summary"
}

{
  echo "project_source=$project_source"
  echo "question=$question"
  echo "variants=$variants"
  echo "reps=$reps"
  echo "baml_commit=$(git -C "$repo_root" rev-parse HEAD)"
} > "$outdir/run-config.txt"

for variant in ${variants//,/ }; do
  for rep in $(seq 1 "$reps"); do
    run_one "$variant" "$rep"
  done
done

summarize
echo "Raw results: $outdir"
echo "Do not publish the raw directory because claude-home contains credentials."

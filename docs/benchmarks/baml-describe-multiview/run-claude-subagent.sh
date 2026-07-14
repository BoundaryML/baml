#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: run-claude-subagent.sh PROJECT_DIR QUESTION [OUT_DIR]

Example:
  ./run-claude-subagent.sh ../baml-demos/bamlcode \
    "What kinds of errors can agent.tool_edit_file return or throw?"
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
outdir="${3:-$repo_root/target/describe-subagent-bench-$stamp}"

for command in cargo claude jq rsync; do
  command -v "$command" >/dev/null || {
    echo "FATAL: missing required command: $command" >&2
    exit 1
  }
done

mkdir -p "$outdir"
outdir="$(cd "$outdir" && pwd)"
target_dir="$(mktemp -d /tmp/baml-describe-subagent.XXXXXX)"

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

mkdir -p "$target_dir/.claude/agents"
cp "$script_dir/baml-describe-delegator.md" "$target_dir/CLAUDE.md"
cp "$script_dir/baml-describe-subagent.md" "$target_dir/.claude/agents/baml-describe-navigator.md"

prompt="This is a BAML project. Follow the project navigation guidance. Delegate the complete code-navigation question exactly once to the baml-describe-navigator project agent, without adding requested facts or broadening its scope. Wait for its evidence packet and answer from that packet. Do not inspect the codebase independently.

$question"

trace="$outdir/claude-guided-subagent-r1.jsonl"
errf="$outdir/claude-guided-subagent-r1.err"
wallf="$outdir/claude-guided-subagent-r1.wall"

echo "==> running Claude describe subagent"
start="$(date +%s)"
(cd "$target_dir" && env PATH="$bench_bin:$PATH" CLAUDE_CONFIG_DIR="$claude_home" \
  claude -p "$prompt" \
    --strict-mcp-config \
    --output-format stream-json \
    --verbose \
    --allowedTools "Agent,Bash" \
    --max-turns 40) \
  < /dev/null > "$trace" 2> "$errf"
end="$(date +%s)"
echo "$((end - start))" > "$wallf"

summary="$outdir/summary.md"
jq -s --arg question "$question" --argjson wall "$(cat "$wallf")" '
  ([.[] | select(.type=="result")] | last) as $result |
  {
    question: $question,
    wall: $wall,
    calls: ([.[] | select(.type=="assistant") | .message.content[]? | select(.type=="tool_use")] | length),
    baml_calls: ([.[] | select(.type=="assistant") | .message.content[]? | select(.type=="tool_use" and .name=="Bash") | .input.command // "" | select(contains("baml describe"))] | length),
    cost: $result.total_cost_usd,
    context: ([$result.modelUsage | to_entries[] | select(.key | contains("haiku") | not) | .value | (.inputTokens + .cacheReadInputTokens + .cacheCreationInputTokens)] | add),
    output: ([$result.modelUsage | to_entries[] | select(.key | contains("haiku") | not) | .value.outputTokens] | add),
    answer: $result.result
  }' "$trace" > "$outdir/metrics.json"

{
  echo "# Claude describe-subagent benchmark $stamp"
  echo
  echo "Question: $question"
  echo
  echo "| wall | calls | BAML calls | cost | context | output |"
  echo "|---:|---:|---:|---:|---:|---:|"
  jq -r '"| \(.wall)s | \(.calls) | \(.baml_calls) | $\(.cost) | \(.context) | \(.output) |"' "$outdir/metrics.json"
  echo
  echo "## Commands"
  echo '```'
  jq -r 'select(.type=="assistant") | .message.content[]? | select(.type=="tool_use") | "\(.name)\t\(.input.command // .input.subagent_type // .input.description // "")"' "$trace"
  echo '```'
  echo
  echo "## Final answer"
  echo
  jq -r '.answer' "$outdir/metrics.json"
} > "$summary"

{
  echo "project_source=$project_source"
  echo "question=$question"
  echo "baml_commit=$(git -C "$repo_root" rev-parse HEAD)"
  echo "claude=$(claude --version 2>/dev/null | head -1)"
  echo "subagent_sha1=$(shasum "$script_dir/baml-describe-subagent.md" | cut -d' ' -f1)"
  echo "delegator_sha1=$(shasum "$script_dir/baml-describe-delegator.md" | cut -d' ' -f1)"
} > "$outdir/run-config.txt"

echo "Summary: $summary"
echo "Raw results: $outdir"
echo "Do not publish the raw directory because claude-home contains credentials."

#!/usr/bin/env bash
# Starts the three site servers of the durable functions demo and stops them on
# exit (contract section 8.5).
#
#   local   http://127.0.0.1:<PORT_BASE>       run store server/.baml/runs<RUNS_TAG>-local
#   cloud   http://127.0.0.1:<PORT_BASE + 1>   run store server/.baml/runs<RUNS_TAG>-cloud
#   cloud2  http://127.0.0.1:<PORT_BASE + 2>   run store server/.baml/runs<RUNS_TAG>-cloud2
#
# Environment:
#   PORT_BASE    first port. Default: 8787
#   RUNS_TAG     added to the run store names, so that a second instance of the
#                demo can run next to the first one. Default: empty when
#                PORT_BASE is 8787, and -p<PORT_BASE> otherwise, so an instance
#                on other ports never shares the run stores of the default one.
#                Example: PORT_BASE=18787 RUNS_TAG=-test ./scripts/dev.sh
#   REMOTE_POOL  JSON array of the sites that may host remote_ calls.
#                Default (in the server): ["cloud","cloud2"]
#   WORKER_CMD   JSON array, the worker command prefix.
#                Default: ["node","scripts/mock-worker.mjs"] (relative to demo_durable/)
#                Real worker: WORKER_CMD='["/abs/path/to/baml-cli","worker"]'
#   PROGRAM_DIR  BAML project that the workers run. Default: ../program
#   BAML_LOG     log level of the site servers (off, error, warn, info, debug). Default: warn
#   CLEAN=1      delete the three run stores of this instance before starting
#
# Two instances must not share a run store: both would write the same meta.json
# files, and the cleanup of one would end the workers of the other. The script
# therefore claims each run store with the file <store>/.instance.lock, which
# holds its pid. It refuses to start, before it deletes or stops anything, when
# a run store is claimed by a live process or when a worker process already
# runs on it.
#
# Start the web app separately: (cd web && pnpm dev)
# For an instance on other ports:
#   LOCAL_SITE_URL=http://127.0.0.1:18787 CLOUD_SITE_URL=http://127.0.0.1:18788 \
#   CLOUD2_SITE_URL=http://127.0.0.1:18789 WEB_PORT=15173 pnpm dev
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT/server"

# The baml CLI refuses to run inside a coding agent session that has no BAML
# skill installed. The demo servers are not agent work, so skip that check.
export BAML_AGENT_SKILL_CHECK="${BAML_AGENT_SKILL_CHECK:-off}"
LOG_LEVEL="${BAML_LOG:-warn}"

PORT_BASE="${PORT_BASE:-8787}"
if ! [[ "$PORT_BASE" =~ ^[0-9]+$ ]] || (( PORT_BASE < 1 || PORT_BASE > 65533 )); then
  echo "PORT_BASE must be a port number between 1 and 65533, got: $PORT_BASE" >&2
  exit 1
fi
# An instance on other ports gets its own run stores unless RUNS_TAG says otherwise.
if [[ -z "${RUNS_TAG+set}" ]]; then
  if [[ "$PORT_BASE" == "8787" ]]; then RUNS_TAG=""; else RUNS_TAG="-p$PORT_BASE"; fi
fi
if ! [[ "$RUNS_TAG" =~ ^[A-Za-z0-9_-]*$ ]]; then
  echo "RUNS_TAG may contain only letters, digits, '-' and '_', got: $RUNS_TAG" >&2
  exit 1
fi

SITE_NAMES=(local cloud cloud2)
SITE_PORTS=("$PORT_BASE" "$((PORT_BASE + 1))" "$((PORT_BASE + 2))")
runs_dir() { echo ".baml/runs${RUNS_TAG}-$1"; }

# Every site server gets the same registry.
SITES_JSON="{"
for i in 0 1 2; do
  [[ $i -gt 0 ]] && SITES_JSON+=","
  SITES_JSON+="\"${SITE_NAMES[$i]}\":\"http://127.0.0.1:${SITE_PORTS[$i]}\""
done
SITES_JSON+="}"
export SITES="$SITES_JSON"
# PEER_URL is no longer read by the server.
unset PEER_URL

# This check runs before the cleanup trap is installed, so a failed start never
# touches a process of another instance.
for port in "${SITE_PORTS[@]}"; do
  if lsof -nP -iTCP:"$port" -sTCP:LISTEN >/dev/null 2>&1; then
    echo "port $port is already in use" >&2
    exit 1
  fi
done

# The run stores must belong to this instance alone. These checks also run
# before the trap is installed and before CLEAN deletes anything.
lock_file() { echo "$(runs_dir "$1")/.instance.lock"; }
for site in "${SITE_NAMES[@]}"; do
  lock="$(lock_file "$site")"
  if [[ -f "$lock" ]]; then
    owner="$(tr -cd '0-9' < "$lock" || true)"
    # A pid that was reused by an unrelated process does not count.
    if [[ -n "$owner" && "$owner" != "$$" ]] && kill -0 "$owner" 2>/dev/null \
      && [[ "$(ps -o command= -p "$owner" 2>/dev/null)" == *dev.sh* ]]; then
      echo "run store $ROOT/server/$(runs_dir "$site") is used by pid $owner; set RUNS_TAG to give this instance its own run stores" >&2
      exit 1
    fi
  fi
  dir="$ROOT/server/$(runs_dir "$site")/"
  while read -r pid cmd; do
    [[ -n "${pid:-}" ]] || continue
    if [[ "$cmd" == *"--snapshot-dir $dir"* ]]; then
      echo "run store ${dir%/} is used by the worker process $pid of another instance; set RUNS_TAG to give this instance its own run stores" >&2
      exit 1
    fi
  done < <(ps -axo pid=,command=)
done

if [[ "${CLEAN:-0}" == "1" ]]; then
  for site in "${SITE_NAMES[@]}"; do
    rm -rf "$(runs_dir "$site")"
  done
fi

LOCKED=()  # the lock files that this script wrote
for site in "${SITE_NAMES[@]}"; do
  mkdir -p "$(runs_dir "$site")"
  echo "$$" > "$(lock_file "$site")"
  LOCKED+=("$ROOT/server/$(lock_file "$site")")
done

PIDS=()           # the site servers that this script started
STARTED_PORTS=()  # their ports

# Prints the pids of `$1` and of every descendant of it.
process_tree() {
  local pid="$1" child
  echo "$pid"
  for child in $(pgrep -P "$pid" 2>/dev/null || true); do
    process_tree "$child"
  done
}

cleanup() {
  trap - EXIT INT TERM
  # The processes that belong to this instance: the servers that this script
  # started and their descendants (the workers). Collected before the servers
  # end, because an orphaned child loses the link to its parent.
  local own=() pid port site dir line cmd lock
  for pid in "${PIDS[@]:-}"; do
    [[ -n "$pid" ]] || continue
    while read -r line; do own+=("$line"); done < <(process_tree "$pid")
  done
  for pid in "${PIDS[@]:-}"; do
    [[ -n "$pid" ]] && kill "$pid" 2>/dev/null || true
  done
  # Workers exit when their stdin closes. Give them a moment.
  sleep 0.3
  # End any worker that still runs on one of this instance's run stores. The
  # server passes the absolute --snapshot-dir, and the match is a fixed string
  # that ends with "/", so runs-cloud does not match runs-cloud2 and an instance
  # with another RUNS_TAG is never matched.
  for site in "${SITE_NAMES[@]}"; do
    dir="$ROOT/server/$(runs_dir "$site")/"
    while read -r pid cmd; do
      [[ -n "${pid:-}" ]] || continue
      if [[ "$cmd" == *"--snapshot-dir $dir"* ]]; then
        kill "$pid" 2>/dev/null || true
      fi
    done < <(ps -axo pid=,command=)
  done
  # Make sure that nothing of this instance still listens on its ports. A
  # listener that this script did not start is left alone.
  for port in "${STARTED_PORTS[@]:-}"; do
    [[ -n "$port" ]] || continue
    for pid in $(lsof -nP -tiTCP:"$port" -sTCP:LISTEN 2>/dev/null || true); do
      if [[ " ${own[*]:-} " == *" $pid "* ]]; then
        kill "$pid" 2>/dev/null || true
      else
        echo "port $port: pid $pid was not started by this script, leaving it" >&2
      fi
    done
  done
  wait 2>/dev/null || true
  # Release the run stores. A lock that another process wrote in the meantime stays.
  for lock in "${LOCKED[@]:-}"; do
    [[ -n "$lock" && -f "$lock" ]] || continue
    if [[ "$(tr -cd '0-9' < "$lock" || true)" == "$$" ]]; then
      rm -f "$lock"
    fi
  done
}
trap cleanup EXIT INT TERM

for i in 0 1 2; do
  SITE="${SITE_NAMES[$i]}" PORT="${SITE_PORTS[$i]}" RUNS_DIR="$(runs_dir "${SITE_NAMES[$i]}")" \
    baml run main --log "$LOG_LEVEL" &
  PIDS+=($!)
  STARTED_PORTS+=("${SITE_PORTS[$i]}")
done

echo "site servers: local http://127.0.0.1:${SITE_PORTS[0]}, cloud http://127.0.0.1:${SITE_PORTS[1]}, cloud2 http://127.0.0.1:${SITE_PORTS[2]} (Ctrl-C stops all three)"
echo "run stores: $ROOT/server/.baml/runs${RUNS_TAG}-{local,cloud,cloud2}"
wait

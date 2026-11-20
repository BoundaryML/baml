#!/usr/bin/env python3
"""gcbench: measure the garbage collector's cost on BAML programs.

Each workload is packed with `baml-cli pack` and run by the VM, once per row:
a build of the CLI and pack host, plus environment variables (for example a
collector setting swept over several values).

    gcbench.py build     pack, verify and count collections (no timing)
    gcbench.py run       build what is missing, then measure and summarise
    gcbench.py summary   re-print the summary of a results.json
    gcbench.py calibrate suggest per-workload iteration counts for a target time
    gcbench.py memtrace  resident memory over time (Linux)

See README.md. Python 3.11+ standard library only.
"""

import argparse
import hashlib
import json
import math
import os
import platform
import re
import shutil
import signal
import statistics
import subprocess
import sys
import time
import tomllib
from dataclasses import dataclass, field
from pathlib import Path

HERE = Path(__file__).resolve().parent
BAML_LANGUAGE = HERE.parent.parent
WORKLOADS = BAML_LANGUAGE / "tools/speedtest/workloads"
# gcbench's own workloads (e.g. baseline/empty, a program that allocates nothing),
# used when an id is not found under speedtest's.
OWN_WORKLOADS = HERE / "workloads"
DEFAULT_CLI = "target/release/baml-cli"
ENTRY = "gcbench_entry"
# Inherited variables that would change a collector's behaviour: only a variant's
# own env may set them. BAML_TELEMETRY and BAML_HOME are set per run; a swept
# variable is dropped too (see clean_env), so an inherited value cannot leak into
# the baseline. glibc's malloc tunables change the footprint on Linux.
CONTROLLED_ENV = re.compile(r"^(BAML_GC_|BAML_TELEMETRY$|BAML_HOME$|MALLOC_|GLIBC_TUNABLES$)")
# Set by a job queue that runs gcbench while it holds back other work: when the
# machine still looks busy, measure after a short wait, with a warning.
QUEUED_ENV = "GCBENCH_QUEUED"
MIB = 1 << 20
NCPU = os.cpu_count() or 1


def log(message):
    print(f"[gcbench {time.strftime('%H:%M:%S')}] {message}", file=sys.stderr, flush=True)


def warn_loudly(message):
    bar = "!" * 78
    print(f"\n{bar}\n!! {message}\n{bar}\n", file=sys.stderr, flush=True)


def digest(*parts):
    h = hashlib.sha256()
    for part in parts:
        h.update(part if isinstance(part, bytes) else json.dumps(part, sort_keys=True).encode())
        h.update(b"\0")
    return h.hexdigest()[:16]


def file_digest(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()[:16]


# ---------------------------------------------------------------- configuration


@dataclass
class Variant:
    label: str
    baml_cli: Path | None = None    # the CLI that packs
    pack_host: Path | None = None   # the host embedded in the packed program
    pack_toml: str = ""             # appended to the packed project's baml.toml
    env: dict = field(default_factory=dict)
    sweep_var: str | None = None    # run once per value of this environment variable
    sweep_values: list = field(default_factory=list)
    relative_to: str | None = None  # the sweep tables' reference variant (default: the baseline)

    @property
    def env_vars(self):
        """What a run adds to the inherited environment: no telemetry unless the
        variant's env says otherwise."""
        return {"BAML_TELEMETRY": "off"} | self.env

    def describe(self):
        d = {"label": self.label, "env": self.env_vars, "pack_toml": self.pack_toml,
             "baml_cli": binary_identity(self.baml_cli, version=True), "pack_host": binary_identity(self.pack_host)}
        if self.sweep_var:
            d["sweep"] = {"var": self.sweep_var, "values": self.sweep_values, "relative_to": self.relative_to}
        return d


@dataclass
class Instance:
    """One row of the comparison: a variant, at one value of its swept
    environment variable if it has one."""
    variant: Variant
    value: str | None = None            # the swept variable's value, or None

    @property
    def label(self):
        return self.variant.label if self.value is None else f"{self.variant.label}@{self.value}"

    @property
    def setting(self):
        """The environment this instance adds beyond its variant's."""
        return {self.variant.sweep_var: self.value} if self.value is not None else {}


def parse_sweep(label, raw):
    """sweep = { var = "NAME", values = [1.5, 2, "3"] }: values become strings,
    written as in the config (an int stays an int: 2, not 2.0)."""
    if raw is None:
        return None, []
    if not isinstance(raw, dict) or not isinstance(raw.get("var"), str) or not raw.get("values"):
        sys.exit(f"variant {label}: sweep is {{ var = \"NAME\", values = [...] }}, not {raw!r}")
    values = [v if isinstance(v, str) else f"{v:g}" if isinstance(v, float) else str(v) for v in raw["values"]]
    if len(set(values)) != len(values):
        sys.exit(f"variant {label}: duplicate sweep values {values}")
    return raw["var"], values


def config_path(p):
    """A path from the config: `${VAR}` and `~` are expanded; a relative path is
    relative to baml_language/."""
    p = Path(os.path.expandvars(str(p))).expanduser()
    return p if p.is_absolute() else BAML_LANGUAGE / p


def load_config(path):
    raw = tomllib.loads(Path(path).read_text())
    variants = []
    for v in raw.get("variant", []):
        var, values = parse_sweep(v["label"], v.get("sweep"))
        cli = config_path(v.get("baml_cli", DEFAULT_CLI))
        variant = Variant(v["label"], cli, config_path(v["pack_host"]) if "pack_host" in v else cli.parent / "baml-pack-host",
                          str(v.get("pack_toml", "")), {k: str(x) for k, x in v.get("env", {}).items()},
                          var, values, v.get("relative_to"))
        try:
            tomllib.loads(pack_project_toml(variant))
        except tomllib.TOMLDecodeError as e:
            sys.exit(f"variant {v['label']}: pack_toml does not make a valid baml.toml: {e}")
        variants.append(variant)
    labels = [v.label for v in variants]
    if len(set(labels)) != len(labels):
        sys.exit(f"duplicate variant labels in {path}")
    for v in variants:
        if v.relative_to and v.relative_to not in labels:
            sys.exit(f"variant {v.label}: relative_to names no variant: {v.relative_to!r}")
    return {
        "baseline": raw.get("baseline", "vm"),
        "iters": int(raw.get("iters", 20)),
        "iters_override": {k: int(x) for k, x in raw.get("iters_override", {}).items()},
        "workloads": list(raw.get("workloads", [])),
        "variants": variants,
        "groups": {k: list(x) for k, x in raw.get("groups", {}).items()},
    }


def missing_binaries(variants):
    """Exit, naming every selected variant whose baml-cli or pack host is missing."""
    missing = [f"{v.label}: no {what} at {path}" for v in variants
               for what, path in (("baml-cli", v.baml_cli), ("baml-pack-host", v.pack_host)) if not path.is_file()]
    if missing:
        sys.exit("missing binaries (cargo build --release -p baml_cli -p baml_pack_host):\n  " + "\n  ".join(missing))


_identities = {}


def git_info(path):
    git = lambda *a: subprocess.run(["git", "-C", str(path.parent), *a], capture_output=True, text=True).stdout.strip()
    return {"commit": git("rev-parse", "HEAD") or None,
            "dirty": bool(git("status", "--porcelain", "--untracked-files=no"))}


def binary_identity(path, version=False):
    """A binary's content hash (the cache key), plus where it came from: the
    checkout's commit at the time of the run, the build time and, for the CLI,
    what `--version` says."""
    if path in _identities:
        return _identities[path]
    ident = {"path": str(path), "sha256": file_digest(path),
             "built": time.strftime("%Y-%m-%dT%H:%M:%S", time.localtime(path.stat().st_mtime)), **git_info(path)}
    if version:
        r = subprocess.run([str(path), "--version"], capture_output=True, text=True, env=cli_env())
        ident["version"] = (r.stdout.strip() or r.stderr.strip()).splitlines()[0][:200] if r.returncode == 0 else None
    _identities[path] = ident
    return ident


def cli_env():
    # The CLI refuses to run inside coding agents without an installed skill;
    # the harness is not an agent session (speedtest does the same).
    return {**os.environ, "DEV_BAML_CLI_DISABLE_AGENT_DETECTION": "1"}


# -------------------------------------------------------------------- workloads


@dataclass
class Workload:
    id: str
    dir: Path
    baml: str
    expect: str
    setup: bool
    result_type: str
    throws: str


def load_workload(wid):
    d = WORKLOADS / wid
    if not (d / "main.baml").exists() and (OWN_WORKLOADS / wid / "main.baml").exists():
        d = OWN_WORKLOADS / wid
    if not (d / "main.baml").exists():
        sys.exit(f"no workload {wid} (looked for {d / 'main.baml'})")
    baml = (d / "main.baml").read_text()
    bench = tomllib.loads((d / "bench.toml").read_text())
    setup = bool(re.search(r"^function setup\(", baml, re.M) and re.search(r"^function run\(", baml, re.M))
    sig = re.search(rf"^function {'run' if setup else 'main'}\([^)]*\)\s*->\s*(\w+)(\s+throws\s+[\w.]+)?", baml, re.M)
    if not sig:
        sys.exit(f"{wid}: cannot find the signature of {'run' if setup else 'main'}")
    return Workload(wid, d, baml, str(bench["expect"]), setup, sig.group(1), sig.group(2) or "")


def resolve_workloads(names, default):
    if not names:
        return default
    if names == ["all"]:
        return sorted(str(p.parent.relative_to(WORKLOADS)) for p in WORKLOADS.glob("*/*/main.baml"))
    out = []
    for name in names:
        hits = [w for w in default if w == name or w.endswith("/" + name)] or \
               [str(p.parent.relative_to(WORKLOADS)) for p in WORKLOADS.glob(f"*/{name}/main.baml")] or [name]
        out += [h for h in hits if h not in out]
    return out


def wrapper(w, n):
    """The entry function packed beside the workload's main.baml. It calls the workload n
    times (after one setup(), when it has one), folds every result into `acc` so no
    call can be dropped, and returns "<first result> <acc>": the first half is checked
    against bench.toml's expect, the second must agree between rows."""
    fold = {"int": "r % 1000003", "string": "r.length()", "bool": "if (r) { 1 } else { 0 }"}.get(w.result_type)
    if fold is None:
        raise ValueError(f"result type {w.result_type} is not supported")
    call = "run(state)" if w.setup else "main()"
    text = "first" if w.result_type == "string" else "first.to_string()"
    return f"""
function {ENTRY}() -> string{w.throws} {{
  {"let state = setup();" if w.setup else ""}
  let first = {call};
  let acc = 0;
  let i = 1;
  while (i < {n}) {{
    let r = {call};
    acc = (acc + {fold}) % 1000003;
    i += 1;
  }}
  {text} + " " + acc.to_string()
}}
"""


# ----------------------------------------------------------------------- builds


@dataclass
class Exe:
    path: Path | None
    key: str
    error: str | None = None


def pack_project_toml(variant):
    """The packed project's baml.toml. The packed program reads BAML_TELEMETRY
    only if baml.toml names it (as speedtest's rows do); without this the level is
    fixed at pack time. The variant's pack_toml follows the [pack.env_var_names]
    table, so keys before its first table header add names to that table."""
    toml = ('[package]\nname = "gcbench_workload"\n\n'
            '[pack.env_var_names]\nBAML_TELEMETRY = "BAML_TELEMETRY"\n')
    return toml + ("\n" + variant.pack_toml.strip() + "\n" if variant.pack_toml.strip() else "")


def build_vm(variant, w, n, cache):
    """Pack the workload with the variant's baml-cli and pack host: a project with
    main.baml and the generated entry function beside it, packed into one
    executable. The cache key is both binaries' content, the sources and the
    project file, so a rebuilt CLI or host repacks and an unchanged one does not."""
    try:
        entry = wrapper(w, n)
    except ValueError as e:
        return Exe(None, "", str(e))
    toml = pack_project_toml(variant)
    cli, host = binary_identity(variant.baml_cli), binary_identity(variant.pack_host)
    key = digest("vm", cli["sha256"], host["sha256"], toml, w.baml, entry)
    d = cache / "vm" / key
    meta = d / "meta.json"
    if meta.exists():
        m = json.loads(meta.read_text())
        return Exe(d / "exe" if m["ok"] else None, key, m.get("error"))
    if d.exists():
        shutil.rmtree(d)  # a pack that died half-way
    (d / "project/baml_src").mkdir(parents=True)
    (d / "project/baml.toml").write_text(toml)
    (d / "project/baml_src/main.baml").write_text(w.baml)
    (d / "project/baml_src/gcbench_entry.baml").write_text(entry)
    cmd = [str(variant.baml_cli), "--color", "never", "pack", ENTRY, "-o", str(d / "exe"),
           "--host-binary", str(variant.pack_host)]
    start = time.perf_counter()
    r = subprocess.run(cmd, cwd=d / "project", capture_output=True, text=True, env=cli_env(), stdin=subprocess.DEVNULL)
    ok = r.returncode == 0 and (d / "exe").exists()
    lines = [l for l in r.stderr.strip().splitlines() if l.strip() and not l.startswith("warning:")]
    error = None if ok else ("\n".join(lines[-5:]) or r.stdout.strip() or f"exit {r.returncode}")[-2000:]
    meta.write_text(json.dumps({"ok": ok, "error": error, "cmd": cmd, "workload": w.id, "n": n,
                                "seconds": round(time.perf_counter() - start, 2)}, indent=1))
    return Exe(d / "exe" if ok else None, key, error)


# ---------------------------------------------------------------------- running

TIME_FIELDS = {
    "max_rss_bytes": r"(\d+)\s+maximum resident set size",
    "page_reclaims": r"(\d+)\s+page reclaims",
    "page_faults_io": r"(\d+)\s+page faults",
    "instructions": r"(\d+)\s+instructions retired",
    "cycles": r"(\d+)\s+cycles elapsed",
    "peak_bytes": r"(\d+)\s+peak memory footprint",
}


# What the CPU column is: cycles from `/usr/bin/time -l` on macOS; on Linux,
# where perf counters are often closed to unprivileged users, user + system CPU time in
# nanoseconds from the child's resource usage. Stored in the trial's "cycles"
# field either way, so every ratio and table works the same; labels follow this.
CPU_METRIC = "cycles" if sys.platform == "darwin" else "cpu_ns"


def run_once(cmd, env, timeout=None):
    """One process, measured. macOS: under /usr/bin/time -l, in its own process
    group so a timeout kills the program and not only `time`. Linux: run_once_linux."""
    if sys.platform != "darwin":
        return run_once_linux(cmd, env, timeout)
    start = time.perf_counter_ns()
    p = subprocess.Popen(["/usr/bin/time", "-l", *map(str, cmd)], stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                         stdin=subprocess.DEVNULL, env=env, text=True, start_new_session=True)
    try:
        out, err = p.communicate(timeout=timeout)
    except subprocess.TimeoutExpired:
        os.killpg(p.pid, signal.SIGKILL)
        p.communicate()
        return {"exit": "timeout", "wall_ns": time.perf_counter_ns() - start, "stdout": ""}
    trial = {"exit": p.returncode, "wall_ns": time.perf_counter_ns() - start}
    for name, pattern in TIME_FIELDS.items():
        m = re.search(pattern, err)
        trial[name] = int(m.group(1)) if m else None
    if trial["page_reclaims"] is not None:
        trial["page_faults"] = trial["page_reclaims"] + (trial["page_faults_io"] or 0)
    lines = [l for l in out.splitlines() if l.strip()]
    trial["stdout"] = lines[-1] if lines else ""
    if p.returncode != 0:
        trial["stderr_tail"] = "\n".join(l for l in err.splitlines()[-40:] if not re.match(r"^\s+\d+\s+[a-z]", l))[-1500:]
        if p.returncode < 0:  # `time` re-raises the program's signal on itself
            trial["signal"] = -p.returncode
    return trial


def run_once_linux(cmd, env, timeout=None):
    """One process, reaped with wait4 so its own resource usage comes back:
    peak resident memory (ru_maxrss, KiB on Linux), user + system CPU time and
    page faults. The program is the direct child (no wrapper), so the numbers
    are the program's."""
    import threading
    start = time.perf_counter_ns()
    p = subprocess.Popen(list(map(str, cmd)), stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                         stdin=subprocess.DEVNULL, env=env, start_new_session=True)
    bufs = {}

    def drain(name, f):
        bufs[name] = f.read()

    readers = [threading.Thread(target=drain, args=(n, f), daemon=True) for n, f in (("out", p.stdout), ("err", p.stderr))]
    for t in readers:
        t.start()
    timed_out = []

    def kill():
        timed_out.append(True)
        try:
            os.killpg(p.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass

    timer = threading.Timer(timeout, kill) if timeout else None
    if timer:
        timer.start()
    _, status, ru = os.wait4(p.pid, 0)
    wall = time.perf_counter_ns() - start
    if timer:
        timer.cancel()
    p.returncode = os.waitstatus_to_exitcode(status)  # so Popen never waits on the reaped pid
    for t in readers:
        t.join()
    out = bufs.get("out", b"").decode(errors="replace")
    err = bufs.get("err", b"").decode(errors="replace")
    if timed_out:
        return {"exit": "timeout", "wall_ns": wall, "stdout": ""}
    trial = {"exit": p.returncode, "wall_ns": wall,
             "cycles": round((ru.ru_utime + ru.ru_stime) * 1e9), "cpu_user_ns": round(ru.ru_utime * 1e9),
             "cpu_sys_ns": round(ru.ru_stime * 1e9), "instructions": None,
             "max_rss_bytes": ru.ru_maxrss * 1024, "peak_bytes": ru.ru_maxrss * 1024,
             "page_reclaims": ru.ru_minflt, "page_faults_io": ru.ru_majflt,
             "page_faults": ru.ru_minflt + ru.ru_majflt}
    lines = [l for l in out.splitlines() if l.strip()]
    trial["stdout"] = lines[-1] if lines else ""
    if p.returncode != 0:
        trial["stderr_tail"] = "\n".join(err.splitlines()[-40:])[-1500:]
        if p.returncode < 0:
            trial["signal"] = -p.returncode
    return trial


def outcome(w, trial):
    """(outcome, acc). ok: right result; timeout: killed for running too long;
    wrong: exit 0 with a wrong result; crash: any other exit or a signal."""
    if trial["exit"] == "timeout":
        return "timeout", None
    if trial["exit"] != 0:
        return "crash", None
    ok, acc = check_output(w, trial)
    return ("ok" if ok else "wrong"), acc


def describe_failure(trial):
    tail = re.sub(r"\s*time: command terminated abnormally.*", "", trial.get("stderr_tail", ""), flags=re.S)
    how = f"killed by signal {trial['signal']}" if trial.get("signal") else f"exit {trial['exit']}"
    return f"{how}: {tail[-400:].strip()!r}"


def check_output(w, trial):
    """(ok, acc): the program prints "<first result> <acc>"."""
    if trial["exit"] != 0:
        return False, None
    out = trial["stdout"]
    if out.startswith('"'):  # a string result prints JSON-quoted, as `baml run` prints it
        try:
            out = json.loads(out)
        except ValueError:
            return False, None
    text, _, acc = out.rpartition(" ")
    return text == w.expect, acc


def verify(inst, w, cmd):
    """Run once untimed and check the result. A success is remembered next to the
    executable, per instance environment, so later invocations skip it."""
    env_vars = inst.variant.env_vars | inst.setting
    marker = Path(cmd[0]).parent / f"verified-{digest(env_vars, [str(c) for c in cmd[1:]])}.json"
    if marker.exists():
        record = json.loads(marker.read_text())
        if "peak_bytes" in record:
            return record
    t = run_trial(inst, cmd)
    result, acc = outcome(w, t)
    ok = result == "ok"
    record = {"ok": ok, "outcome": result, "acc": acc, "exit": t["exit"], "stdout": t["stdout"][:200],
              "peak_bytes": t.get("peak_bytes"), "failure": None if ok else describe_failure(t)}
    if ok:
        marker.write_text(json.dumps(record))
    return record


GC_QUERY = "SELECT duration, context_metadata FROM spans WHERE span_name = 'baml.gc'"


def gc_probe(inst, w, exe):
    """GC counts and pauses for a program, from one untimed run with telemetry
    on: the engine records every collection as a `baml.gc` span (its duration is
    the pause, from the request to the end of the collection, parking included;
    finalizers run after it), and `baml-cli local recordings query` reads them
    back. The timed runs keep telemetry off, so this costs them nothing; the
    probe's own counts can differ slightly from a telemetry-off run. The
    collection at shutdown is left out. Cached next to the executable."""
    v = inst.variant
    env_vars = v.env_vars | inst.setting
    marker = Path(exe).parent / f"gcprobe-{digest(env_vars)}.json"
    if marker.exists():
        return json.loads(marker.read_text())
    import tempfile
    with tempfile.TemporaryDirectory(prefix="gcbench-probe-") as root:
        root = Path(root)
        (root / "baml_src").mkdir()
        (root / "baml.toml").write_text('[package]\nname = "gcbench_probe"\n')
        level = env_vars.get("BAML_TELEMETRY", "off")
        env = run_env(inst) | {"BAML_HOME": str(root / ".baml"),
                                     "BAML_TELEMETRY": "low" if level == "off" else level}
        t = run_once([exe], env)
        if outcome(w, t)[0] != "ok":
            return {"error": f"probe run failed: {describe_failure(t)}"}
        r = subprocess.run([str(v.baml_cli), "--color", "never", "--output-format", "json", "local", "recordings",
                            "query", GC_QUERY], cwd=root, capture_output=True, text=True, env=cli_env())
    try:
        rows = json.loads(r.stdout)["rows"]
    except (ValueError, KeyError):
        return {"error": f"recordings query failed: {(r.stderr.strip() or r.stdout.strip())[-300:]}"}
    pauses = [int(d) for d, m in rows if not (isinstance(m, dict) and m.get("trigger") == "shutdown")]
    stats = {"collections": len(pauses), "pause_ns_total": sum(pauses), "pause_ns_max": max(pauses, default=0),
             "pause_ns": pauses, "probe_wall_ns": t["wall_ns"], "probe_telemetry": env["BAML_TELEMETRY"]}
    marker.write_text(json.dumps(stats))
    return stats


def apply_probe(a, probe, n):
    """Fill a row's GC columns from its probe runs (1 and n iterations)."""
    cold, steady = probe.get("cold") or {}, probe.get("steady") or {}
    if "collections" not in cold or "collections" not in steady:
        return
    for phase, s in (("cold", cold), ("steady", steady)):
        if phase not in a:
            continue
        a[phase]["collections_med"] = s["collections"]
        a[phase]["pause_total_ns_med"] = s["pause_ns_total"]
        a[phase]["pause_max_ns_med"] = s["pause_ns_max"] if s["pause_ns"] else None
        a[phase]["pause_p50_ns_med"] = pct(s["pause_ns"], 50)
        a[phase]["pause_p99_ns_med"] = pct(s["pause_ns"], 99)
        a[phase]["pause_share_med"] = s["pause_ns_total"] / s["probe_wall_ns"] if s["probe_wall_ns"] else None
    if "steady" in a and n > 1:
        a["steady"]["collections_per_iter"] = (steady["collections"] - cold["collections"]) / (n - 1)


def command(exes, phase, n):
    """The command line for one phase: cold = one iteration, steady = n iterations."""
    return [exes[1 if phase == "cold" else n]]


SWEPT_VARS = set()  # every variable any configured variant sweeps (filled by main)


def clean_env(base):
    return {k: v for k, v in base.items() if not CONTROLLED_ENV.match(k) and k not in SWEPT_VARS}


def run_env(inst):
    return {**clean_env(os.environ), **inst.variant.env_vars, **inst.setting}


_home_root = None


def run_trial(inst, cmd, timeout=None):
    """run_once with the instance's environment and a fresh, empty BAML_HOME per
    run (as speedtest's trials do), so nothing a program writes there carries
    over from one run to the next."""
    env = run_env(inst)
    global _home_root
    if _home_root is None:
        import atexit
        import tempfile
        _home_root = Path(tempfile.mkdtemp(prefix="gcbench-home-"))
        atexit.register(shutil.rmtree, _home_root, True)
    home = _home_root / "home"
    shutil.rmtree(home, ignore_errors=True)
    home.mkdir()
    return run_once(cmd, env | {"BAML_HOME": str(home)}, timeout)


# ------------------------------------------------------------------ machine load


def busy_cores():
    """Cores in use right now, measured over one second (load average lags by
    minutes, so right after builds drain it still reads high on an idle machine)."""
    try:
        if sys.platform == "darwin":
            out = subprocess.run(["top", "-l", "2", "-n", "0", "-s", "1"], capture_output=True, text=True).stdout
            idle = float(re.findall(r"CPU usage:.*?([\d.]+)% idle", out)[-1])
            return round((100 - idle) / 100 * NCPU, 2)
        def sample():
            f = [int(x) for x in open("/proc/stat").readline().split()[1:]]
            return sum(f), f[3] + f[4]
        (t0, i0), _ = sample(), time.sleep(1)
        t1, i1 = sample()
        return round((1 - (i1 - i0) / max(1, t1 - t0)) * NCPU, 2)
    except Exception:  # noqa: BLE001 - an unknown platform gets no gate, but says so
        return None


def load_snapshot():
    return {"loadavg": [round(x, 2) for x in os.getloadavg()], "busy_cores": busy_cores(), "time": time.time()}


def gate(args):
    """Refuse to measure on a busy machine: wait up to --wait-idle seconds for it to
    quiet down, then exit (or, with --force, carry on with a loud warning).

    With GCBENCH_QUEUED set (a job queue that holds back its other work, which may
    still show as busy), the wait is at most 30 s and then it measures with a
    warning. Every round records the outside load, and the summary names the
    noisy rounds."""
    queued = bool(os.environ.get(QUEUED_ENV))
    deadline = time.time() + (min(args.wait_idle, 30) if queued else args.wait_idle)
    while True:
        snap = load_snapshot()
        busy = snap["busy_cores"]
        if busy is None:
            warn_loudly("cannot measure CPU load on this platform; results may be noisy")
            return snap
        if busy <= args.max_busy:
            if snap["loadavg"][0] > NCPU / 2:
                log(f"load average {snap['loadavg']} is high but only {busy} cores are busy now (drained builds?)")
            return snap
        if time.time() > deadline:
            msg = f"machine busy: {busy} cores in use (limit --max-busy {args.max_busy}), load average {snap['loadavg']}"
            if queued and not args.force:
                warn_loudly(msg + f"; measuring anyway because {QUEUED_ENV} is set. Check the noisy rounds "
                            "in the summary.")
                return snap | {"gate": "queued"}
            if not args.force:
                warn_loudly(msg + "; refusing to measure (wait for an idle machine, or pass --force)")
                sys.exit(3)
            warn_loudly(msg + "; measuring anyway because of --force: TREAT THESE NUMBERS AS NOISE")
            return snap | {"gate": "forced"}
        log(f"{busy} cores busy, waiting for the machine to go idle")
        time.sleep(5)


# ------------------------------------------------------------------------ build


def prepare(cfg, args, cache):
    """Pack, verify and probe everything. Returns the plan. A row that fails
    verification on a workload is dropped from that workload (and reported
    loudly); the rest of its variant still runs."""
    variants = select_variants(cfg, args.variants)
    missing_binaries(variants)
    if cfg["baseline"] not in {v.label for v in variants}:
        warn_loudly(f"the baseline variant {cfg['baseline']!r} is not configured: ratios will be empty")
    workloads, skipped = [], {}
    exes, dropped, probes = {}, set(), {}
    for wid in resolve_workloads(args.workloads, cfg["workloads"]):
        w = load_workload(wid)
        n = cfg["iters_override"].get(wid, args.iters or cfg["iters"])
        problems, accs = [], {}
        for v in variants:
            built = {k: build_vm(v, w, k, cache) for k in (1, n)}
            bad = [e for e in built.values() if e.error]
            if bad:
                problems.append(f"{v.label}: does not compile: {bad[0].error.splitlines()[-1][:300]}")
                continue
            insts = instances([v])
            for inst in insts:
                for k, e in built.items():
                    t = verify(inst, w, [e.path])
                    if not t["ok"]:
                        problems.append(f"{inst.label}: N={k} FAILED verification ({t['outcome']}): "
                                        f"{t['failure'] if t['outcome'] != 'wrong' else repr(t['stdout'][:80])}")
                        dropped.add((inst.label, wid))
                        break
                    if k == n:
                        accs[inst.label] = t["acc"]
                if args.gc_probe and (inst.label, wid) not in dropped:
                    probe = {phase: gc_probe(inst, w, built[k].path) for phase, k in (("cold", 1), ("steady", n))}
                    errors = [p["error"] for p in probe.values() if "error" in p]
                    if errors:
                        log(f"  {wid}: {inst.label}: no GC numbers: {errors[0]}")
                    probes[(inst.label, wid)] = probe
            if all((i.label, wid) in dropped for i in insts):
                continue
            exes[(v.label, wid)] = {k: e.path for k, e in built.items()}
        if len(set(accs.values())) > 1:
            warn_loudly(f"{wid}: rows disagree on the accumulated result: {accs}")
        if problems:
            skipped[wid] = problems
            for p in problems:
                (warn_loudly if "FAILED" in p else log)(f"  {wid}: {p}")
        if any((v.label, wid) in exes for v in variants):
            workloads.append((w, n))
        log(f"ready {wid} (N={n})" if not problems else f"partial {wid}")
    return {"variants": variants, "workloads": workloads, "skipped": skipped, "exes": exes, "dropped": dropped,
            "gc_probe": probes}


def select_variants(cfg, names):
    """--variants takes labels and group names; the baseline (and any variant a
    sweep is relative to) always comes along, since the ratios need it."""
    if not names:
        return cfg["variants"]
    wanted = set()
    for name in names:
        if name in cfg["groups"]:
            wanted |= set(cfg["groups"][name])
        elif any(v.label == name for v in cfg["variants"]):
            wanted.add(name)
        else:
            sys.exit(f"no variant or group {name!r}; groups: {', '.join(cfg['groups']) or 'none'}")
    refs = {cfg["baseline"]} | {v.relative_to for v in cfg["variants"] if v.label in wanted and v.relative_to}
    for ref in sorted(refs - wanted):
        if any(v.label == ref for v in cfg["variants"]):
            log(f"adding the reference variant {ref!r}")
            wanted.add(ref)
    unknown = wanted - {v.label for v in cfg["variants"]}
    if unknown:
        sys.exit(f"groups name unknown variants: {sorted(unknown)}")
    return [v for v in cfg["variants"] if v.label in wanted]


def instances(variants, values=None):
    """values: sweep values to keep (from --values), e.g. {"2", "4"}."""
    out = []
    for v in variants:
        if v.sweep_var:
            out += [Instance(v, x) for x in v.sweep_values if values is None or x in values]
        else:
            out.append(Instance(v))
    return out


# ---------------------------------------------------------------------- measure


def measure(cfg, args, cache):
    plan = prepare(cfg, args, cache)
    start = gate(args)
    insts = instances(plan["variants"], set(args.values) if args.values else None)
    skip = plan["dropped"]
    out = Path(args.out or HERE / "results" / time.strftime("%Y%m%d-%H%M%S"))
    out.mkdir(parents=True, exist_ok=True)
    harness_git = lambda *a: subprocess.run(["git", "-C", str(HERE), *a], capture_output=True, text=True).stdout.strip()
    results = {
        "meta": {
            "date": time.strftime("%Y-%m-%dT%H:%M:%S%z"),
            "machine": machine(),
            "harness_commit": harness_git("rev-parse", "HEAD"),
            "rounds": args.rounds,
            "baseline": cfg["baseline"],
            "variants": [v.describe() for v in plan["variants"]],
            "workloads": {w.id: {"iters": n, "expect": w.expect, "setup": w.setup} for w, n in plan["workloads"]},
            "skipped": plan["skipped"],
            "instances": [{"label": i.label, "variant": i.variant.label, "sweep_var": i.variant.sweep_var,
                           "value": i.value} for i in insts],
            "gc_probe": [{"instance": k[0], "workload": k[1], **p} for k, p in plan["gc_probe"].items()],
            "load_start": start,
            "load_rounds": [],
            "round_seconds": [],
        },
        "trials": [],
    }
    path = out / "results.json"
    failures = []
    for r in range(args.rounds):
        snap = load_snapshot()
        results["meta"]["load_rounds"].append(snap)
        if snap["busy_cores"] is not None and snap["busy_cores"] > args.max_busy:
            warn_loudly(f"round {r + 1}: {snap['busy_cores']} cores busy from outside; this round is noisy")
        log(f"round {r + 1}/{args.rounds}")
        round_start = time.perf_counter()
        for w, n in plan["workloads"]:
            k = r % len(insts)
            for inst in insts[k:] + insts[:k]:  # rotate so no variant always runs first
                exes = plan["exes"].get((inst.variant.label, w.id))
                if exes is None or (inst.label, w.id) in skip:
                    continue
                for phase in ("cold", "steady"):
                    t = run_trial(inst, command(exes, phase, n), timeout=args.timeout)
                    t["outcome"], _ = outcome(w, t)
                    t["ok"] = t["outcome"] == "ok"
                    if t["outcome"] in ("crash", "wrong"):
                        failures.append(f"{inst.label} {w.id} {phase}: {t['outcome']}: {describe_failure(t)}")
                        warn_loudly(f"FAILED {failures[-1]}")
                    elif not t["ok"]:  # timeout
                        log(f"  {inst.label} {w.id} {phase}: {t['outcome']}")
                    results["trials"].append({"round": r, "workload": w.id, "variant": inst.variant.label,
                                              "instance": inst.label, "value": inst.value, "phase": phase, "iters": 1 if phase == "cold" else n, **t})
        results["meta"]["round_seconds"].append(round(time.perf_counter() - round_start, 1))
        log(f"round {r + 1} took {results['meta']['round_seconds'][-1]:.0f} s")
        path.write_text(json.dumps(results, indent=1))  # after every round: a killed run keeps its data
    results["meta"]["load_end"] = load_snapshot()
    path.write_text(json.dumps(results, indent=1))
    text = summarize(results)
    (out / "summary.md").write_text(text)
    print(text)
    log(f"wrote {path} and {out / 'summary.md'}")
    failures += [f"{wid}: {p}" for wid, ps in plan["skipped"].items() for p in ps if "FAILED" in p]
    if failures:
        warn_loudly(f"{len(failures)} failures (crashes or wrong results; details in the summary):\n"
                    + "\n".join(f[:200] for f in failures[:20]))
        sys.exit(4)


def machine():
    sysctl = lambda k: subprocess.run(["sysctl", "-n", k], capture_output=True, text=True).stdout.strip()
    info = {"platform": platform.platform(), "cores": NCPU, "python": platform.python_version()}
    if sys.platform == "darwin":
        info |= {"cpu": sysctl("machdep.cpu.brand_string"), "model": sysctl("hw.model"),
                 "memory_gib": round(int(sysctl("hw.memsize") or 0) / 2**30)}
    elif Path("/proc/cpuinfo").exists():
        cpu = re.search(r"^model name\s*:\s*(.+)$", Path("/proc/cpuinfo").read_text(), re.M)
        mem = re.search(r"^MemTotal:\s*(\d+) kB", Path("/proc/meminfo").read_text(), re.M)
        info |= {"cpu": cpu.group(1) if cpu else "?", "model": platform.node(),
                 "memory_gib": round(int(mem.group(1)) / 2**20) if mem else "?",
                 "libc": " ".join(platform.libc_ver()), "affinity_cores": len(os.sched_getaffinity(0))}
        try:  # a cgroup CPU limit (a job runner may cap the cores)
            info["cgroup_cpu_max"] = Path("/sys/fs/cgroup" + Path("/proc/self/cgroup").read_text().strip()
                                          .split("::")[-1] + "/cpu.max").read_text().strip()
        except (OSError, IndexError):
            pass
    info["cpu_metric"] = CPU_METRIC
    return info


# ---------------------------------------------------------------------- summary


def pct(values, q):
    s = sorted(values)
    return s[min(len(s) - 1, max(0, math.ceil(q / 100 * len(s)) - 1))] if s else None


def med(values):
    values = [v for v in values if v is not None]
    return statistics.median(values) if values else None


def aggregate(trials, n):
    """Cold and steady-state numbers for one (instance, workload) from its rounds."""
    cold = [t for t in trials if t["phase"] == "cold" and t["ok"] and t.get("cycles")]
    steady = [t for t in trials if t["phase"] == "steady" and t["ok"] and t.get("cycles")]
    bad = [t.get("outcome", "failed") for t in trials if not t["ok"]]
    a = {"rounds_ok": min(len(cold), len(steady)),
         "failed": ", ".join(f"{bad.count(o)} {o}" for o in dict.fromkeys(bad))}
    if not cold or not steady:
        return a
    bc, bs = min(cold, key=lambda t: t["cycles"]), min(steady, key=lambda t: t["cycles"])
    per = lambda s, c: (s - c) / (n - 1) if n > 1 and s is not None and c is not None else None
    a["cold"] = {"cycles_best": bc["cycles"], **{f"{k}_med": med(t[k] for t in cold) for k in
                 ("cycles", "instructions", "wall_ns", "page_faults", "peak_bytes")}}
    a["steady"] = {
        "cycles_best": per(bs["cycles"], bc["cycles"]),
        **{f"{k}_med": per(med(t[k] for t in steady), a["cold"][f"{k}_med"]) for k in
           ("cycles", "instructions", "wall_ns", "page_faults")},
        "peak_bytes_med": med(t["peak_bytes"] for t in steady),
    }
    return a


def failed(meta, label, wid):
    """Whether this row (or its whole variant) was dropped from the workload by a failure."""
    variant = next((i["variant"] for i in meta.get("instances", []) if i["label"] == label), label)
    return any(p.startswith((variant + ":", label + ":")) for p in meta["skipped"].get(wid, []))


def geomean(xs):
    return math.exp(sum(map(math.log, xs)) / len(xs))


def by_value(meta, agg, what, metric):
    """For each variant that sweeps an environment variable: one row per value,
    with the geometric mean over the workloads and one column per workload, each
    relative to the variant's reference (relative_to, default the baseline: the
    same build at its default settings)."""
    swept = list(dict.fromkeys(i["variant"] for i in meta.get("instances", []) if i.get("value") is not None))
    variants = {v["label"]: v for v in meta["variants"]}
    workloads = list(meta["workloads"])
    short = [wid.rsplit("/", 1)[-1] for wid in workloads]
    lines = []
    for name in swept:
        v = variants[name]
        ref = v.get("sweep", {}).get("relative_to") or meta["baseline"]
        var = v["sweep"]["var"]
        lines += [f"## {what} by `{var}` (`{name}`): relative to `{ref}`", "",
                  "Geometric mean over the workloads with data (count in brackets), then each workload. "
                  "Lower is better.", "",
                  f"| {var} | geomean | " + " | ".join(short) + " |", "|--:|--:|" + "--:|" * len(workloads)]
        for inst in (i for i in meta["instances"] if i["variant"] == name):
            cells, ratios = [], []
            for wid in workloads:
                x, y = metric(agg.get((inst["label"], wid), {})), metric(agg.get((ref, wid), {}))
                if x and y and x > 0 and y > 0:
                    ratios.append(x / y)
                    cells.append(f"{x / y:.3f}")
                else:
                    cells.append("FAILED" if failed(meta, inst["label"], wid) else "–")
            g = f"{geomean(ratios):.3f}x ({len(ratios)})" if ratios else "–"
            lines.append(f"| {inst['value']} | {g} | " + " | ".join(cells) + " |")
        lines.append("")
    return lines


def fmt(v, scale=1.0, digits=1):
    return "–" if v is None else f"{v / scale:,.{digits}f}"


def summarize(results):
    meta, trials = results["meta"], results["trials"]
    order = [i["label"] for i in meta.get("instances", [])]
    for t in trials:
        if t["instance"] not in order:
            order.append(t["instance"])
    probes = {(x["instance"], x["workload"]): x for x in meta.get("gc_probe", [])}
    agg = {}
    for wid, info in meta["workloads"].items():
        for inst in order:
            ts = [t for t in trials if t["workload"] == wid and t["instance"] == inst]
            if ts:
                agg[(inst, wid)] = aggregate(ts, info["iters"])
                if (inst, wid) in probes:
                    apply_probe(agg[(inst, wid)], probes[(inst, wid)], info["iters"])
    lines = [f"# gcbench {meta['date']}", ""]
    m = meta["machine"]
    cpu_ns = m.get("cpu_metric") == "cpu_ns"
    cpu_unit = "CPU ms" if cpu_ns else "Mcyc"
    lines += [f"- machine: {m.get('cpu', '?')} ({m.get('model', '?')}), {m['cores']} cores, "
              f"{m.get('memory_gib', '?')} GiB, {m['platform']}",
              f"- rounds: {meta['rounds']}; load average start {meta['load_start']['loadavg']} "
              f"({meta['load_start']['busy_cores']} cores busy), end {meta.get('load_end', {}).get('loadavg')} "
              f"({meta.get('load_end', {}).get('busy_cores')} cores busy)"]
    if meta["load_start"].get("gate") == "queued":
        lines.append(f"- **the machine was busy at the start; measured anyway because {QUEUED_ENV} was set**")
    if meta["load_start"].get("gate") == "forced":
        lines.append("- **measured on a busy machine because of --force**")
    loads = [s["busy_cores"] for s in meta["load_rounds"] if s["busy_cores"] is not None]
    noisy = [i + 1 for i, s in enumerate(meta["load_rounds"]) if (s["busy_cores"] or 0) > 1.5]
    if noisy:
        lines.append(f"- **noisy rounds (outside load > 1.5 cores): {noisy}**; outside load per round: {loads}")
    if meta.get("round_seconds"):
        rs = meta["round_seconds"]
        lines.append(f"- time per round: {', '.join(f'{x:.0f} s' for x in rs)} (mean {statistics.mean(rs):.0f} s)")
    origin = lambda p: (f"(checkout at {p['commit'][:10] if p.get('commit') else '?'}{'+dirty' if p.get('dirty') else ''}, "
                        f"sha256 {p['sha256']})")
    for v in meta["variants"]:
        extra = (f" baml-cli {v['baml_cli']['path']} {origin(v['baml_cli'])}"
                 f"{' ' + repr(v['baml_cli']['version']) if v['baml_cli'].get('version') else ''},"
                 f" pack host ({v['pack_host']['sha256']})")
        sweep = v.get("sweep")
        sweep = f" sweeps {sweep['var']} over {', '.join(sweep['values'])}" if sweep else ""
        lines.append(f"- `{v['label']}`: env {v['env']}{sweep}{extra}")
    lines += ["", "Steady = cost per iteration of the N-iteration build beyond the 1-iteration one; "
              "cold = the 1-iteration build (process start + setup + 1 call). CPU in "
              + ("milliseconds of user + system CPU time (wait4 resource usage)" if cpu_ns else "million cycles")
              + " (summed over threads), best of the rounds, median in brackets. Peak = "
              + ("peak resident set size (ru_maxrss)" if cpu_ns else "peak memory footprint")
              + " of the N-iteration run. GC columns come from the untimed telemetry probe run (each `baml.gc` "
              "span; its pause includes parking the VM's threads; the shutdown collection is left out); pauses "
              "are from the N-iteration run. fails counts runs that did not finish or gave a wrong result.", ""]
    for wid, info in meta["workloads"].items():
        lines += [f"## {wid} (N = {info['iters']})", "",
                  f"| variant | steady {cpu_unit}/iter | cold {cpu_unit} | steady wall ms/iter | peak MiB "
                  "| cold faults | GCs/iter | GCs cold | max pause ms | p99 pause ms | pause % | fails |",
                  "|---|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|"]
        for inst in order:
            a = agg.get((inst, wid))
            if not a:
                if failed(meta, inst, wid):
                    lines.append(f"| {inst} | **FAILED** (see Skipped) |" + " |" * 10)
                continue
            c, s = a.get("cold", {}), a.get("steady", {})
            lines.append(
                f"| {inst} | {fmt(s.get('cycles_best'), 1e6)} "
                f"({fmt(s.get('cycles_med'), 1e6)}) | {fmt(c.get('cycles_best'), 1e6)} ({fmt(c.get('cycles_med'), 1e6)}) "
                f"| {fmt(s.get('wall_ns_med'), 1e6, 2)} | {fmt(s.get('peak_bytes_med'), MIB)} "
                f"| {fmt(c.get('page_faults_med'), 1, 0)} | {fmt(s.get('collections_per_iter'), 1, 1)} "
                f"| {fmt(c.get('collections_med'), 1, 0)} "
                f"| {fmt(s.get('pause_max_ns_med'), 1e6, 2)} | {fmt(s.get('pause_p99_ns_med'), 1e6, 2)} "
                f"| {fmt(None if s.get('pause_share_med') is None else s['pause_share_med'] * 100, 1, 1)} "
                f"| {a['failed'] or ''} |")
        lines.append("")
    base = meta["baseline"]
    lines += by_value(meta, agg, "steady CPU", lambda a: a.get("steady", {}).get("cycles_best"))
    lines += by_value(meta, agg, "peak memory", lambda a: a.get("steady", {}).get("peak_bytes_med"))
    lines += [f"## Geometric mean relative to `{base}` (lower is better)", "",
              "| variant | steady CPU | cold CPU | peak memory | workloads (steady) |", "|---|--:|--:|--:|--:|"]
    for inst in order:
        ratios = {"steady": [], "cold": [], "peak": []}
        for wid in meta["workloads"]:
            a, b = agg.get((inst, wid), {}), agg.get((base, wid), {})
            for key, x, y in (("steady", a.get("steady", {}).get("cycles_best"), b.get("steady", {}).get("cycles_best")),
                              ("cold", a.get("cold", {}).get("cycles_best"), b.get("cold", {}).get("cycles_best")),
                              ("peak", a.get("steady", {}).get("peak_bytes_med"), b.get("steady", {}).get("peak_bytes_med"))):
                if x and y and x > 0 and y > 0:
                    ratios[key].append(x / y)
        g = lambda xs: f"{geomean(xs):.3f}x" if xs else "–"
        lines.append(f"| {inst} | {g(ratios['steady'])} | {g(ratios['cold'])} | {g(ratios['peak'])} "
                     f"| {len(ratios['steady'])} |")
    if meta["skipped"]:
        lines += ["", "## Skipped or partial", ""]
        for wid, problems in meta["skipped"].items():
            lines += [f"- {wid}: " + "; ".join(problems)]
    return "\n".join(lines) + "\n"


# -------------------------------------------------------------------- calibrate


def calibrate(cfg, args, cache):
    """Suggest per-workload iteration counts: pack the baseline (or the first
    variant) at 1 and K iterations, run each twice, and pick the N whose steady
    run takes about --target seconds. Prints an [iters_override] table to paste
    into the config. Not a measurement; keep the counts fixed while comparing."""
    if not cfg["variants"]:
        sys.exit("calibrate needs a variant in the config")
    v = next((v for v in cfg["variants"] if v.label == cfg["baseline"]), cfg["variants"][0])
    missing_binaries([v])
    k = args.iters or 6
    log(f"calibrating with {v.label}: N = 1 and {k}, target {args.target:g} s of steady wall time")
    print("[iters_override]")
    for wid in resolve_workloads(args.workloads, cfg["workloads"]):
        w = load_workload(wid)
        walls, kk, error = {}, k, None
        for _ in range(4):  # refine until the K-iteration run's extra time is at least a quarter of the target
            for n in (1, kk):
                if n in walls:
                    continue
                e = build_vm(v, w, n, cache)
                if e.error:
                    error = f"does not build: {e.error.splitlines()[-1][:200]}"
                    break
                ts = [run_trial(Instance(v), [e.path]) for _ in range(2)]
                if any(outcome(w, t)[0] != "ok" for t in ts):
                    error = "FAILED a calibration run"
                    break
                walls[n] = min(t["wall_ns"] for t in ts) / 1e9
            if error:
                break
            per = max(1e-6, (walls[kk] - walls[1]) / (kk - 1))
            if walls[kk] - walls[1] >= args.target / 4:
                break
            kk = max(kk + 1, min(round(args.target / 2 / per), 100_000))
        if error:
            print(f"# {wid}: {error}")
            continue
        n = max(2, round(args.target / per))
        print(f'"{wid}" = {n:<6} # {per * 1e3:.2f} ms/iter (from N = {kk}), cold {walls[1] * 1e3:.0f} ms')


def parse_env_sets(text):
    """'plain=;mmap=MALLOC_MMAP_THRESHOLD_=131072,MALLOC_ARENA_MAX=1' ->
    {"plain": {}, "mmap": {"MALLOC_MMAP_THRESHOLD_": "131072", "MALLOC_ARENA_MAX": "1"}}"""
    out = {}
    for part in filter(None, (x.strip() for x in text.split(";"))):
        name, _, rest = part.partition("=")
        out[name] = dict(kv.split("=", 1) for kv in filter(None, rest.split(",")))
    return out


def trace_rss(cmd, env, interval):
    """Run one process and sample its resident memory (/proc/<pid>/status VmRSS)
    every `interval` seconds until it exits. Returns the samples [(ms, KiB)],
    the kernel's peak (ru_maxrss, KiB), the exit status and the last stdout line."""
    import tempfile
    with tempfile.TemporaryFile() as out:
        start = time.perf_counter()
        p = subprocess.Popen(list(map(str, cmd)), stdout=out, stderr=subprocess.DEVNULL, stdin=subprocess.DEVNULL,
                             env=env, start_new_session=True)
        samples, status_path = [], f"/proc/{p.pid}/status"
        while True:
            pid, status, ru = os.wait4(p.pid, os.WNOHANG)
            if pid:
                break
            try:
                m = re.search(r"^VmRSS:\s*(\d+) kB", Path(status_path).read_text(), re.M)
                if m:
                    samples.append((round((time.perf_counter() - start) * 1e3, 1), int(m.group(1))))
            except OSError:
                pass
            time.sleep(interval)
        p.returncode = os.waitstatus_to_exitcode(status)
        out.seek(0)
        lines = [l for l in out.read().decode(errors="replace").splitlines() if l.strip()]
    return {"samples": samples, "maxrss_kib": ru.ru_maxrss, "exit": p.returncode,
            "wall_ms": round((time.perf_counter() - start) * 1e3, 1), "stdout": lines[-1] if lines else ""}


def memtrace(cfg, args, cache):
    """Resident memory over time (Linux): the baseline variant's N-iteration
    executable per workload, once per environment set (--env-sets), sampling
    VmRSS every --interval seconds. Writes memtrace.json and memtrace.md."""
    if not Path("/proc/self/status").exists():
        sys.exit("memtrace needs Linux (/proc/<pid>/status)")
    plan = prepare(cfg, args, cache)
    v = next((v for v in plan["variants"] if v.label == cfg["baseline"]), plan["variants"][0])
    sets = parse_env_sets(args.env_sets)
    out = Path(args.out or HERE / "results" / time.strftime("memtrace-%Y%m%d-%H%M%S"))
    out.mkdir(parents=True, exist_ok=True)
    runs = []
    for w, n in plan["workloads"]:
        exes = plan["exes"].get((v.label, w.id))
        if not exes:
            continue
        for name, extra in sets.items():
            import tempfile
            env = run_env(Instance(v)) | extra | {"BAML_HOME": tempfile.mkdtemp(prefix="gcbench-home-")}
            r = trace_rss([exes[n]], env, args.interval)
            shutil.rmtree(env["BAML_HOME"], ignore_errors=True)
            ok, _ = check_output(w, {"exit": r["exit"], "stdout": r["stdout"]})
            kib = [k for _, k in r["samples"]]
            peak_at = kib.index(max(kib)) if kib else 0
            drops = [a - b for a, b in zip(kib, kib[1:]) if a - b >= 1024]
            runs.append({"workload": w.id, "iters": n, "variant": v.label, "env_set": name, "env": extra, "ok": ok,
                         **r, "peak_sample_kib": max(kib, default=None), "end_kib": kib[-1] if kib else None,
                         "min_after_peak_kib": min(kib[peak_at:], default=None), "drops_ge_1mib": len(drops),
                         "largest_drop_kib": max(drops, default=0)})
            log(f"  {w.id} [{name}]: peak {r['maxrss_kib'] / 1024:.1f} MiB, end "
                f"{(kib[-1] if kib else 0) / 1024:.1f} MiB, {len(kib)} samples{'' if ok else ', WRONG RESULT'}")
    (out / "memtrace.json").write_text(json.dumps({"date": time.strftime("%Y-%m-%dT%H:%M:%S%z"), "machine": machine(),
                                                   "variant": v.describe(), "interval_s": args.interval,
                                                   "env_sets": sets, "runs": runs}, indent=1))
    mib = lambda k: "–" if k is None else f"{k / 1024:.1f}"
    lines = [f"# memtrace {time.strftime('%Y-%m-%d %H:%M')}", "",
             f"Variant `{v.label}`, the N-iteration executable, one run per workload and environment set; VmRSS "
             f"sampled every {args.interval * 1e3:g} ms. Peak = ru_maxrss; end = the last sample before exit; "
             "min after peak = the lowest sample after the highest one; drops = sample-to-sample falls of 1 MiB "
             "or more.", "",
             "| workload | env set | peak MiB | end MiB | min after peak MiB | drops ≥ 1 MiB | largest drop MiB "
             "| wall ms | samples | ok |", "|---|---|--:|--:|--:|--:|--:|--:|--:|---|"]
    for r in runs:
        lines.append(f"| {r['workload']} | {r['env_set']} | {mib(r['maxrss_kib'])} | {mib(r['end_kib'])} "
                     f"| {mib(r['min_after_peak_kib'])} | {r['drops_ge_1mib']} | {mib(r['largest_drop_kib'])} "
                     f"| {r['wall_ms']:.0f} | {len(r['samples'])} | {'yes' if r['ok'] else 'NO'} |")
    lines += ["", "## Resident memory over time (MiB, 24 evenly spaced samples per run)", ""]
    for r in runs:
        k = [x for _, x in r["samples"]]
        pick = [k[round(i * (len(k) - 1) / 23)] for i in range(24)] if len(k) >= 24 else k
        lines.append(f"- {r['workload']} [{r['env_set']}]: " + " ".join(f"{x / 1024:.0f}" for x in pick))
    (out / "memtrace.md").write_text("\n".join(lines) + "\n")
    print("\n".join(lines))
    log(f"wrote {out / 'memtrace.json'} and {out / 'memtrace.md'}")


# ---------------------------------------------------------------------- command


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("command", choices=["build", "run", "summary", "calibrate", "memtrace"])
    ap.add_argument("results", nargs="?", help="results.json, for `summary`")
    ap.add_argument("--config", default=HERE / "variants.toml")
    ap.add_argument("--variants", type=lambda s: s.split(","), help="comma-separated labels or groups (default: all)")
    ap.add_argument("--workloads", type=lambda s: s.split(","), help="comma-separated ids or names, or `all` (default: config)")
    ap.add_argument("--values", type=lambda s: s.split(","),
                    help="sweep values to run, e.g. 2,4 (default: each swept variant's list)")
    ap.add_argument("--iters", type=int, help="steady-state iterations (default: config; calibrate: K, default 6)")
    ap.add_argument("--target", type=float, default=0.5, help="calibrate: seconds of steady wall time per run")
    ap.add_argument("--rounds", type=int, default=5)
    ap.add_argument("--cache", default=os.environ.get("GCBENCH_CACHE", Path.home() / ".cache/baml-gcbench"))
    ap.add_argument("--out", help="results directory (default: tools/gcbench/results/<timestamp>)")
    ap.add_argument("--timeout", type=float, default=600, help="seconds per timed run")
    ap.add_argument("--max-busy", type=float, default=1.0, help="cores busy from outside before refusing")
    ap.add_argument("--wait-idle", type=float, default=120, help="seconds to wait for an idle machine")
    ap.add_argument("--force", action="store_true", help="measure on a busy machine, with a warning")
    ap.add_argument("--env-sets", default="plain=",
                    help="memtrace: environment sets, e.g. 'plain=;arena1=MALLOC_ARENA_MAX=1'")
    ap.add_argument("--interval", type=float, default=0.01, help="memtrace: seconds between samples")
    ap.add_argument("--no-gc-probe", dest="gc_probe", action="store_false",
                    help="skip the untimed telemetry run that counts collections and pauses")
    args = ap.parse_args()
    if args.command == "summary":
        print(summarize(json.loads(Path(args.results).read_text())))
        return
    cfg = load_config(args.config)
    SWEPT_VARS.update(v.sweep_var for v in cfg["variants"] if v.sweep_var)
    cache = Path(args.cache)
    if args.command == "calibrate":
        calibrate(cfg, args, cache)
    elif args.command == "memtrace":
        memtrace(cfg, args, cache)
    elif args.command == "build":
        plan = prepare(cfg, args, cache)
        log(f"{len(plan['workloads'])} workloads ready, {len(plan['skipped'])} skipped or partial")
    else:
        measure(cfg, args, cache)


if __name__ == "__main__":
    main()

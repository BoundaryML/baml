"""Shards: the protocol split across machines, planned from a previous bundle and merged back into one.

A shard is a set of whole scenarios: every row of a scenario, the control
row included, runs on one machine, so every bar and its drift reference
share a host. A shard builds and verifies only the cases its scenarios use.

Planning. A machine j that runs scenarios S_j over cases C_j finishes at

    T_j = B0 + Binc * (|C_j| - 1) + sum_{c in C_j} v_c + sum_{s in S_j} t_s

with B0 a clean native build, Binc each further case's build, v_c a case's
verification and t_s a scenario's trials (process wall plus harness overhead).
`plan` finds the fewest machines with every T_j within `slack` of the floor
B0 + max_s (v_c(s) + t_s): bin packing with a per-case setup cost, solved
exactly by branch and bound (the protocols have a few dozen scenarios).
"""
import csv
import json
import os
import re
import shutil
import statistics
from pathlib import Path

from . import runner

# The order a merged run reports the worst status in.
STATUS_ORDER = ["shard_failed", "native_build_failed", "verify_failed", "no_trials", "trials_failed", "ok"]


# ── planning ─────────────────────────────────────────────────────────────────

def costs(bundle):
    """Seconds per scenario, per case verification, and the clean and warm builds, from a bundle."""
    bundle = Path(bundle)
    protocol = json.loads((bundle / "results" / "protocol.json").read_text())
    manifest = json.loads((bundle / "manifest.json").read_text())
    case_of = {s["name"]: s["case"] for s in protocol["scenarios"]}
    with (bundle / "results" / "trials.csv").open() as f:
        trials = list(csv.DictReader(f))
    process = {}
    for r in trials:
        process[r["scenario"]] = process.get(r["scenario"], 0.0) + float(r["process_wall_ns"] or 0) / 1e9
    # Harness time between trials, spread evenly: the trial phase's wall time less the processes' time.
    stamps = log_stamps(bundle / "results" / "execute.log")
    scheduled = next((t for t, line in stamps if " trials scheduled" in line), None)
    done = next((t for t, line in stamps if line.startswith("done:")), None)
    overhead = max(0.0, ((done - scheduled) % 86400 - sum(process.values())) / len(trials)) if scheduled is not None and done is not None and trials else 0.0
    count = {}
    for r in trials:
        count[r["scenario"]] = count.get(r["scenario"], 0) + 1
    t = {s: (case_of[s], process[s] + overhead * count[s]) for s in process}
    verify, prev = {}, None
    for when, line in stamps:
        m = re.match(r"verify (\S+): ", line)
        if m and prev is not None:
            verify[m.group(1)] = (when - prev) % 86400
        prev = when
    builds = [b for b in manifest.get("builds", {}).values() if b.get("status") == "ok"]
    clean = [b["seconds"] for b in builds if b.get("clean")]
    warm = [b["seconds"] for b in builds if not b.get("clean")]
    return t, verify, (clean[0] if clean else max(warm, default=0.0)), (statistics.median(warm) if warm else 0.0)


def log_stamps(path):
    out = []
    for line in Path(path).read_text().splitlines() if Path(path).exists() else []:
        m = re.match(r"(\d\d):(\d\d):(\d\d) (.*)", line)
        if m:
            out.append((int(m[1]) * 3600 + int(m[2]) * 60 + int(m[3]), m[4]))
    return out


def plan(t, verify, b0, binc, slack=0.0):
    """Fewest machines with every finish time within `slack` of the floor; returns (finish, bins)."""
    items = sorted(((s, c, x) for s, (c, x) in t.items()), key=lambda i: -i[2])

    def finish(bin_):
        cases = {c for _, c, _ in bin_}
        return b0 + binc * (len(cases) - 1) + sum(verify.get(c, 0.0) for c in cases) + sum(x for *_, x in bin_)

    def place(bins, i):
        if i == len(items):
            return True
        seen = set()
        for b in bins:
            key = (frozenset(c for _, c, _ in b), round(sum(x for *_, x in b), 3))
            if key in seen:
                continue                      # an equivalent machine was already tried
            seen.add(key)
            b.append(items[i])
            if finish(b) <= limit and place(bins, i + 1):
                return True
            b.pop()
            if not b:
                break                         # empty machines are interchangeable
        return False

    limit = max(finish([i]) for i in items) + slack
    for n in range(1, len(items) + 1):
        bins = [[] for _ in range(n)]
        if place(bins, 0):
            return [(finish(b), [s for s, _, _ in b]) for b in bins]


# ── running ──────────────────────────────────────────────────────────────────

def split(protocol, directory):
    """One protocol file per shard: its scenarios, and only the cases they use. One shard when none are declared."""
    d = json.loads(Path(protocol.path).read_text())
    groups = d.pop("shards", None) or [[s["name"] for s in d["scenarios"]]]
    files = []
    for k, names in enumerate(groups):
        scenarios = [s for s in d["scenarios"] if s["name"] in names]
        used = {s["case"] for s in scenarios}
        part = {**d, "cases": [c for c in d["cases"] if c in used], "scenarios": scenarios, "shard": f"{k + 1}/{len(groups)}"}
        path = Path(directory) / f"protocol-{k + 1}.json"
        path.write_text(json.dumps(part, indent=1))
        files.append(path)
    return files


def stage_for(stage, protocol_file, dest):
    """The stage with this shard's protocol: hard links, so the source archive is not copied."""
    shutil.copytree(stage, dest, copy_function=os.link)
    (dest / "protocol.json").unlink()
    shutil.copy(protocol_file, dest / "protocol.json")
    return dest


def merge(out, parts, protocol):
    """Fold the shards' results into out/results, as one machine would have written them.

    parts: [(directory, exit code, error, vm)], each directory holding that shard's results/."""
    res = out / "results"
    res.mkdir(parents=True, exist_ok=True)
    shutil.copy(protocol.path, res / "protocol.json")
    trials, verify_failures, summary, log, shards = [], [], {}, [], []
    manifest, builds, artifacts, skipped = None, {}, {}, {}
    for k, (part, code, error, vm) in enumerate(parts, 1):
        r = part / "results"
        p = json.loads((r / "protocol.json").read_text()) if (r / "protocol.json").exists() else {}
        m = json.loads((r / "manifest.json").read_text()) if (r / "manifest.json").exists() else {}
        for name, rows in (("trials.csv", trials), ("verify.csv", verify_failures)):
            if (r / name).exists():
                with (r / name).open() as f:
                    rows.extend(csv.DictReader(f))
                (r / name).unlink()
        if (r / "summary.json").exists():
            summary.update(json.loads((r / "summary.json").read_text()))
            (r / "summary.json").unlink()
        for case, b in m.get("builds", {}).items():
            # Keep each case's clean build where a shard had one; that shard's artifact, log and source go with it.
            if case not in builds or (b.get("clean") and not builds[case].get("clean")):
                builds[case] = b
                if case in m.get("artifacts", {}):
                    artifacts[case] = m["artifacts"][case]
                for f in (f"build-{case}.log", f"generated-{case}.rs"):
                    if (r / f).exists():
                        shutil.copy(r / f, res / f)
        for case, rows in (m.get("verify") or {}).get("skipped_rows", {}).items():
            skipped[case] = sorted(set(skipped.get(case, [])) | set(rows))
        if (r / "execute.log").exists():
            log.append(f"== shard {k}/{len(parts)} on {vm or '?'}: {', '.join(s['name'] for s in p.get('scenarios', []))}\n")
            log.append((r / "execute.log").read_text())
        for sub in ("runtime", "verify"):
            if (r / sub).exists():
                (res / sub / f"shard-{k}").parent.mkdir(parents=True, exist_ok=True)
                shutil.move(str(r / sub), str(res / sub / f"shard-{k}"))
        status = m.get("status") or "shard_failed"
        shards.append({"shard": k, "vm": vm, "exit": code, "error": error, "status": status,
                       "scenarios": [s["name"] for s in p.get("scenarios", [])], "cases": p.get("cases", []),
                       **{f: m.get(f) for f in ("started", "finished", "cpu_model", "loadavg_start", "loadavg_end", "trials_total", "trials_failed")}})
        if manifest is None and m:
            manifest = dict(m)
    manifest = manifest or {}
    runner.write_csv(res / "trials.csv", trials, runner.FIELDS)
    runner.write_csv(res / "verify.csv", verify_failures, runner.FIELDS)
    if summary:
        (res / "summary.json").write_text(json.dumps(summary, indent=1))
    (res / "execute.log").write_text("".join(log))
    started = [s["started"] for s in shards if s["started"]]
    finished = [s["finished"] for s in shards if s["finished"]]
    manifest.update(builds=builds, artifacts=artifacts, shards=shards,
                    verify={"failures": len(verify_failures), "skipped_rows": skipped},
                    trials_total=sum(s["trials_total"] or 0 for s in shards), trials_failed=sum(s["trials_failed"] or 0 for s in shards),
                    started=min(started) if started else None, finished=max(finished) if len(finished) == len(shards) else None,
                    status=min((s["status"] for s in shards), key=lambda x: STATUS_ORDER.index(x) if x in STATUS_ORDER else 0))
    (res / "manifest.json").write_text(json.dumps(manifest, indent=1))
    # What is left of each shard (its manifest, log, protocol, machine.txt) stays beside the merge.
    for k, (part, *_rest) in enumerate(parts, 1):
        shutil.copytree(part, res / "shards" / str(k), dirs_exist_ok=True)
    return next((code for _, code, _, _ in parts if code), 0)

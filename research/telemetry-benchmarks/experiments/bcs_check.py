"""Check the billing replay against what hosted BCS stored.

For every hosted BAML run in results/*.json, compare the spans the replay
(lab/logical_usage.py) counted with the spans `baml query` finds for that run's
process ID. Also prints the project's total span count, which should equal the
Billing page's "Spans" figure.

    python experiments/bcs_check.py
"""
import glob
import json
import os
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "experiments"))
from searchable import BAML, BCS_PROJECT, S  # noqa: E402


def query(sql):
    env = {**os.environ, "BOUNDARY_API_KEY": S["BOUNDARY_QUERY_KEY"]}
    proc = subprocess.run([BAML, "--agent-skill-check", "off", "query", *BCS_PROJECT, "--format", "json", sql],
                          env=env, cwd=ROOT / "apps" / "baml", capture_output=True, text=True, timeout=300)
    if proc.returncode:
        raise RuntimeError(proc.stderr[-800:])
    return json.loads(proc.stdout)["rows"]


def hosted_runs():
    for f in sorted(glob.glob(str(ROOT / "results" / "*.json"))):
        try:
            rows = json.loads(Path(f).read_text())
        except Exception:
            continue
        if not isinstance(rows, list):
            continue
        for r in rows:
            if isinstance(r, dict) and r.get("hosted"):
                billing = (r.get("telemetry") or {}).get("billing") or {}
                if billing.get("process_ids"):
                    yield Path(f).name, r, billing


def main():
    total = query("select count(*) from spans")[0][0]
    print(f"project spans: {total}")
    stored = dict((pid, n) for pid, n in query("select process_id, count(*) from spans group by process_id"))
    out = []
    for name, r, billing in hosted_runs():
        pids = billing["process_ids"]
        found = sum(stored.get(p, 0) for p in pids)
        out.append({"file": name, "turns": r.get("turns"), "utc": r.get("utc"), "predicted_spans": billing.get("spans"),
                    "stored_spans": found, "predicted_bytes": billing.get("logical_bytes")})
        print(f"{name:32} turns={str(r.get('turns')):>4} predicted={billing.get('spans'):>5} stored={found:>5} "
              f"bytes={billing.get('logical_bytes')}")
    match = sum(1 for o in out if o["predicted_spans"] == o["stored_spans"])
    print(f"{match}/{len(out)} runs match exactly; predicted spans {sum(o['predicted_spans'] or 0 for o in out)}, "
          f"stored {sum(o['stored_spans'] for o in out)}")
    (ROOT / "results" / "bcs-check.json").write_text(json.dumps({"project_spans": total, "runs": out}, indent=1) + "\n")


if __name__ == "__main__":
    main()

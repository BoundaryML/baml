"""E9: how long after a call finishes can you find it in the vendor's UI/API?

Each trial runs one traced function with a unique marker in a fresh process,
flushes, and records the time the process exited. Then it polls the vendor's
query API every 0.5 s (Logfire) or 2 s (Datadog, whose search API rate-limits) until the marker appears (timeout 300 s).

- Logfire: Python + logfire SDK, polled with the SQL query API.
- Datadog: Python + ddtrace APM spans via the OTLP intake, polled with spans search.
- BCS: BAML CLI `Classify` call (against the loopback mock model) uploaded through a
  recording relay that learns the process ID, polled with `baml query` for spans of
  that process (needs a key with read access as BOUNDARY_QUERY_KEY in .env).

    python experiments/searchable.py --trials 5 --out results/searchable.json
"""
import argparse
import json
import os
import subprocess
import sys
import time
import uuid
from pathlib import Path

import requests

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "lab"))
sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "env" / "proto"))

ROOT = Path(__file__).resolve().parents[1]
S = dict(line.strip().split("=", 1) for line in open(ROOT / ".env") if "=" in line)
PY = str(ROOT / "env" / "py" / "bin" / "python")

LOGFIRE_APP = """
import logfire, sys
logfire.configure(service_name="bench-search", console=False, metrics=False)
with logfire.span("searchable", marker=sys.argv[1]):
    pass
logfire.force_flush()
"""

DATADOG_APP = """
import os, sys
os.environ.update(DD_SERVICE="bench-search", OTEL_EXPORTER_OTLP_TRACES_PROTOCOL="http/protobuf",
    OTEL_TRACES_EXPORTER="otlp", OTEL_EXPORTER_OTLP_TRACES_ENDPOINT="https://otlp.datadoghq.com/v1/traces",
    OTEL_EXPORTER_OTLP_TRACES_HEADERS="dd-api-key=" + os.environ["DD_API_KEY"],
    DD_REMOTE_CONFIGURATION_ENABLED="false", DD_INSTRUMENTATION_TELEMETRY_ENABLED="false")
from ddtrace.trace import tracer
with tracer.trace("searchable") as span:
    span.set_tag("marker", sys.argv[1])
tracer.flush()
"""


def logfire_found(marker):
    r = requests.get("https://logfire-us.pydantic.dev/v1/query", timeout=30,
                     headers={"Authorization": "Bearer " + S["LOGFIRE_TOKEN"], "Accept": "application/json"},
                     params={"sql": f"select count(*) as n from records where attributes->>'marker' = '{marker}'"})
    r.raise_for_status()
    return r.json()["columns"][0]["values"][0] > 0


def datadog_found(marker):
    r = requests.post("https://api.datadoghq.com/api/v2/spans/events/search", timeout=30,
                      headers={"DD-API-KEY": S["DD_API_KEY"], "DD-APPLICATION-KEY": S["DD_APP_KEY"],
                               "Content-Type": "application/json"},
                      json={"data": {"type": "search_request", "attributes": {
                          "filter": {"query": f"@marker:{marker}", "from": "now-15m", "to": "now"},
                          "page": {"limit": 1}}}})
    if r.status_code == 429:  # spans search is rate limited; treat as "not yet"
        time.sleep(float(r.headers.get("x-ratelimit-reset", 5)))
        return False
    r.raise_for_status()
    return len(r.json().get("data", [])) > 0


BAML = str(ROOT / "bin" / "baml")
BCS_PROJECT = ["--project", "aarons-prod-org1/test", "--environment", "Production"]


def bcs_found(process_id):
    env = {**os.environ, "BOUNDARY_API_KEY": S["BOUNDARY_QUERY_KEY"]}
    proc = subprocess.run([BAML, "--agent-skill-check", "off", "query", *BCS_PROJECT, "--format", "json",
                           f"select count(*) from spans where process_id = '{process_id}'"],
                          env=env, cwd=ROOT / "apps" / "baml", capture_output=True, text=True, timeout=120)
    if proc.returncode:
        raise RuntimeError(proc.stderr[-500:])
    return json.loads(proc.stdout)["rows"][0][0] > 0


def bcs_trial():
    from logical_usage import Meter
    from servers import bcs_relay, mock_llm
    model, meter = mock_llm(), Meter()
    sink = bcs_relay(S["BOUNDARY_API_KEY"], meter=meter)
    env = {"PATH": os.environ["PATH"], "HOME": os.environ["HOME"], "MODEL_BASE_URL": model.url + "/v1",
           "MODEL_API_KEY": "fixture", "BOUNDARY_API_KEY": sink.handle, "BOUNDARY_API_URL": sink.url}
    subprocess.run([BAML, "--agent-skill-check", "off", "run", "Classify", "--log", "off", "--", "--text",
                    "searchable " + uuid.uuid4().hex], env=env, cwd=ROOT / "apps" / "baml", check=True,
                   capture_output=True, timeout=60)
    sent = time.time()
    sink.shutdown()
    model.shutdown()
    (process_id,) = meter.summary()["process_ids"]
    polls = 0
    while time.time() - sent < 300:
        polls += 1
        if bcs_found(process_id):
            return {"vendor": "bcs", "marker": process_id, "seconds": time.time() - sent, "polls": polls}
        time.sleep(0.5)
    return {"vendor": "bcs", "marker": process_id, "seconds": None, "polls": polls}


def trial(vendor, marker):
    if vendor == "bcs":
        return bcs_trial()
    env = {**os.environ, "LOGFIRE_TOKEN": S["LOGFIRE_TOKEN"], "DD_API_KEY": S["DD_API_KEY"]}
    code, found = {"logfire": (LOGFIRE_APP, logfire_found), "datadog": (DATADOG_APP, datadog_found)}[vendor]
    subprocess.run([PY, "-c", code, marker], env=env, check=True, capture_output=True, timeout=60)
    sent = time.time()
    polls = 0
    while time.time() - sent < 300:
        polls += 1
        if found(marker):
            return {"vendor": vendor, "marker": marker, "seconds": time.time() - sent, "polls": polls}
        time.sleep(0.5 if vendor == "logfire" else 2.0)
    return {"vendor": vendor, "marker": marker, "seconds": None, "polls": polls}


def main():
    p = argparse.ArgumentParser()
    p.add_argument("--vendors", nargs="+", default=["logfire", "datadog", "bcs"])
    p.add_argument("--trials", type=int, default=5)
    p.add_argument("--out", required=True)
    args = p.parse_args()
    out = ROOT / args.out
    rows = json.loads(out.read_text()) if out.exists() else []
    for t in range(args.trials):
        for vendor in args.vendors:
            row = trial(vendor, "m" + uuid.uuid4().hex[:16])
            rows.append(row)
            print(row, flush=True)
            (ROOT / args.out).write_text(json.dumps(rows, indent=1) + "\n")


if __name__ == "__main__":
    main()

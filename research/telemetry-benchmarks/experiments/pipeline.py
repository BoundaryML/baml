"""E13: a non-LLM workload with many small functions.

Each request runs `Request`, which calls `Validate` and `Price` 20 times each
(41 function calls). Each stack traces every function the way its docs suggest:
BAML traces automatically, and the Python stacks put a decorator on every
function. Everything goes to loopback stand-ins, so this measures what each
stack would send and bill.

    python experiments/pipeline.py --requests 200 --out results/pipeline.json
"""
import argparse
import json
import os
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "lab"))
sys.path.insert(0, str(ROOT / "env" / "proto"))
from logical_usage import Meter  # noqa: E402
from servers import bcs_relay, relay  # noqa: E402

PY = str(ROOT / "env" / "py" / "bin" / "python")
BAML = str(ROOT / "bin" / "baml")

PY_APP = """
import os, sys
stack, requests, steps = sys.argv[1], int(sys.argv[2]), int(sys.argv[3])
url = os.environ["TELEMETRY_URL"]
if stack == "logfire":
    import logfire
    logfire.configure(service_name="pipe", console=False, metrics=False,
                      advanced=logfire.AdvancedOptions(base_url=url))
    traced = logfire.instrument()
    flush = logfire.force_flush
elif stack == "otel":
    from opentelemetry import trace
    from opentelemetry.exporter.otlp.proto.http import Compression
    from opentelemetry.exporter.otlp.proto.http.trace_exporter import OTLPSpanExporter
    from opentelemetry.sdk.trace import TracerProvider
    from opentelemetry.sdk.trace.export import BatchSpanProcessor
    provider = TracerProvider()
    provider.add_span_processor(BatchSpanProcessor(OTLPSpanExporter(endpoint=url + "/v1/traces", compression=Compression.Gzip)))
    trace.set_tracer_provider(provider)
    tracer = trace.get_tracer("pipe")
    def traced(fn):
        def wrapper(*args):
            with tracer.start_as_current_span(fn.__name__):
                return fn(*args)
        return wrapper
    flush = provider.force_flush
elif stack == "datadog":
    os.environ.update(OTEL_TRACES_EXPORTER="otlp", OTEL_EXPORTER_OTLP_TRACES_PROTOCOL="http/protobuf",
                      OTEL_EXPORTER_OTLP_TRACES_ENDPOINT=url + "/v1/traces", DD_SERVICE="pipe",
                      DD_REMOTE_CONFIGURATION_ENABLED="false", DD_INSTRUMENTATION_TELEMETRY_ENABLED="false")
    from ddtrace.trace import tracer
    traced = tracer.wrap()
    flush = tracer.flush

@traced
def validate(order):
    return order["total"] > 0

@traced
def price(order, step):
    return order["total"] * 1.01 + step

@traced
def request(i, steps):
    order = {"id": i, "total": 10.0, "status": "new"}
    total = 0.0
    for s in range(steps):
        if validate(order):
            total += price(order, s)
    return total

for i in range(requests):
    request(i, steps)
flush()
"""


def run(stack, requests, steps):
    env = {k: v for k, v in os.environ.items() if not k.startswith(("BOUNDARY_", "DD_", "LOGFIRE_", "OTEL_"))}
    meter = Meter()
    if stack.startswith("baml"):
        sink = bcs_relay(None, meter=meter)
        env.update(BOUNDARY_API_KEY=sink.handle, BOUNDARY_API_URL=sink.url,
                   BAML_TELEMETRY=stack.split("-")[1])
        cmd = [BAML, "--agent-skill-check", "off", "run", "Pipeline", "--log", "off", "--",
               "--requests", str(requests), "--steps", str(steps)]
        cwd = ROOT / "apps" / "baml_pipeline"
    else:
        sink = relay()
        env.update(TELEMETRY_URL=sink.url, LOGFIRE_TOKEN="pylf_v1_us_" + "0" * 40, DD_API_KEY="0" * 32)
        cmd = [PY, "-c", PY_APP, stack, str(requests), str(steps)]
        cwd = ROOT
    proc = subprocess.run(cmd, env=env, cwd=cwd, capture_output=True, text=True, timeout=600)
    sink.shutdown()
    if proc.returncode:
        raise RuntimeError(proc.stderr[-1000:])
    row = {"stack": stack, "requests": requests, "steps": steps,
           "function_calls": requests * (1 + 2 * steps),
           "bytes": sum(e["sent"] for e in sink.events)}
    if stack.startswith("baml"):
        s = meter.summary()
        row.update(spans=s["spans"], billed=s["logical_bytes"], events=s["events"])
    else:
        spans = sum(e.get("spans", 0) for e in sink.events)
        pending = sum(1 for e in sink.events for _, key, _ in e.get("fields", []) if key == "logfire.pending_parent_id")
        row.update(spans=spans, pending_spans=pending, stored_records=spans - pending)
    return row


def main():
    p = argparse.ArgumentParser()
    p.add_argument("--stacks", nargs="+", default=["baml-medium", "baml-high", "logfire", "otel", "datadog"])
    p.add_argument("--requests", type=int, default=200)
    p.add_argument("--steps", type=int, default=20)
    p.add_argument("--out", required=True)
    args = p.parse_args()
    rows = []
    for stack in args.stacks:
        rows.append(run(stack, args.requests, args.steps))
        print(rows[-1], flush=True)
    (ROOT / args.out).write_text(json.dumps(rows, indent=1) + "\n")


if __name__ == "__main__":
    main()

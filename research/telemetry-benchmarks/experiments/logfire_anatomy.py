"""E14: where Logfire's bytes come from, and why compression helps some runs far more than others.

Runs the Python classify app with Logfire against a local sink that keeps every
export body. Each body is gunzipped and decoded as OTLP, then split into
categories by exact protobuf size. The same decoded bodies are recompressed
with gzip and with lzma (an 8 MB window) to test the window explanation.

    python experiments/logfire_anatomy.py --out results/logfire-anatomy.json
"""
import argparse
import gzip
import json
import lzma
import os
import subprocess
import sys
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "lab"))
from corpus import document  # noqa: E402
from servers import mock_llm  # noqa: E402

PY = str(ROOT / "env" / "py" / "bin" / "python")
PROMPT_KEYS = {"text": "prompt as function argument", "gen_ai.input.messages": "prompt in model span"}


class Sink(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def log_message(self, *a):
        pass

    def do_GET(self):
        self._reply()

    def do_POST(self):
        body = self.rfile.read(int(self.headers.get("Content-Length") or 0))
        with self.server.lock:
            self.server.bodies.append((self.path, self.headers.get("Content-Encoding"), body))
        self._reply()

    def _reply(self):
        self.send_response(200)
        self.send_header("Content-Type", "application/x-protobuf")
        self.send_header("Content-Length", "0")
        self.end_headers()


def sink():
    s = ThreadingHTTPServer(("127.0.0.1", 0), Sink)
    s.daemon_threads, s.bodies, s.lock = True, [], threading.Lock()
    threading.Thread(target=s.serve_forever, daemon=True).start()
    s.url = f"http://127.0.0.1:{s.server_port}"
    return s


def texts(size, n, content, pattern, seed):
    if pattern == "independent":
        return [document(size, seed * 100 + k, content) for k in range(n)]
    base = document(size, seed * 100, content)
    if pattern == "suffix":  # how the first E1 run varied its documents
        return [f"{base} #{k}" for k in range(n)]
    return [base] * n


def run(size, n, content, pattern, seed):
    docs = texts(size, n, content, pattern, seed)
    model, tele = mock_llm(), sink()
    env = {**os.environ, "MODEL_BASE_URL": model.url + "/v1", "MODEL_API_KEY": "fixture",
           "TELEMETRY_URL": tele.url, "LOGFIRE_TOKEN": "pylf_v1_us_" + "0" * 40}
    cmd = [PY, str(ROOT / "apps" / "py_apps.py"), "logfire", "classify", "--text", "|".join(docs),
           "--n", str(n), "--vary"]
    try:
        proc = subprocess.run(cmd, env=env, capture_output=True, text=True, timeout=300)
        if proc.returncode:
            raise RuntimeError(proc.stderr[-1500:])
        time.sleep(0.2)
        bodies = [b for b in tele.bodies if b[0].endswith("/v1/traces")]
        return analyze(bodies, docs, model.requests)
    finally:
        model.shutdown()
        tele.shutdown()


def analyze(bodies, docs, model_requests):
    from opentelemetry.proto.collector.trace.v1.trace_service_pb2 import ExportTraceServiceRequest
    cat = {"prompt as function argument": 0, "prompt in model span": 0, "other attributes": 0,
           "span ids, names and times": 0, "resource and batch envelope": 0}
    sent = decoded = regz = relz = 0
    spans = pending = pending_bytes = 0
    order, gaps = [], []
    probes = [d[:120].encode() for d in docs]
    for _, encoding, body in bodies:
        plain = gzip.decompress(body) if encoding == "gzip" else body
        sent += len(body)
        decoded += len(plain)
        regz += len(gzip.compress(plain, 6))
        relz += len(lzma.compress(plain, preset=6))
        req = ExportTraceServiceRequest.FromString(plain)
        in_spans = 0
        for rs in req.resource_spans:
            for ss in rs.scope_spans:
                for span in ss.spans:
                    spans += 1
                    size = span.ByteSize()
                    in_spans += size
                    attrs = 0
                    kind = None
                    for kv in span.attributes:
                        b = kv.ByteSize()
                        attrs += b
                        label = PROMPT_KEYS.get(kv.key)
                        cat[label or "other attributes"] += b
                        if kv.key == "logfire.span_type":
                            kind = kv.value.string_value
                    cat["span ids, names and times"] += size - attrs
                    if kind == "pending_span":
                        pending += 1
                        pending_bytes += size
                    order.append({"scope": ss.scope.name, "name": span.name[:40], "pending": kind == "pending_span"})
        cat["resource and batch envelope"] += len(plain) - in_spans
        # Distance between consecutive copies of the same document inside this request.
        for probe in probes:
            at, start = [], 0
            while (j := plain.find(probe, start)) >= 0:
                at.append(j)
                start = j + 1
            gaps += [b - a for a, b in zip(at, at[1:])]
    n = len(docs)
    return {
        "calls": n, "doc_bytes": len(docs[0]), "requests": len(bodies), "spans": spans,
        "pending_spans": pending, "pending_bytes": pending_bytes,
        "sent": sent, "decoded": decoded, "regzip": regz, "lzma": relz,
        "doc_gzip_alone": sum(len(gzip.compress(d.encode(), 6)) for d in docs),
        "categories": cat, "copy_gaps": sorted(gaps), "order": order[:60],
        "model_sent": sum(r["sent"] for r in model_requests),
    }


def main():
    p = argparse.ArgumentParser()
    p.add_argument("--sizes", nargs="+", type=int, default=[1024, 4096, 10240, 32768, 102400])
    p.add_argument("--contents", nargs="+", default=["random", "english"])
    p.add_argument("--patterns", nargs="+", default=["independent", "suffix", "identical"])
    p.add_argument("--n", type=int, default=10)
    p.add_argument("--seed", type=int, default=7)
    p.add_argument("--out", required=True)
    args = p.parse_args()
    rows = []
    plan = [(c, pat, s) for c in args.contents for pat in args.patterns for s in args.sizes]
    for i, (content, pattern, size) in enumerate(plan):
        row = {"content": content, "pattern": pattern, "size": size,
               **run(size, args.n, content, pattern, args.seed + i)}
        rows.append(row)
        print(f"[{i+1}/{len(plan)}] {content:7} {pattern:11} {size:>6}  sent/call={row['sent']/args.n:9.0f}  "
              f"decoded/call={row['decoded']/args.n:9.0f}  lzma/call={row['lzma']/args.n:9.0f}  "
              f"spans={row['spans']} pending={row['pending_spans']} gaps~{row['copy_gaps'][len(row['copy_gaps'])//2] if row['copy_gaps'] else None}",
              flush=True)
        (ROOT / args.out).write_text(json.dumps(rows, indent=1) + "\n")


if __name__ == "__main__":
    main()

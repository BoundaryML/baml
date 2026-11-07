"""Loopback servers shared by every experiment.

MockLLM answers OpenAI Chat Completions requests deterministically, so every
stack sends and receives the same model traffic.

Relay sits between an SDK and its vendor. It records the exact HTTP request
body bytes each SDK sends, decodes a copy to count spans and payload fields,
and either forwards the original bytes to the vendor (hosted mode) or answers
with an empty success (local mode). It never stores keys or signed URLs.
"""
import gzip
import http.client
import json
import re
import threading
import time
import uuid
import zlib
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import urlsplit

BCS_GATEWAY = "https://api.prod.bcs.boundaryml.com"
LOGFIRE_INTAKE = "https://logfire-us.pydantic.dev"
DATADOG_OTLP = "https://otlp.datadoghq.com"
DATADOG_LLMOBS = "https://llmobs-intake.datadoghq.com"


def upstream(url, method, body, headers, timeout=60):
    parts = urlsplit(url)
    if parts.scheme != "https":
        raise ValueError("refusing non-HTTPS upstream")
    conn = http.client.HTTPSConnection(parts.hostname, parts.port or 443, timeout=timeout)
    try:
        target = parts.path + ("?" + parts.query if parts.query else "")
        conn.request(method, target, body=body, headers=headers)
        response = conn.getresponse()
        return response.status, response.read(), response.getheader("content-type", "application/octet-stream")
    finally:
        conn.close()


def serve(handler, **attrs):
    server = ThreadingHTTPServer(("127.0.0.1", 0), handler)
    server.daemon_threads = True
    for key, value in attrs.items():
        setattr(server, key, value)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    server.url = f"http://127.0.0.1:{server.server_port}"
    return server


class _Quiet(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def log_message(self, *args):
        pass

    def body(self):
        if self.headers.get("Transfer-Encoding", "").lower() == "chunked":
            chunks = []
            while True:
                size = int(self.rfile.readline().strip(), 16)
                if size == 0:
                    self.rfile.readline()
                    break
                chunks.append(self.rfile.read(size))
                self.rfile.readline()
            return b"".join(chunks)
        return self.rfile.read(int(self.headers.get("Content-Length") or 0))

    def reply(self, status, data=b"", content_type="application/json"):
        self.send_response(status)
        self.send_header("Content-Type", content_type)
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)


# ---------------------------------------------------------------- mock model

class MockLLM(_Quiet):
    """OpenAI Chat Completions. The reply is chosen by a rule on the last user message.

    server.script: list of replies consumed in order, or None to use server.rule.
    A reply of "503" returns HTTP 503. server.delay_s simulates model latency.
    """

    def do_POST(self):
        raw = self.body()
        request = json.loads(raw)
        prompt = "\n".join(str(m.get("content")) for m in request.get("messages", []))
        with self.server.lock:
            index = len(self.server.requests)
            if self.server.script is not None:
                answer = self.server.script[index % len(self.server.script)]
            else:
                answer = self.server.rule(prompt)
        if self.server.delay_s:
            time.sleep(self.server.delay_s)
        if answer == "503":
            data = json.dumps({"error": {"message": "synthetic_busy", "type": "server_error"}}).encode()
            status = 503
        else:
            data = json.dumps({
                "id": f"chatcmpl-{index}", "object": "chat.completion", "created": 1794000000,
                "model": request.get("model", "mock"),
                "choices": [{"index": 0, "finish_reason": "stop",
                             "message": {"role": "assistant", "content": answer}}],
                "usage": {"prompt_tokens": max(1, len(prompt) // 4), "completion_tokens": max(1, len(answer) // 4),
                          "total_tokens": max(1, len(prompt) // 4) + max(1, len(answer) // 4)},
            }, separators=(",", ":")).encode()
            status = 200
        with self.server.lock:
            self.server.requests.append({"path": self.path, "sent": len(raw), "received": len(data), "status": status})
        self.reply(status, data)


def default_rule(prompt):
    if "Reply with only true or false" in prompt:
        return "true"
    if "Extract the invoice" in prompt:
        return '{"vendor":"Acme Corp","total":1234.5,"currency":"USD","paid":false}'
    if "Choose the SKU" in prompt:
        return '{"sku":"ACME-1"}'
    if "Units available" in prompt:
        return "There are seven left."
    return "ok"


def mock_llm(script=None, rule=default_rule, delay_s=0.0):
    return serve(MockLLM, script=script, rule=rule, delay_s=delay_s, requests=[], lock=threading.Lock())


# ---------------------------------------------------------------- OTLP / JSON relay

def _inflate(data, encoding):
    encoding = (encoding or "").lower()
    if encoding == "gzip":
        return gzip.decompress(data)
    if encoding == "deflate":
        return zlib.decompress(data)
    return data


def decode_otlp(raw):
    from opentelemetry.proto.collector.trace.v1.trace_service_pb2 import ExportTraceServiceRequest
    req = ExportTraceServiceRequest.FromString(raw)
    spans, attr_bytes, values = 0, 0, []
    for rs in req.resource_spans:
        for ss in rs.scope_spans:
            for span in ss.spans:
                spans += 1
                for kv in span.attributes:
                    v = kv.value
                    text = v.string_value if v.HasField("string_value") else str(v)
                    attr_bytes += len(kv.key) + len(text.encode())
                    values.append((span.name, kv.key, len(text.encode())))
    return {"spans": spans, "attr_bytes": attr_bytes, "fields": values}


class Relay(_Quiet):
    """Record and optionally forward OTLP (protobuf) or JSON telemetry requests."""

    def do_POST(self):
        raw = self.body()
        encoding = self.headers.get("Content-Encoding")
        row = {"path": self.path, "sent": len(raw), "encoding": encoding or "identity",
               "content_type": self.headers.get("Content-Type"), "t": time.time()}
        try:
            plain = _inflate(raw, encoding)
            row["decoded"] = len(plain)
            if "protobuf" in (row["content_type"] or ""):
                row.update(decode_otlp(plain))
            elif "json" in (row["content_type"] or ""):
                row["json_events"] = _count_json_events(plain)
        except Exception as exc:  # keep the body count even when decoding fails
            row["decode_error"] = repr(exc)[:200]
        status, reply, ctype = 200, b"", "application/x-protobuf"
        if self.server.upstream:
            headers = {k: v for k, v in self.headers.items()
                       if k.lower() in ("content-type", "content-encoding", "authorization", "dd-api-key", "dd-application-key")}
            headers.update(self.server.auth_headers)
            status, reply, ctype = upstream(self.server.upstream + self.path, "POST", raw, headers)
            row["upstream_status"] = status
            row["upstream_reply"] = reply[:300].decode(errors="replace") if status >= 300 else ""
        elif "json" in (row["content_type"] or ""):
            reply, ctype = b"{}", "application/json"
        with self.server.lock:
            self.server.events.append(row)
        self.reply(status if status < 300 else status, reply, ctype)

    do_PUT = do_POST

    def do_GET(self):
        # Logfire's SDK checks its token with a GET at startup.
        if not self.server.upstream:
            return self.reply(200, b"{}")
        headers = {k: v for k, v in self.headers.items() if k.lower() in ("authorization", "dd-api-key")}
        headers.update(self.server.auth_headers)
        status, reply, ctype = upstream(self.server.upstream + self.path, "GET", None, headers)
        self.reply(status, reply, ctype)


def _count_json_events(plain):
    try:
        doc = json.loads(plain)
    except Exception:
        return None
    if isinstance(doc, list):
        return sum(len(d.get("spans", [])) if isinstance(d, dict) else 1 for d in doc)
    if isinstance(doc, dict):
        if "spans" in doc:
            return len(doc["spans"])
        data = doc.get("data")
        if isinstance(data, dict):
            return len(data.get("attributes", {}).get("spans", []) or [])
    return None


def relay(upstream_url=None, auth_headers=None):
    return serve(Relay, upstream=upstream_url, auth_headers=auth_headers or {}, events=[], lock=threading.Lock())


# ---------------------------------------------------------------- BCS relay

class BcsRelay(_Quiet):
    """Forward the BAML exporter's heartbeat, prepare and PUT calls to hosted BCS.

    The program under test holds a placeholder key; only this process holds the
    real one. Signed upload URLs stay in memory behind opaque loopback paths.
    When server.key is None the relay acts as a local sink: prepare asks for
    every object and PUT bodies are counted and dropped.
    """

    def do_POST(self):
        if self.headers.get("Authorization") != "Bearer " + self.server.handle:
            return self.reply(401)
        match = re.fullmatch(r"/v1/recordings/([a-fA-F0-9-]+)/(heartbeat|uploads:prepare)", self.path)
        if not match:
            return self.reply(404)
        recording, operation = match.groups()
        body = self.body()
        if self.server.key:
            headers = {"Authorization": "Bearer " + self.server.key, "Content-Type": "application/json"}
            if "Idempotency-Key" in self.headers:
                headers["Idempotency-Key"] = self.headers["Idempotency-Key"]
            status, reply, ctype = upstream(BCS_GATEWAY + self.path, "POST", body, headers)
        else:
            status, ctype = (204, "application/json") if operation == "heartbeat" else (200, "application/json")
            reply = b"" if operation == "heartbeat" else self._local_prepare(body)
        row = dict(recording=recording, operation=operation, sent=len(body), received=len(reply),
                   status=status, t=time.time())
        if operation == "uploads:prepare" and status == 200:
            doc = json.loads(reply)
            for up in doc.get("uploads", []):
                handle = "/put/" + uuid.uuid4().hex
                self.server.urls[handle] = (up.get("presigned_put_url"), up.get("required_headers", {}), recording)
                up["presigned_put_url"] = f"{self.server.url}{handle}"
            reply = json.dumps(doc).encode()
            row["already_available"] = sum(1 for c in doc.get("cas", []) if c["disposition"]["kind"] == "already_available")
        with self.server.lock:
            self.server.events.append(row)
        self.reply(status, reply, ctype)

    def _local_prepare(self, body):
        """Answer a prepare the way BCS does when it has none of the objects (bcs_api/src/wire.rs)."""
        req = json.loads(body)
        kinds = {"recording": "inline_with_recording", "cas_batch": "member_of_batch", "cas_object": "separate_object"}
        uploads, cas = [], []
        for target in req["proposed_uploads"]:
            upload_id = uuid.uuid4().hex
            uploads.append({"upload_id": upload_id, "client_target_id": target["client_target_id"],
                            "object_key": upload_id, "presigned_put_url": "https://local.invalid/" + upload_id,
                            "expires_at_unix_ms": int(time.time() * 1000) + 3_600_000, "required_headers": {},
                            "kind": target["kind"], "candidate_indices": target["candidate_indices"]})
            for index in target["candidate_indices"]:
                cas.append({"candidate_index": index, "disposition": {"kind": kinds[target["kind"]], "upload_id": upload_id}})
        return json.dumps({"plan_id": uuid.uuid4().hex, "expires_at_unix_ms": int(time.time() * 1000) + 3_600_000,
                           "uploads": uploads, "cas": cas}).encode()

    def do_PUT(self):
        target = self.server.urls.get(self.path)
        if not target:
            return self.reply(404)
        url, headers, recording = target
        body = self.body()
        if self.server.key:
            status, reply, ctype = upstream(url, "PUT", body, headers)
        else:
            status, reply, ctype = 200, b"", "application/octet-stream"
        if 200 <= status < 300 and self.server.meter is not None:
            self.server.meter.observe(body)
        with self.server.lock:
            self.server.events.append(dict(recording=recording, operation="upload", sent=len(body),
                                           received=len(reply), status=status, t=time.time()))
        self.reply(status, reply, ctype)


def bcs_relay(key, meter=None):
    return serve(BcsRelay, key=key, handle="bdry_secret_" + uuid.uuid4().hex, urls={}, events=[],
                 meter=meter, lock=threading.Lock())

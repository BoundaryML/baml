#!/usr/bin/env python3
"""Run the native telemetry resource matrix with an out-of-process HTTP sink."""

import argparse
import ctypes
import datetime
import errno
import json
import multiprocessing
import os
from pathlib import Path
import platform
import random
import selectors
import socket
import subprocess
import sys
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.request import urlopen


ROOT = Path(__file__).resolve().parents[1]
MODES = ("off", "auto-no-sink", "local", "cloud-fast", "cloud-slow")
WORKLOADS = {
    "tiny": ("tiny.baml", 1, 0, 1, "roots"),
    "calls": ("calls.baml", 100000, 0, 100000, "calls"),
    "spawn": ("burst.baml", 64, 0, 2048, "children"),
    "async": ("dense.baml", 4, 0, 128, "children"),
    "burst": ("burst.baml", 4, 200, 128, "children"),
    "cold": ("tiny.baml", 1, 0, 1, "roots"),
}


class Usage(ctypes.Structure):
    _fields_ = [("uuid", ctypes.c_ubyte * 16)] + [
        (name, ctypes.c_uint64)
        for name in (
            "user", "system", "idle", "interrupts", "pageins", "wired",
            "resident", "footprint", "start", "exit", "child_user",
            "child_system", "child_idle", "child_interrupts", "child_pageins",
            "child_elapsed", "read", "write",
        )
    ]


class Timebase(ctypes.Structure):
    _fields_ = [("numer", ctypes.c_uint32), ("denom", ctypes.c_uint32)]


class MacSampler:
    def __init__(self):
        if sys.platform != "darwin":
            raise RuntimeError("This probe matches the report's macOS proc_pid_rusage sampler.")
        self.lib = ctypes.CDLL("/usr/lib/libproc.dylib", use_errno=True)
        self.lib.proc_pid_rusage.argtypes = [ctypes.c_int, ctypes.c_int, ctypes.c_void_p]
        self.lib.proc_pid_rusage.restype = ctypes.c_int
        system = ctypes.CDLL("/usr/lib/libSystem.B.dylib")
        system.mach_timebase_info.argtypes = [ctypes.POINTER(Timebase)]
        system.mach_timebase_info.restype = ctypes.c_int
        self.timebase = Timebase()
        if system.mach_timebase_info(ctypes.byref(self.timebase)) != 0:
            raise RuntimeError("Cannot read Mach timebase")
        self.seconds_per_tick = self.timebase.numer / self.timebase.denom / 1e9

    def read(self, pid):
        usage = Usage()
        if self.lib.proc_pid_rusage(pid, 2, ctypes.byref(usage)) != 0:
            code = ctypes.get_errno()
            if code in (errno.ESRCH, errno.ENOENT):
                return None
            raise OSError(code, os.strerror(code))
        return {
            "cpu_s": (usage.user + usage.system) * self.seconds_per_tick,
            "rss_bytes": usage.resident,
            "read_bytes": usage.read,
            "write_bytes": usage.write,
        }

    def calibrate(self):
        before = self.read(os.getpid())["cpu_s"]
        start = time.process_time()
        while time.process_time() - start < 0.1:
            pass
        measured = time.process_time() - start
        sampled = self.read(os.getpid())["cpu_s"] - before
        if not 0.95 <= sampled / measured <= 1.05:
            raise RuntimeError("Mach CPU conversion failed process_time calibration")
        return {"process_time_s": measured, "libproc_cpu_s": sampled}


def serve_mock(pipe, delay_ms):
    lock = threading.Lock()
    stats = {
        "prepare_requests": 0, "offered_candidates": 0,
        "put_requests": 0, "acknowledged_puts": 0,
        "received_bytes": 0, "acknowledged_bytes": 0,
    }

    class Handler(BaseHTTPRequestHandler):
        protocol_version = "HTTP/1.1"

        def setup(self):
            super().setup()
            self.connection.setsockopt(socket.IPPROTO_TCP, socket.TCP_NODELAY, 1)

        def log_message(self, *_args):
            pass

        def send_json(self, value):
            body = json.dumps(value).encode()
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)

        def length(self, limit):
            length = int(self.headers.get("Content-Length", "-1"))
            if length < 0 or length > limit:
                self.send_error(413)
                self.close_connection = True
                return None
            return length

        def do_GET(self):
            if self.path != "/stats":
                self.send_error(404)
                return
            with lock:
                snapshot = dict(stats)
            self.send_json(snapshot)

        def do_POST(self):
            if not self.path.endswith("/uploads:prepare"):
                self.send_error(404)
                return
            length = self.length(64 * 1024)
            if length is None:
                return
            request = json.loads(self.rfile.read(length))
            with lock:
                stats["prepare_requests"] += 1
                stats["offered_candidates"] += len(request["candidates"])
            recording = request["recording"]
            sequence = recording["recording_file_sequence"]
            plan_id = f'{recording["recording_id"]}-{sequence}'
            expiry = int(time.time() * 1000) + 3_600_000
            uploads, cas = [], []
            kinds = {
                "recording": "inline_with_recording",
                "cas_batch": "member_of_batch",
                "cas_object": "separate_object",
            }
            for target in request["proposed_uploads"]:
                upload_id = f'{plan_id}-{target["client_target_id"]}'
                members = target["candidate_indices"]
                uploads.append({
                    "upload_id": upload_id,
                    "client_target_id": target["client_target_id"],
                    "object_key": f"objects/{upload_id}",
                    "presigned_put_url": f"http://127.0.0.1:{self.server.server_port}/objects/{upload_id}",
                    "expires_at_unix_ms": expiry,
                    "required_headers": {"content-type": "application/x-protobuf"},
                    "kind": target["kind"],
                    "candidate_indices": members,
                })
                cas.extend({
                    "candidate_index": index,
                    "disposition": {"kind": kinds[target["kind"]], "upload_id": upload_id},
                } for index in members)
            self.send_json({
                "plan_id": plan_id, "expires_at_unix_ms": expiry,
                "uploads": uploads, "cas": cas,
            })

        def do_PUT(self):
            if not self.path.startswith("/objects/"):
                self.send_error(404)
                return
            length = self.length(16 * 1024 * 1024)
            if length is None:
                return
            remaining = length
            while remaining:
                chunk = self.rfile.read(min(remaining, 64 * 1024))
                if not chunk:
                    self.close_connection = True
                    return
                remaining -= len(chunk)
            with lock:
                stats["put_requests"] += 1
                stats["received_bytes"] += length
            time.sleep(delay_ms / 1000)
            self.send_response(200)
            self.send_header("Content-Length", "0")
            self.end_headers()
            with lock:
                stats["acknowledged_puts"] += 1
                stats["acknowledged_bytes"] += length

    server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    server.daemon_threads = True
    pipe.send(server.server_port)
    pipe.close()
    server.serve_forever()


class MockCloud:
    def __init__(self, delay_ms):
        context = multiprocessing.get_context("spawn")
        receiver, sender = context.Pipe(duplex=False)
        self.process = context.Process(target=serve_mock, args=(sender, delay_ms), daemon=True)
        self.process.start()
        sender.close()
        try:
            if not receiver.poll(10):
                raise RuntimeError("Mock HTTP server did not start")
            self.url = f"http://127.0.0.1:{receiver.recv()}"
        except BaseException:
            self.close()
            raise
        finally:
            receiver.close()

    def stats(self):
        with urlopen(self.url + "/stats", timeout=5) as response:
            return json.load(response)

    def close(self):
        if self.process.is_alive():
            self.process.terminate()
        self.process.join(timeout=5)
        if self.process.is_alive():
            self.process.kill()
            self.process.join()


def monitor(command, env, sampler, sample_ms, memory_limit, timeout_s, run_dir):
    started = time.monotonic()
    samples, stages, events = [], {}, []
    phase = "launch"
    error = None
    result = None
    process = subprocess.Popen(command, env=env, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    selector = selectors.DefaultSelector()
    buffers = {"stdout": b"", "stderr": b""}
    for name, stream in (("stdout", process.stdout), ("stderr", process.stderr)):
        os.set_blocking(stream.fileno(), False)
        selector.register(stream, selectors.EVENT_READ, name)
    next_sample = started

    def sample():
        nonlocal error
        reading = sampler.read(process.pid)
        if reading is None:
            return None
        reading.update(elapsed_s=time.monotonic() - started, phase=phase)
        samples.append(reading)
        if reading["rss_bytes"] > memory_limit and error is None:
            error = "observed RSS exceeded memory limit"
            process.kill()
        return reading

    try:
        with (run_dir / "stdout.jsonl").open("wb") as stdout, (run_dir / "stderr.log").open("wb") as stderr:
            while selector.get_map():
                now = time.monotonic()
                if now - started > timeout_s and error is None:
                    error = "runner timeout"
                    process.kill()
                if now >= next_sample:
                    sample()
                    next_sample = now + sample_ms / 1000
                for key, _ in selector.select(max(0, next_sample - time.monotonic())):
                    data = os.read(key.fileobj.fileno(), 65536)
                    if not data:
                        selector.unregister(key.fileobj)
                        continue
                    name = key.data
                    (stdout if name == "stdout" else stderr).write(data)
                    buffers[name] += data
                    while b"\n" in buffers[name]:
                        line, buffers[name] = buffers[name].split(b"\n", 1)
                        if name != "stdout":
                            continue
                        try:
                            event = json.loads(line)
                        except (ValueError, UnicodeDecodeError):
                            continue
                        events.append(event)
                        kind = event.get("event", event.get("type"))
                        if kind == "phase":
                            phase = event["phase"]
                            reading = sample()
                            if reading is not None:
                                stages[phase] = reading
                        elif kind == "result":
                            result = event
            sample()
        code = process.wait(timeout=5)
        if code and error is None:
            error = f"runner exited with code {code}"
    finally:
        selector.close()
        if process.poll() is None:
            process.kill()
            process.wait()
        process.stdout.close()
        process.stderr.close()
    return result, samples, stages, events, error


def run_case(args, sampler, workload, mode, trial, artifact, index):
    source, n, idle_ms, units, work_unit = WORKLOADS[workload]
    run_dir = args.output / "runs" / f"{index:03}-{workload}-{mode}-{trial}"
    run_dir.mkdir(parents=True)
    cloud = MockCloud(args.slow_put_ms if mode == "cloud-slow" else 0) if mode.startswith("cloud") else None
    duration_ms = 0 if workload == "cold" else round(args.duration_s * 1000)
    command = [
        str(args.binary), "run", "--artifact", str(artifact),
        "--mode", mode, "--n", str(n), "--duration-ms", str(duration_ms),
        "--idle-ms", str(idle_ms), "--units-per-root", str(units),
        "--work-unit", work_unit, "--output-dir", str(run_dir / "files"),
        "--workers", str(args.workers),
    ]
    if cloud:
        command += ["--prepare-base-url", cloud.url]
    env = os.environ.copy()
    env["BAML_TELEMETRY"] = "off" if mode == "off" else "medium"
    try:
        result, samples, stages, events, error = monitor(
            command, env, sampler, args.sample_ms, args.memory_limit_mib * 1024**2,
            max(120, args.duration_s * 6), run_dir,
        )
        cloud_stats = cloud.stats() if cloud else {}
    finally:
        if cloud:
            cloud.close()
    result = result or {}
    success = result.get("status") == "ok" and error is None
    telemetry = result.get("telemetry_result")
    if isinstance(telemetry, dict) and "Err" in telemetry:
        success = False
        error = error or telemetry["Err"]
    status = result.get("telemetry_status")
    if status not in (None, "off", "no-sink", "ok"):
        success = False
        error = error or status
    cpu = None
    if "execute" in stages and "drain" in stages and result.get("execute_ms", 0) > 0:
        cpu = (stages["drain"]["cpu_s"] - stages["execute"]["cpu_s"]) / (result["execute_ms"] / 1000) * 100
    row = {
        "workload": workload, "mode": mode, "trial": trial,
        "cpu_percent": cpu,
        "peak_rss_bytes": max((s["rss_bytes"] for s in samples), default=0),
        "read_bytes": max((s["read_bytes"] for s in samples), default=0),
        "write_bytes": max((s["write_bytes"] for s in samples), default=0),
        "output_bytes": cloud_stats.get("acknowledged_bytes", 0) if cloud else result.get("recording_bytes", 0) + result.get("cas_bytes", 0),
        "telemetry_success": success,
        "error": error or (None if success else "missing/failed native result"),
        "native_result": result, "samples": samples, "stage_samples": stages,
        "phase_events": events, "cloud_stats": cloud_stats,
        "mock_server_pid": cloud.process.pid if cloud else None,
    }
    for key in ("drain_ms", "execute_ms", "load_ms", "setup_ms", "work_units", "invocations", "work_unit"):
        row[key] = result.get(key)
    row["work_per_s"] = result.get("throughput_per_s")
    if row["work_per_s"] is None and result.get("execute_ms", 0) > 0:
        row["work_per_s"] = result["work_units"] / (result["execute_ms"] / 1000)
    (run_dir / "result.json").write_text(json.dumps(row, indent=2))
    return row


def sysctl(name):
    return subprocess.check_output(["/usr/sbin/sysctl", "-n", name], text=True).strip()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=ROOT / "target/release/examples/resource_benchmark")
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--reference", type=Path)
    parser.add_argument("--duration-s", type=float, default=10)
    parser.add_argument("--repeats", type=int, default=2)
    parser.add_argument("--workers", type=int, default=os.cpu_count())
    parser.add_argument("--sample-ms", type=int, default=100)
    parser.add_argument("--memory-limit-mib", type=int, default=1024)
    parser.add_argument("--slow-put-ms", type=int, default=50)
    parser.add_argument("--seed", type=int, default=4958)
    parser.add_argument("--workloads", nargs="+", choices=WORKLOADS, default=list(WORKLOADS))
    parser.add_argument("--modes", nargs="+", choices=MODES, default=list(MODES))
    args = parser.parse_args()
    if min(args.duration_s, args.repeats, args.workers, args.sample_ms, args.memory_limit_mib) <= 0:
        parser.error("duration, repeats, workers, sampling interval and memory limit must be positive")
    if args.slow_put_ms < 0:
        parser.error("slow PUT delay must be nonnegative")
    args.binary = args.binary.resolve()
    args.output = args.output.resolve()
    if not args.binary.is_file():
        parser.error("build the release resource_benchmark example first")
    args.output.mkdir(parents=True, exist_ok=False)
    artifacts = args.output / "artifacts"
    artifacts.mkdir()
    sampler = MacSampler()
    metadata = {
        "machine": sysctl("machdep.cpu.brand_string"),
        "memory_bytes": int(sysctl("hw.memsize")),
        "physical_cores": int(sysctl("hw.physicalcpu")),
        "logical_cores": int(sysctl("hw.logicalcpu")),
        "platform": platform.platform(),
        "power_source": subprocess.check_output(["/usr/bin/pmset", "-g", "batt"], text=True).strip(),
        "compiler": subprocess.check_output(["rustc", "--version"], cwd=ROOT, text=True).strip(),
        "duration_s": args.duration_s, "repeats": args.repeats,
        "sample_ms": args.sample_ms, "memory_limit_bytes": args.memory_limit_mib * 1024**2,
        "workers": args.workers, "seed": args.seed, "slow_put_ms": args.slow_put_ms,
        "source_revision": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
        "started_at": datetime.datetime.now(datetime.timezone.utc).isoformat(),
        "cpu_calibration": sampler.calibrate(),
        "mach_timebase": {"numer": sampler.timebase.numer, "denom": sampler.timebase.denom},
    }
    (args.output / "metadata.json").write_text(json.dumps(metadata, indent=2))
    compiled = {}
    for workload in args.workloads:
        source = WORKLOADS[workload][0]
        if source in compiled:
            continue
        artifact = artifacts / (Path(source).stem + ".borsh")
        subprocess.run([
            str(args.binary), "compile", "--source",
            str(ROOT / "crates/bex_engine/examples/resource_workloads" / source),
            "--artifact", str(artifact),
        ], check=True)
        compiled[source] = artifact
    cases = [
        (workload, mode, trial)
        for workload in args.workloads
        for mode in args.modes
        for trial in range(1 if workload == "cold" else args.repeats)
    ]
    random.Random(args.seed).shuffle(cases)
    runs = []
    try:
        with (args.output / "runs.jsonl").open("w") as output:
            for index, (workload, mode, trial) in enumerate(cases):
                row = run_case(args, sampler, workload, mode, trial, compiled[WORKLOADS[workload][0]], index)
                runs.append(row)
                output.write(json.dumps(row) + "\n")
                output.flush()
                print(json.dumps({key: row[key] for key in (
                    "workload", "mode", "trial", "cpu_percent", "peak_rss_bytes",
                    "work_per_s", "output_bytes", "drain_ms", "telemetry_success", "error",
                )}), flush=True)
    finally:
        from telemetry_resource_report import render_report
        render_report(runs, metadata, args.output, args.reference)
    print(f"Report: {args.output / 'report.html'}")
    if any(not row["telemetry_success"] for row in runs):
        raise SystemExit("One or more runs failed; excluded from comparisons")


if __name__ == "__main__":
    main()

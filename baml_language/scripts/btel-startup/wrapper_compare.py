#!/usr/bin/env python3
"""Matched macOS Rust-vs-packed wrapper measurements with synthetic fixtures.

Build both revisions with the normal release profile before running. Timings
should run while builds/tests are idle. Output contains raw samples and summaries.
No production credentials or network endpoints are used."""

import argparse
import ctypes
import gzip
import hashlib
import http.server
import json
import os
import pathlib
import random
import select
import shutil
import statistics
import subprocess
import threading
import time

SOURCE = pathlib.Path(__file__).resolve().parent
CONFIG = '# benchmark config\n[default]\nselector = "0.11.0"\n[update]\nauto_check = false\n[extra]\nvalue = 42\n'
PROJECT_CONFIG = '# benchmark project\n[package]\nname = "benchmark"\n[toolchain]\nversion = "0.11.0"\n'


def setup():
    ROOT.mkdir(parents=True, exist_ok=True)
    for name in ("measure", "child"):
        subprocess.run(
            [
                "/usr/bin/clang",
                "-O2",
                str(SOURCE / f"{name}.c"),
                "-o",
                str(ROOT / name),
            ],
            check=True,
        )
    tiny = ROOT / "tiny-project"
    tiny.mkdir(exist_ok=True)
    (tiny / "baml.toml").write_text('[package]\nname = "wrapper_metrics"\n')
    (tiny / "main.baml").write_text("function Main() -> void {}\n")
    PROJECT.mkdir(parents=True, exist_ok=True)
    BAML_HOME.mkdir(parents=True, exist_ok=True)
    for i in range(10):
        folder = BAML_HOME / "toolchains" / f"0.{11 + i}.0"
        (folder / "bin").mkdir(parents=True, exist_ok=True)
        shutil.copy2(CHILD, folder / "bin" / "baml-cli")
        (folder / "VERSION").write_text(f"0.{11 + i}.0\n")
    cache = BAML_HOME / "manifest-cache" / "prod" / "version"
    cache.mkdir(parents=True, exist_ok=True)
    targets = [
        "aarch64-apple-darwin",
        "x86_64-apple-darwin",
        "aarch64-unknown-linux-gnu",
        "x86_64-unknown-linux-gnu",
        "aarch64-unknown-linux-musl",
        "x86_64-unknown-linux-musl",
        "aarch64-pc-windows-msvc",
        "x86_64-pc-windows-msvc",
    ]
    manifest = {
        "schema": 1,
        "version": "0.11.0",
        "channel": "canary",
        "released_at": "2026-10-03T00:00:00Z",
        "artifacts": {
            t: {"url": f"https://example.invalid/{t}.tar.gz", "sha256": "0" * 64}
            for t in targets
        },
    }
    (cache / "0.11.0.json").write_text(json.dumps(manifest))
    reset()


def reset():
    (BAML_HOME / "config.toml").write_text(CONFIG)
    (PROJECT / "baml.toml").write_text(PROJECT_CONFIG)


def summarize(samples):
    result = {"n": len(samples)}
    for key in samples[0]:
        values = sorted(s[key] for s in samples)
        result[key] = {
            "median": statistics.median(values),
            "p95": values[max(0, (len(values) * 95 + 99) // 100 - 1)],
            "p99": values[max(0, (len(values) * 99 + 99) // 100 - 1)],
            "min": values[0],
            "max": values[-1],
            "mean": statistics.mean(values),
        }
    return result


def measure(binary, args, mode="off", env_extra=None, allow_stderr=False):
    env = (
        BASE_ENV
        | ({} if mode == "local" else {"BAML_TELEMETRY": "off"})
        | (env_extra or {})
    )
    command = [str(MEASURE), str(binary)] + list(args)
    out = subprocess.run(
        command, cwd=PROJECT, env=env, capture_output=True, text=True, timeout=65
    )
    if out.returncode != 0:
        raise RuntimeError(f"Measurement failed: {out.stderr}")
    data = json.loads(out.stdout)
    data["cpu_ms"] = data["user_ms"] + data["system_ms"]
    data["exit_after_first_output_ms"] = (
        data["wall_ms"] - data["first_output_ms"]
        if data["first_output_ms"] >= 0
        else -1
    )
    if data["exit"] != 0 or (out.stderr and not allow_stderr):
        raise RuntimeError(f"{binary} {args}: {data}; stderr={out.stderr}")
    return data


def timings(n=100):
    cases = {
        "help": (["toolchain", "--help"], {}),
        "version_installed": (["--version"], {}),
        "list_10": (["toolchain", "list"], {}),
        "passthrough_installed": (["noop"], {}),
        "passthrough_local": (["noop"], {"BAML_TOOLCHAIN": str(CHILD)}),
        "version_local": (["--version"], {"BAML_TOOLCHAIN": str(CHILD)}),
        "pin_local": (["toolchain", "pin", str(CHILD)], {}),
        "use_local": (["toolchain", "use", str(CHILD)], {}),
        "install_cached": (["toolchain", "install", "0.11.0"], {}),
        "stream_32mib": (["stream"], {"BAML_TOOLCHAIN": str(CHILD)}),
    }
    data = {}
    disk = {}
    rng = random.Random(20261004)
    variants = {
        "old": (OLD, "off"),
        "packed_off": (NEW, "off"),
        "packed_local": (NEW, "local"),
    }
    for name, (args, extra) in cases.items():
        before = local_stats()
        data[name] = {v: [] for v in variants}
        for _ in range(4):
            for binary, mode in variants.values():
                reset()
                measure(binary, args, mode, extra)
        for i in range(n):
            order = list(variants)
            rng.shuffle(order)
            for variant in order:
                reset()
                binary, mode = variants[variant]
                row = measure(binary, args, mode, extra)
                if name == "stream_32mib":
                    assert row["stdout_bytes"] == 32 * 1024 * 1024, row
                elif name.startswith("passthrough"):
                    assert row["stdout_bytes"] == len("ready\n"), row
                data[name][variant].append(row)
        summary = {v: summarize(s) for v, s in data[name].items()}
        after = local_stats()
        disk[name] = {key: after[key] - before[key] for key in before}
        (ROOT / "local-disk-by-case.json").write_text(json.dumps(disk, indent=2))
        (ROOT / "timings-raw.json").write_text(json.dumps(data, indent=2))
        print(
            name,
            {
                v: {
                    "wall_ms": round(s["wall_ms"]["median"], 3),
                    "p95": round(s["wall_ms"]["p95"], 3),
                    "rss_mib": round(s["peak_rss_bytes"]["median"] / 2**20, 2),
                }
                for v, s in summary.items()
            },
            flush=True,
        )
    summary = {
        c: {v: summarize(s) for v, s in variants.items()}
        for c, variants in data.items()
    }
    (ROOT / "timings.json").write_text(json.dumps(summary, indent=2))
    direct = {
        case: summarize([measure(CHILD, [arg]) for _ in range(n)])
        for case, arg in [("noop", "noop"), ("stream_32mib", "stream")]
    }
    (ROOT / "direct-child.json").write_text(json.dumps(direct, indent=2))


def end_to_end(n=40):
    project = ROOT / "check-project"
    project.mkdir(exist_ok=True)
    (project / "baml.toml").write_text('[package]\nname = "wrapper_metrics"\n')
    (project / "main.baml").write_text(
        'function Main() -> void { baml.io.println("hello"); }\n'
    )
    cases = {
        "cli_version": ["--version"],
        "cli_check": ["check", "--project", str(project)],
        "cli_run": ["run", "Main", "--project", str(project)],
    }
    data = {}
    rng = random.Random(20261004)
    for name, args in cases.items():
        data[name] = {
            "direct": [],
            "old": [],
            "packed_off": [],
            "direct_local": [],
            "old_local": [],
            "packed_local": [],
        }
        choices = {
            "direct": (CLI, "off"),
            "old": (OLD, "off"),
            "packed_off": (NEW, "off"),
            "direct_local": (CLI, "local"),
            "old_local": (OLD, "local"),
            "packed_local": (NEW, "local"),
        }
        for _ in range(3):
            for binary, mode in choices.values():
                measure(
                    binary, args, mode, {"BAML_TOOLCHAIN": str(CLI)}, allow_stderr=True
                )
        for i in range(n):
            order = list(choices)
            rng.shuffle(order)
            for key in order:
                binary, mode = choices[key]
                data[name][key].append(
                    measure(
                        binary,
                        args,
                        mode,
                        {"BAML_TOOLCHAIN": str(CLI)},
                        allow_stderr=True,
                    )
                )
        print(
            "end to end",
            name,
            {
                k: round(statistics.median(r["wall_ms"] for r in rows), 3)
                for k, rows in data[name].items()
            },
            flush=True,
        )
        (ROOT / "end-to-end-raw.json").write_text(json.dumps(data, indent=2))
    (ROOT / "end-to-end.json").write_text(
        json.dumps(
            {c: {v: summarize(rows) for v, rows in d.items()} for c, d in data.items()},
            indent=2,
        )
    )


def runtime_floor(n=60):
    data = {}
    rng = random.Random(20261004)
    cases = {
        "native_noop": (CHILD, ["noop"], "off", {}),
        "tiny_off": (TINY, [], "off", {}),
        "tiny_local": (TINY, [], "local", {}),
    }
    for name, (binary, args, mode, extra) in cases.items():
        for _ in range(3):
            measure(binary, args, mode, extra)
        data[name] = []
    for _ in range(n):
        order = list(cases)
        rng.shuffle(order)
        for name in order:
            binary, args, mode, extra = cases[name]
            data[name].append(measure(binary, args, mode, extra))
    levels = {level: [] for level in ["off", "low", "medium", "high"]}
    for _ in range(40):
        order = list(levels)
        rng.shuffle(order)
        for level in order:
            levels[level].append(
                measure(NEW, ["--version"], "local", {"BAML_TELEMETRY": level})
            )
    summary = {name: summarize(rows) for name, rows in data.items()}
    profiles = {level: summarize(rows) for level, rows in levels.items()}
    (ROOT / "runtime-floor-raw.json").write_text(json.dumps(data, indent=2))
    (ROOT / "runtime-floor.json").write_text(json.dumps(summary, indent=2))
    (ROOT / "profiles-raw.json").write_text(json.dumps(levels, indent=2))
    (ROOT / "profiles.json").write_text(json.dumps(profiles, indent=2))
    print(
        "runtime floor",
        {name: round(row["wall_ms"]["median"], 3) for name, row in summary.items()},
        flush=True,
    )
    print(
        "profiles",
        {name: round(row["wall_ms"]["median"], 3) for name, row in profiles.items()},
        flush=True,
    )


class TaskInfo(ctypes.Structure):
    _fields_ = [
        (name, ctypes.c_uint64)
        for name in [
            "virtual",
            "resident",
            "user",
            "system",
            "threads_user",
            "threads_system",
        ]
    ] + [
        (name, ctypes.c_int32)
        for name in [
            "policy",
            "faults",
            "pageins",
            "cow",
            "sent",
            "received",
            "mach_syscalls",
            "unix_syscalls",
            "switches",
            "threads",
            "running",
            "priority",
        ]
    ]


class UsageInfo0(ctypes.Structure):
    _fields_ = [("uuid", ctypes.c_uint8 * 16)] + [
        (name, ctypes.c_uint64)
        for name in [
            "user",
            "system",
            "pkg_wakeups",
            "interrupt_wakeups",
            "pageins",
            "wired",
            "resident",
            "footprint",
            "start",
            "exit",
        ]
    ]


def resident():
    lib = ctypes.CDLL("/usr/lib/libproc.dylib")
    lib.proc_pidinfo.argtypes = [
        ctypes.c_int,
        ctypes.c_int,
        ctypes.c_uint64,
        ctypes.c_void_p,
        ctypes.c_int,
    ]
    lib.proc_pid_rusage.argtypes = [ctypes.c_int, ctypes.c_int, ctypes.c_void_p]
    data = {}
    for alloc in [0, 64]:
        for name, binary, mode in [
            ("direct", CHILD, "off"),
            ("old", OLD, "off"),
            ("packed_off", NEW, "off"),
            ("packed_local", NEW, "local"),
        ]:
            rows = []
            for _ in range(4):
                env = (
                    BASE_ENV
                    | {"BAML_TOOLCHAIN": str(CHILD)}
                    | ({} if mode == "local" else {"BAML_TELEMETRY": "off"})
                )
                p = subprocess.Popen(
                    [str(binary), "hold", str(alloc)],
                    cwd=PROJECT,
                    env=env,
                    stdout=subprocess.PIPE,
                    stderr=subprocess.PIPE,
                    text=True,
                )
                line = p.stdout.readline().strip()
                assert line.startswith("ready "), line
                child_pid = int(line.split()[1])
                pids = sorted({p.pid, child_pid})
                time.sleep(0.15)
                processes = []
                for pid in pids:
                    info = TaskInfo()
                    length = lib.proc_pidinfo(
                        pid, 4, 0, ctypes.byref(info), ctypes.sizeof(info)
                    )
                    assert length == ctypes.sizeof(info), (pid, length)
                    fd_capacity = lib.proc_pidinfo(pid, 1, 0, None, 0)
                    fd_buffer = ctypes.create_string_buffer(fd_capacity + 1024)
                    fd_bytes = lib.proc_pidinfo(pid, 1, 0, fd_buffer, len(fd_buffer))
                    usage = UsageInfo0()
                    assert lib.proc_pid_rusage(pid, 0, ctypes.byref(usage)) == 0, pid
                    # Distinguish virtual stack/address reservations from actual resident memory.
                    processes.append(
                        {
                            "pid": pid,
                            "role": "child" if pid == child_pid else "wrapper",
                            "rss_bytes": info.resident,
                            "footprint_bytes": usage.footprint,
                            "virtual_bytes": info.virtual,
                            "threads": info.threads,
                            "open_fds": fd_bytes // 8,
                        }
                    )
                _, err = p.communicate(timeout=8)
                assert p.returncode == 0 and not err, (p.returncode, err)
                rows.append(
                    {
                        "processes": processes,
                        "process_count": len(pids),
                        "total_rss_bytes": sum(i["rss_bytes"] for i in processes),
                        "total_footprint_bytes": sum(
                            i["footprint_bytes"] for i in processes
                        ),
                        "total_threads": sum(i["threads"] for i in processes),
                    }
                )
            data[f"{name}_{alloc}mib"] = rows
            print(
                "resident",
                name,
                alloc,
                "MiB",
                round(statistics.median(r["total_rss_bytes"] for r in rows) / 2**20, 2),
                "threads",
                rows[0]["total_threads"],
                flush=True,
            )
    (ROOT / "resident.json").write_text(json.dumps(data, indent=2))


def lsp():
    lib = ctypes.CDLL("/usr/lib/libproc.dylib")
    lib.proc_pidinfo.argtypes = [
        ctypes.c_int,
        ctypes.c_int,
        ctypes.c_uint64,
        ctypes.c_void_p,
        ctypes.c_int,
    ]
    lib.proc_pid_rusage.argtypes = [ctypes.c_int, ctypes.c_int, ctypes.c_void_p]
    project = ROOT / "tiny-project"
    data = {}
    for name, binary, mode in [
        ("direct", CLI, "off"),
        ("direct_local", CLI, "local"),
        ("old", OLD, "off"),
        ("old_local", OLD, "local"),
        ("packed_off", NEW, "off"),
        ("packed_local", NEW, "local"),
    ]:
        rows = []
        for _ in range(4):
            env = (
                BASE_ENV
                | {"BAML_TOOLCHAIN": str(CLI)}
                | ({} if mode == "local" else {"BAML_TELEMETRY": "off"})
            )
            start = time.monotonic()
            p = subprocess.Popen(
                [str(binary), "lsp", "--workspace", str(project)],
                cwd=PROJECT,
                env=env,
                stdin=subprocess.PIPE,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
            )
            buffer = bytearray()

            def send(msg):
                body = json.dumps({"jsonrpc": "2.0"} | msg).encode()
                p.stdin.write(f"Content-Length: {len(body)}\r\n\r\n".encode() + body)
                p.stdin.flush()

            def read_response(wanted):
                deadline = time.monotonic() + 15
                while True:
                    while True:
                        index = buffer.find(b"\r\n\r\n")
                        if index >= 0:
                            headers = buffer[:index].decode()
                            length = int(
                                next(
                                    line.split(":", 1)[1]
                                    for line in headers.split("\r\n")
                                    if line.lower().startswith("content-length:")
                                )
                            )
                            if len(buffer) >= index + 4 + length:
                                msg = json.loads(buffer[index + 4 : index + 4 + length])
                                del buffer[: index + 4 + length]
                                break
                        timeout = deadline - time.monotonic()
                        assert (
                            timeout > 0
                            and select.select([p.stdout], [], [], timeout)[0]
                        ), "LSP response timeout"
                        chunk = os.read(p.stdout.fileno(), 65536)
                        assert chunk, "LSP closed before response"
                        buffer.extend(chunk)
                    if msg.get("id") == wanted and "method" not in msg:
                        assert "error" not in msg, msg
                        return msg
                    if "id" in msg and "method" in msg:
                        send({"id": msg["id"], "result": None})

            send(
                {
                    "id": 1,
                    "method": "initialize",
                    "params": {
                        "processId": os.getpid(),
                        "rootUri": project.as_uri(),
                        "capabilities": {},
                        "workspaceFolders": [
                            {"uri": project.as_uri(), "name": "metrics"}
                        ],
                    },
                }
            )
            read_response(1)
            init_ms = (time.monotonic() - start) * 1000
            send({"method": "initialized", "params": {}})
            time.sleep(0.5)
            snapshot = subprocess.check_output(
                ["/bin/ps", "-axo", "pid=,ppid="], text=True
            )
            pairs = [tuple(map(int, line.split())) for line in snapshot.splitlines()]
            pids = {p.pid}
            for _ in range(3):
                pids |= {pid for pid, ppid in pairs if ppid in pids}
            processes = []
            for pid in sorted(pids):
                info = TaskInfo()
                usage = UsageInfo0()
                assert lib.proc_pidinfo(
                    pid, 4, 0, ctypes.byref(info), ctypes.sizeof(info)
                ) == ctypes.sizeof(info), pid
                assert lib.proc_pid_rusage(pid, 0, ctypes.byref(usage)) == 0, pid
                capacity = lib.proc_pidinfo(pid, 1, 0, None, 0)
                fds = ctypes.create_string_buffer(capacity + 1024)
                fdbytes = lib.proc_pidinfo(pid, 1, 0, fds, len(fds))
                processes.append(
                    {
                        "pid": pid,
                        "rss_bytes": info.resident,
                        "footprint_bytes": usage.footprint,
                        "virtual_bytes": info.virtual,
                        "threads": info.threads,
                        "open_fds": fdbytes // 8,
                    }
                )
            send({"id": 2, "method": "shutdown", "params": None})
            read_response(2)
            send({"method": "exit", "params": None})
            p.stdin.close()
            p.stdin = None
            _, err = p.communicate(timeout=15)
            assert p.returncode == 0, (p.returncode, err.decode())
            rows.append(
                {
                    "initialize_ms": init_ms,
                    "processes": processes,
                    "process_count": len(processes),
                    "total_rss_bytes": sum(i["rss_bytes"] for i in processes),
                    "total_footprint_bytes": sum(
                        i["footprint_bytes"] for i in processes
                    ),
                    "total_threads": sum(i["threads"] for i in processes),
                }
            )
        data[name] = rows
        print(
            "LSP",
            name,
            "init",
            round(statistics.median(r["initialize_ms"] for r in rows), 2),
            "rss MiB",
            round(statistics.median(r["total_rss_bytes"] for r in rows) / 2**20, 2),
            "footprint MiB",
            round(
                statistics.median(r["total_footprint_bytes"] for r in rows) / 2**20, 2
            ),
            flush=True,
        )
        (ROOT / "lsp.json").write_text(json.dumps(data, indent=2))


def live_baml():
    lib = ctypes.CDLL("/usr/lib/libproc.dylib")
    lib.proc_pidinfo.argtypes = [
        ctypes.c_int,
        ctypes.c_int,
        ctypes.c_uint64,
        ctypes.c_void_p,
        ctypes.c_int,
    ]
    lib.proc_pid_rusage.argtypes = [ctypes.c_int, ctypes.c_int, ctypes.c_void_p]
    project = ROOT / "held-project"
    project.mkdir(exist_ok=True)
    (project / "baml.toml").write_text('[package]\nname = "held_metrics"\n')
    (project / "main.baml").write_text(
        'function Hold() -> void { baml.io.println("ready"); baml.sys.sleep(baml.time.Duration.from_seconds(2)); }\n'
    )
    data = {}
    for name, binary, mode in [
        ("direct", CLI, "off"),
        ("old", OLD, "off"),
        ("old_local", OLD, "local"),
        ("packed_off", NEW, "off"),
        ("packed_local", NEW, "local"),
    ]:
        rows = []
        for _ in range(4):
            env = (
                BASE_ENV
                | {"BAML_TOOLCHAIN": str(CLI)}
                | ({} if mode == "local" else {"BAML_TELEMETRY": "off"})
            )
            p = subprocess.Popen(
                [str(binary), "run", "Hold", "--project", str(project)],
                cwd=PROJECT,
                env=env,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
            )
            line = p.stdout.readline().strip()
            assert line == "ready", line
            time.sleep(0.15)
            snapshot = subprocess.check_output(
                ["/bin/ps", "-axo", "pid=,ppid="], text=True
            )
            pairs = [tuple(map(int, line.split())) for line in snapshot.splitlines()]
            pids = {p.pid}
            for _ in range(3):
                pids |= {pid for pid, ppid in pairs if ppid in pids}
            processes = []
            for pid in sorted(pids):
                info = TaskInfo()
                usage = UsageInfo0()
                assert lib.proc_pidinfo(
                    pid, 4, 0, ctypes.byref(info), ctypes.sizeof(info)
                ) == ctypes.sizeof(info), pid
                assert lib.proc_pid_rusage(pid, 0, ctypes.byref(usage)) == 0, pid
                capacity = lib.proc_pidinfo(pid, 1, 0, None, 0)
                fds = ctypes.create_string_buffer(capacity + 1024)
                fdbytes = lib.proc_pidinfo(pid, 1, 0, fds, len(fds))
                processes.append(
                    {
                        "pid": pid,
                        "rss_bytes": info.resident,
                        "footprint_bytes": usage.footprint,
                        "virtual_bytes": info.virtual,
                        "threads": info.threads,
                        "open_fds": fdbytes // 8,
                    }
                )
            _, err = p.communicate(timeout=8)
            assert p.returncode == 0, (p.returncode, err)
            rows.append(
                {
                    "processes": processes,
                    "process_count": len(processes),
                    "total_rss_bytes": sum(i["rss_bytes"] for i in processes),
                    "total_footprint_bytes": sum(
                        i["footprint_bytes"] for i in processes
                    ),
                    "total_threads": sum(i["threads"] for i in processes),
                }
            )
        data[name] = rows
        print(
            "live BAML",
            name,
            "rss MiB",
            round(statistics.median(r["total_rss_bytes"] for r in rows) / 2**20, 2),
            "footprint MiB",
            round(
                statistics.median(r["total_footprint_bytes"] for r in rows) / 2**20, 2
            ),
            flush=True,
        )
        (ROOT / "live-baml.json").write_text(json.dumps(data, indent=2))


class Handler(http.server.BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def log_message(self, *args):
        pass

    def reply(self, status, body=b"", content_type="application/json"):
        self.send_response(status)
        self.send_header("Content-Type", content_type)
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        try:
            self.wfile.write(body)
        except (BrokenPipeError, ConnectionResetError):
            pass

    def do_POST(self):
        raw = self.rfile.read(int(self.headers.get("Content-Length", 0)))
        request = json.loads(raw)
        self.server.events.append(
            {"method": "POST", "bytes": len(raw), "time": time.monotonic()}
        )
        time.sleep(self.server.delay)
        if self.server.fail:
            self.reply(503, b"{}")
            return
        expiry = int(time.time() * 1000) + 3600000
        base = f"http://127.0.0.1:{self.server.server_port}"
        uploads = []
        cas = []
        for target in request["proposed_uploads"]:
            uid = f"{request['recording']['recording_id']}-{request['recording']['recording_file_sequence']}-{target['client_target_id']}"
            uploads.append(
                {
                    "upload_id": uid,
                    "client_target_id": target["client_target_id"],
                    "object_key": "key-" + uid,
                    "presigned_put_url": base + "/put/" + uid,
                    "expires_at_unix_ms": expiry,
                    "required_headers": {},
                    "kind": target["kind"],
                    "candidate_indices": target["candidate_indices"],
                }
            )
            kind = {
                "recording": "inline_with_recording",
                "cas_batch": "member_of_batch",
                "cas_object": "separate_object",
            }[target["kind"]]
            cas.extend(
                {
                    "candidate_index": idx,
                    "disposition": {"kind": kind, "upload_id": uid},
                }
                for idx in target["candidate_indices"]
            )
        body = json.dumps(
            {
                "plan_id": "plan-"
                + request["recording"]["recording_id"]
                + "-"
                + str(request["recording"]["recording_file_sequence"]),
                "expires_at_unix_ms": expiry,
                "uploads": uploads,
                "cas": cas,
            }
        ).encode()
        self.server.response_bytes += len(body)
        self.reply(200, body)

    def do_PUT(self):
        size = int(self.headers.get("Content-Length", 0))
        body = self.rfile.read(size)
        self.server.events.append(
            {"method": "PUT", "bytes": len(body), "time": time.monotonic()}
        )
        time.sleep(self.server.delay)
        self.reply(200, b"")


def cloud():
    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    server.daemon_threads = True
    server.events = []
    server.delay = 0
    server.fail = False
    server.response_bytes = 0
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    extra = {
        "BOUNDARY_URL": f"http://127.0.0.1:{server.server_port}",
        "BOUNDARY_API_KEY": "synthetic-benchmark-only",
    }
    data = {}
    for mode, delay, fail, n in [
        ("fast", 0, False, 20),
        ("100ms_per_request", 0.1, False, 5),
        ("503_retries", 0, True, 3),
        ("timeouts", 11, True, 1),
    ]:
        server.delay = delay
        server.fail = fail
        rows = []
        for _ in range(n):
            server.events = []
            server.response_bytes = 0
            env = BASE_ENV | extra
            out = subprocess.run(
                [str(MEASURE), str(NEW), "--version"],
                cwd=PROJECT,
                env=env,
                capture_output=True,
                text=True,
                timeout=65,
            )
            assert out.returncode == 0, out.stderr
            row = json.loads(out.stdout)
            row["cpu_ms"] = row["user_ms"] + row["system_ms"]
            row["exit_after_first_output_ms"] = row["wall_ms"] - row["first_output_ms"]
            assert row["exit"] == 0, row
            row["requests"] = len(server.events)
            row["post_bytes"] = sum(
                e["bytes"] for e in server.events if e["method"] == "POST"
            )
            row["put_bytes"] = sum(
                e["bytes"] for e in server.events if e["method"] == "PUT"
            )
            row["response_bytes"] = server.response_bytes
            row["warning"] = out.stderr.strip()
            if not fail:
                assert not row["warning"], row
            rows.append(row)
        data[mode] = rows
        print(
            "cloud",
            mode,
            {
                "wall_ms": round(statistics.median(r["wall_ms"] for r in rows), 2),
                "requests": rows[0]["requests"],
                "put_bytes": rows[0]["put_bytes"],
                "warning": rows[0]["warning"],
            },
            flush=True,
        )
        (ROOT / "cloud.json").write_text(json.dumps(data, indent=2))
    server.shutdown()
    server.server_close()


def size():
    data = {}
    for name, path in [("old", OLD), ("packed", NEW), ("host", HOST), ("tiny", TINY)]:
        raw = path.read_bytes()
        compressed = gzip.compress(raw, compresslevel=9, mtime=0)
        data[name] = {
            "bytes": len(raw),
            "gzip_bytes": len(compressed),
            "sha256": hashlib.sha256(raw).hexdigest(),
            "path": str(path),
        }
    data["pack_added_bytes"] = data["packed"]["bytes"] - data["host"]["bytes"]
    (ROOT / "sizes.json").write_text(json.dumps(data, indent=2))
    print(
        "sizes",
        {
            k: v
            if isinstance(v, int)
            else {
                "mib": round(v["bytes"] / 2**20, 3),
                "gzip_mib": round(v["gzip_bytes"] / 2**20, 3),
            }
            for k, v in data.items()
        },
        flush=True,
    )


def local_stats():
    root = BAML_HOME / "btel"
    dirs = (
        list((root / "recordings").iterdir()) if (root / "recordings").exists() else []
    )
    data = {
        "recordings": len(dirs),
        "recording_bytes": sum(
            p.stat().st_size for d in dirs for p in d.rglob("*") if p.is_file()
        ),
        "cas_bytes": sum(
            p.stat().st_size for p in (root / "cas").rglob("*") if p.is_file()
        ),
        "cas_files": sum(1 for p in (root / "cas").rglob("*") if p.is_file()),
    }
    return data


def recordings():
    data = local_stats()
    (ROOT / "local-disk.json").write_text(json.dumps(data, indent=2))
    print("local disk", data, flush=True)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "part",
        choices=[
            "all",
            "timings",
            "resident",
            "cloud",
            "size",
            "recordings",
            "end_to_end",
            "lsp",
            "runtime_floor",
            "live_baml",
        ],
    )
    parser.add_argument("--runs", type=int, default=100)
    for name in ("output", "rust", "packed", "cli", "host", "tiny"):
        parser.add_argument("--" + name, required=True, type=pathlib.Path)
    args = parser.parse_args()
    ROOT = args.output.resolve()
    PROJECT = ROOT / "fixture" / "project"
    HOME = ROOT / "fixture" / "home"
    BAML_HOME = HOME / ".baml"
    CHILD = ROOT / "child"
    OLD, NEW, CLI, HOST, TINY = [
        getattr(args, name).resolve()
        for name in ("rust", "packed", "cli", "host", "tiny")
    ]
    MEASURE = ROOT / "measure"
    BASE_ENV = {
        "PATH": "/usr/bin:/bin",
        "LANG": "en_US.UTF-8",
        "HOME": str(HOME),
        "BAML_HOME": str(BAML_HOME),
        "NO_COLOR": "1",
        "BAML_CLI_ALLOW_DIRECT": "1",
    }
    setup()
    parts = {
        "timings": lambda: timings(args.runs),
        "resident": resident,
        "cloud": cloud,
        "size": size,
        "recordings": recordings,
        "end_to_end": lambda: end_to_end(args.runs),
        "lsp": lsp,
        "runtime_floor": lambda: runtime_floor(args.runs),
        "live_baml": live_baml,
    }
    if args.part == "all":
        # Pin the full report's sample sizes independently of single-part runs.
        for part, run in (
            ("timings", lambda: timings(100)),
            ("runtime_floor", lambda: runtime_floor(60)),
            ("end_to_end", lambda: end_to_end(40)),
            ("resident", resident),
            ("live_baml", live_baml),
            ("lsp", lsp),
            ("cloud", cloud),
            ("size", size),
            ("recordings", recordings),
        ):
            print("starting", part, flush=True)
            run()
    else:
        parts[args.part]()

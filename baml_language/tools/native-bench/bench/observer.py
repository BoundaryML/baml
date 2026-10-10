"""Linux process observations; workload clocks remain inside each adapter.

A small C parent (observer.c) pins the child to the requested CPUs, waits on
it with wait4 and reports rusage, so the Python launcher's own memory never
enters the child's peak RSS. It is compiled on first use into the cache.
"""

import json
import os
from pathlib import Path
import selectors
import signal
import subprocess
import hashlib
import time

from . import hostinfo as host

SOURCE = Path(__file__).resolve().with_name("observer.c")


def observer():
    """The compiled observer, keyed by the source hash so a change rebuilds it."""
    digest = hashlib.sha256(SOURCE.read_bytes()).hexdigest()[:12]
    cache = Path(os.environ.get("BENCH_CACHE", Path.home() / ".cache" / "baml-bench"))
    binary = cache / f"observer-{digest}"
    if not binary.exists():
        cache.mkdir(parents=True, exist_ok=True)
        subprocess.run(["cc", "-O2", "-Wall", "-Wextra", "-Werror", str(SOURCE), "-o", str(binary)], check=True)
    return binary



def run(command, directory, env, cpus, timeout=None, stdin=None):
    directory.mkdir(parents=True, exist_ok=False)
    usage, pidfile = directory / "usage.json", directory / "pid.txt"
    sampler = selectors.DefaultSelector()
    samples, events = [], []
    malformed = 0
    executable = observer()
    host_before = host.snapshot(cpus)
    start = time.monotonic_ns()
    with (
        (directory / "stderr.txt").open("wb") as stderr,
        (directory / "stdout.txt").open("wb") as stdout,
    ):
        child = subprocess.Popen(
            [
                str(executable),
                str(pidfile),
                str(usage),
                str(directory),
                ",".join(map(str, cpus)),
                *map(str, command),
            ],
            stdin=subprocess.PIPE if stdin is not None else subprocess.DEVNULL,
            stdout=subprocess.PIPE,
            stderr=stderr,
            env=env,
            start_new_session=True,
        )
        if stdin is not None:
            child.stdin.write(stdin)
            child.stdin.close()
        sampler.register(child.stdout, selectors.EVENT_READ)
        buffer = b""
        status = "ok"
        pid = None
        ready = None
        result = None
        drain = None
        try:
            while sampler.get_map() or child.poll() is None:
                now = time.monotonic_ns()
                if timeout and (now - start) / 1e9 > timeout:
                    status = "timeout"
                    os.killpg(child.pid, signal.SIGKILL)
                    break
                if pid is None and pidfile.exists():
                    try:
                        pid = int(pidfile.read_text())
                    except ValueError:
                        pass
                if pid:
                    try:
                        fields = (
                            Path(f"/proc/{pid}/stat")
                            .read_text()
                            .rsplit(")", 1)[1]
                            .split()
                        )
                        samples.append(
                            {
                                "elapsed_ns": now - start,
                                "rss_bytes": int(fields[21])
                                * os.sysconf("SC_PAGE_SIZE"),
                            }
                        )
                    except (FileNotFoundError, ProcessLookupError):
                        pass
                for key, _ in sampler.select(0.01):
                    data = os.read(key.fileobj.fileno(), 65536)
                    if not data:
                        sampler.unregister(key.fileobj)
                        continue
                    stdout.write(data)
                    buffer += data
                    while b"\n" in buffer:
                        line, buffer = buffer.split(b"\n", 1)
                        try:
                            event = json.loads(line)
                        except (ValueError, UnicodeDecodeError):
                            malformed += 1
                            continue
                        if not isinstance(event, dict):
                            malformed += 1
                            continue
                        event["received_ns"] = time.monotonic_ns() - start
                        events.append(event)
                        if event.get("event") == "ready":
                            ready = event["received_ns"]
                        if event.get("event") == "result":
                            result = event
                        if event.get("event") == "drained":
                            drain = event
            code = child.wait(timeout=5)
            if code and status == "ok":
                status = "exit_error"
        finally:
            if child.poll() is None:
                os.killpg(child.pid, signal.SIGKILL)
                child.wait()
            child.stdout.close()
            sampler.close()
    observed = json.loads(usage.read_text()) if usage.exists() else {}
    (directory / "host.json").write_text(
        json.dumps({"before": host_before, "after": host.snapshot(cpus)}, indent=2)
        + "\n"
    )
    (directory / "rss.json").write_text(json.dumps(samples) + "\n")
    (directory / "events.json").write_text(json.dumps(events, indent=2) + "\n")
    return {
        "event_order": [event.get("event") for event in events],
        "malformed_lines": malformed + bool(buffer),
        "status": status,
        "exit_code": code,
        "process_wall_ns": observed.get("wall_ns"),
        "process_cpu_ns": round(
            (observed.get("user_cpu_s", 0) + observed.get("system_cpu_s", 0)) * 1e9
        )
        if observed
        else None,
        "peak_rss_bytes": observed.get("peak_rss_bytes"),
        "launch_to_ready_ns": ready,
        "result": result,
        "drain": drain,
    }

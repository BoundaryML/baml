"""One trial: spawn a fresh process, reap it with wait4, read the driver's report.

The parent observes what the child cannot report about itself: process wall
time from spawn to exit, CPU time, and peak resident memory from the kernel's
rusage accounting. The child reports the in-process timed region.
"""

import json
import os
import subprocess
import sys
import tempfile
import threading
import time
from dataclasses import dataclass, field

# ru_maxrss is KiB on Linux and bytes on macOS.
_RSS_SCALE = 1 if sys.platform == "darwin" else 1024


@dataclass
class Trial:
    ok: bool
    error: str | None = None
    result: str | None = None
    jobs: int = 0
    elapsed_ns: int | None = None      # in-process timed region, all jobs
    process_ns: int | None = None      # spawn to exit, observed by the parent
    cpu_ns: int | None = None          # user + system
    peak_rss_bytes: int | None = None
    extra: dict = field(default_factory=dict)

    @property
    def per_job_ns(self):
        return self.elapsed_ns / self.jobs if self.ok and self.jobs else None


def run(cmd, env=None, cpu=None, timeout=600.0):
    """Run ``cmd`` once and return a Trial. Never raises for child failures."""
    def pin():
        if cpu is not None:
            os.sched_setaffinity(0, {cpu})

    with tempfile.TemporaryFile() as err:
        start = time.perf_counter_ns()
        try:
            child = subprocess.Popen(cmd, stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=err,
                                     env=env, preexec_fn=pin if cpu is not None else None)
        except OSError as e:
            return Trial(ok=False, error=f"spawn failed: {e}")
        timer = threading.Timer(timeout, child.kill)
        timer.start()
        try:
            out = child.stdout.read()
            # wait4, not Popen.wait: it is the only way to get the child's rusage.
            _, status, usage = os.wait4(child.pid, 0)
        finally:
            timer.cancel()
        process_ns = time.perf_counter_ns() - start
        child.returncode = os.waitstatus_to_exitcode(status)
        err.seek(0)
        stderr = err.read().decode(errors="replace")

    trial = Trial(ok=False, process_ns=process_ns,
                  cpu_ns=int((usage.ru_utime + usage.ru_stime) * 1e9),
                  peak_rss_bytes=usage.ru_maxrss * _RSS_SCALE)
    if child.returncode != 0:
        tail = stderr.strip().splitlines()[-1:] or ["no stderr"]
        trial.error = f"exit {child.returncode}" + (" (timed out)" if child.returncode == -9 else "") + f": {tail[0][:300]}"
        return trial
    lines = out.decode(errors="replace").strip().splitlines()
    try:
        report = json.loads(lines[-1])
        trial.result = str(report["result"])
        trial.elapsed_ns = int(report["elapsed_ns"])
        trial.jobs = int(report["jobs"])
    except (IndexError, ValueError, KeyError, TypeError):
        trial.error = f"no report on stdout: {(lines[-1] if lines else '')[:300]!r}"
        return trial
    trial.ok = True
    return trial

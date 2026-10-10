"""Read host conditions outside the measured child; affinity is not isolation."""

import os
from pathlib import Path
import time


def read(path):
    try:
        return Path(path).read_text().strip()
    except OSError:
        return None


def snapshot(cpus):
    return {
        "time_unix_ns": time.time_ns(),
        "loadavg": read("/proc/loadavg"),
        "cpu_pressure": read("/proc/pressure/cpu"),
        "memory_pressure": read("/proc/pressure/memory"),
        "cpu_ticks": {
            line.split()[0]: list(map(int, line.split()[1:]))
            for line in Path("/proc/stat").read_text().splitlines()
            if line.startswith("cpu")
        },
        "clock_ticks_per_second": os.sysconf("SC_CLK_TCK"),
        "cpufreq": {
            str(cpu): {
                key: read(f"/sys/devices/system/cpu/cpu{cpu}/cpufreq/{key}")
                for key in ("scaling_governor", "scaling_cur_freq", "scaling_max_freq")
            }
            for cpu in cpus
        },
    }

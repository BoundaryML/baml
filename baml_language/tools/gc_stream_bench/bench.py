#!/usr/bin/env python3
"""Measure GC behavior while streaming many LLM extractions into Python.

Each stream asks a cheap model (Haiku, or gpt-5.4-nano with --provider openai)
to rebuild seed/pr-5041.json from seed/pr-5041.txt, consuming every partial
through the generated `baml_sdk`. Both collectors are observed: CPython's (via `gc.callbacks`) and
the BAML engine's (via `baml_bridge.baml_py._gc_stats()`; pause times need an
extension built with `gc_profiling`, which setup.sh does).

    ./setup.sh                                   # once, and after Rust changes
    .venv/bin/python bench.py                    # replay the checked-in recording
    .venv/bin/python bench.py --streams 32 --concurrency 8 --api async
    infisical run --env=dev-llm-provider-tests -- .venv/bin/python bench.py --mode record --streams 1
    infisical run --env=dev-llm-provider-tests -- .venv/bin/python bench.py --mode live --streams 2
"""
import argparse
import asyncio
import gc
import itertools
import json
import os
import platform
import statistics
import subprocess
import sys
import threading
import time
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

import psutil

HERE = Path(__file__).resolve().parent
SEED_JSON = HERE / 'seed' / 'pr-5041.json'
SEED_TEXT = HERE / 'seed' / 'pr-5041.txt'
BASE_URL_ENV = 'GC_BENCH_BASE_URL'
PROVIDERS = {
    'anthropic': dict(function='ExtractPullRequest', recording='haiku-pr-5041.sse',
                      upstream='https://api.anthropic.com'),
    'openai': dict(function='ExtractPullRequestNano', recording='nano-pr-5041.sse',
                   upstream='https://api.openai.com/v1'),
}


class PyGcMonitor:
    """Times every CPython collection through `gc.callbacks`."""

    def __init__(self):
        self.pauses = {0: [], 1: [], 2: []}
        self.collected = 0
        self.uncollectable = 0
        self._started = {}

    def __call__(self, phase, info):
        now = time.perf_counter()
        tid = threading.get_ident()
        if phase == 'start':
            self._started[tid] = now
        elif tid in self._started:
            self.pauses[info['generation']].append(now - self._started.pop(tid))
            self.collected += info['collected']
            self.uncollectable += info['uncollectable']

    def summary(self):
        allp = [p for ps in self.pauses.values() for p in ps]
        return dict(
            collections={f'gen{g}': len(ps) for g, ps in self.pauses.items()},
            pause_total_ms=sum(allp) * 1e3,
            pause_max_ms=max(allp, default=0) * 1e3,
            pause_p99_ms=percentile(allp, 99) * 1e3,
            pause_total_ms_by_gen={f'gen{g}': sum(ps) * 1e3 for g, ps in self.pauses.items()},
            collected=self.collected,
            uncollectable=self.uncollectable,
        )


class Sampler(threading.Thread):
    """Samples RSS and the BAML heap at a fixed interval to find peaks."""

    def __init__(self, interval, baml_stats):
        super().__init__(daemon=True)
        self.interval = interval
        self.baml_stats = baml_stats
        self.proc = psutil.Process()
        self.stop = threading.Event()
        self.peak_rss = 0
        self.peak_runtime_objects = 0
        self.peak_reserved_slots = 0

    def sample(self):
        self.peak_rss = max(self.peak_rss, self.proc.memory_info().rss)
        s = self.baml_stats()
        self.peak_runtime_objects = max(self.peak_runtime_objects, s['runtime_objects'])
        self.peak_reserved_slots = max(self.peak_reserved_slots, s['reserved_slots'])

    def run(self):
        while not self.stop.wait(self.interval):
            self.sample()


def percentile(values, pct):
    if not values:
        return 0.0
    values = sorted(values)
    return values[min(len(values) - 1, round(pct / 100 * (len(values) - 1)))]


def leaves(value, path=''):
    if isinstance(value, dict):
        for k, v in value.items():
            yield from leaves(v, f'{path}.{k}')
    elif isinstance(value, list):
        for i, v in enumerate(value):
            yield from leaves(v, f'{path}[{i}]')
    else:
        yield path, value


def accuracy(final, expected):
    """Fraction of the seed's leaf values the extraction reproduced exactly."""
    got = dict(leaves(final.model_dump(mode='json')))
    want = dict(leaves(expected))
    return sum(got.get(k) == v for k, v in want.items()) / len(want)


def partial_timings(t0, stamps, final):
    """Time to first partial, then the gaps between consecutive partials."""
    return dict(final=final, first=stamps[0] - t0 if stamps else None,
                gaps=[b - a for a, b in itertools.pairwise(stamps)], partials=len(stamps),
                wall=time.perf_counter() - t0)


def consume_sync(fns, text, done_type):
    stream = fns['sync'](text)
    t0 = time.perf_counter()
    stamps = []
    while not isinstance(stream.next(), done_type):
        stamps.append(time.perf_counter())
    return partial_timings(t0, stamps, final=stream.final())


async def consume_async(fns, text, done_type):
    stream = await fns['async'](text)
    t0 = time.perf_counter()
    stamps = []
    while not isinstance(await stream.next_async(), done_type):
        stamps.append(time.perf_counter())
    return partial_timings(t0, stamps, final=await stream.final_async())


def run_streams(args, fns, text, done_type):
    if args.api == 'async':
        async def all_streams():
            sem = asyncio.Semaphore(args.concurrency)

            async def one():
                async with sem:
                    return await consume_async(fns, text, done_type)
            return await asyncio.gather(*(one() for _ in range(args.streams)))
        return asyncio.run(all_streams())
    with ThreadPoolExecutor(args.concurrency) as pool:
        return list(pool.map(lambda _: consume_sync(fns, text, done_type), range(args.streams)))


def delta(after, before):
    out = {}
    for k, v in after.items():
        if isinstance(v, dict):
            out[k] = {r: n - before.get(k, {}).get(r, 0) for r, n in v.items()}
        elif k in ('runtime_objects', 'reserved_slots', 'active_handles', 'last_live_count', 'max_pause_s', 'max_total_s'):
            out[k] = v
        else:
            out[k] = v - before.get(k, 0)
    return out


def start_server(args):
    if args.mode == 'live':
        os.environ.pop(BASE_URL_ENV, None)
        return None
    if args.mode == 'record' and args.streams != 1:
        sys.exit('--mode record makes exactly one request; pass --streams 1')
    cmd = [sys.executable, str(HERE / 'replay_server.py'), '--recording', str(args.recording),
           '--event-delay-ms', str(args.event_delay_ms), '--upstream', PROVIDERS[args.provider]['upstream']]
    if args.mode == 'record':
        cmd.append('--record')
    elif not args.recording.exists():
        sys.exit(f'{args.recording} is missing; create it with --mode record')
    else:
        # The replay server ignores credentials, but the clients insist on one.
        for key in ('ANTHROPIC_API_KEY', 'OPENAI_API_KEY'):
            os.environ.setdefault(key, 'replay-placeholder')
    server = subprocess.Popen(cmd, stdout=subprocess.PIPE, text=True)
    os.environ[BASE_URL_ENV] = f'http://127.0.0.1:{server.stdout.readline().strip()}'
    return server


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--mode', choices=['replay', 'live', 'record'], default='replay')
    parser.add_argument('--provider', choices=sorted(PROVIDERS), default='openai',
                        help='openai until a Haiku recording is checked in (see README)')
    parser.add_argument('--recording', type=Path, help='Default: recordings/<per-provider>.sse')
    parser.add_argument('--event-delay-ms', type=float, default=0.5, help='Replay pacing per SSE event')
    parser.add_argument('--streams', type=int, default=8, help='Measured streams')
    parser.add_argument('--warmup', type=int, default=1, help='Unmeasured streams run first')
    parser.add_argument('--concurrency', type=int, default=1)
    parser.add_argument('--api', choices=['sync', 'async'], default='sync')
    parser.add_argument('--sample-ms', type=float, default=50)
    parser.add_argument('--json-out', type=Path, help='Also write the full report here')
    args = parser.parse_args()
    args.recording = args.recording or HERE / 'recordings' / PROVIDERS[args.provider]['recording']
    if args.mode != 'replay':
        args.warmup = 0

    server = start_server(args)
    try:
        report = measure(args)
    finally:
        if server:
            server.terminate()
            server.wait()
    print(json.dumps(report, indent=2))
    if args.json_out:
        args.json_out.parent.mkdir(parents=True, exist_ok=True)
        args.json_out.write_text(json.dumps(report, indent=2) + '\n')


def measure(args):
    sys.path.insert(0, str(HERE))
    from baml_bridge import baml_py

    import baml_sdk as sdk
    from baml_sdk.ai.stream import Done

    name = PROVIDERS[args.provider]['function']
    fns = {'sync': getattr(sdk, f'{name}_stream'), 'async': getattr(sdk, f'{name}_stream_async')}
    text = SEED_TEXT.read_text()
    expected = json.loads(SEED_JSON.read_text())
    if args.warmup:
        run_streams(argparse.Namespace(**{**vars(args), 'streams': args.warmup}), fns, text, Done)

    proc = psutil.Process()
    monitor = PyGcMonitor()
    sampler = Sampler(args.sample_ms / 1000, baml_py._gc_stats)
    gc.collect()
    py_objects_before = len(gc.get_objects())
    rss_before = proc.memory_info().rss
    baml_py._gc_stats(reset=True)  # measured GC totals, maxima included, start here
    baml_before = baml_py._gc_stats()
    gc.callbacks.append(monitor)
    sampler.start()
    cpu0, t0 = time.process_time(), time.perf_counter()
    try:
        results = run_streams(args, fns, text, Done)
    finally:
        wall, cpu = time.perf_counter() - t0, time.process_time() - cpu0
        sampler.stop.set()
        sampler.join()
        gc.callbacks.remove(monitor)
    sampler.sample()
    baml_after = baml_py._gc_stats()
    rss_after = proc.memory_info().rss

    gaps = [g for r in results for g in r['gaps']]
    partials = [r['partials'] for r in results]
    firsts = [r['first'] for r in results if r['first'] is not None]
    scores = [accuracy(r['final'], expected) for r in results]
    del results
    gc.collect()
    baml_settled = baml_py._gc_stats()
    py_objects_retained = len(gc.get_objects()) - py_objects_before

    baml = delta(baml_after, baml_before)
    n_partials = sum(partials)
    return dict(
        config=dict(vars(args), recording=str(args.recording), json_out=str(args.json_out),
                    python=platform.python_version(), platform=platform.platform(),
                    gc_profiling='pause_s' in baml_after),
        run=dict(
            wall_s=wall, cpu_s=cpu, streams=len(partials), partials=n_partials,
            partials_per_stream=statistics.mean(partials),
            accuracy=statistics.mean(scores),
        ),
        first_partial_ms=dict(p50=percentile(firsts, 50) * 1e3, max=max(firsts, default=0) * 1e3),
        partial_gap_ms=dict(
            p50=percentile(gaps, 50) * 1e3, p99=percentile(gaps, 99) * 1e3,
            max=max(gaps, default=0) * 1e3,
        ),
        python_gc=dict(monitor.summary(), collections_per_1k_partials=1e3 * sum(
            len(p) for p in monitor.pauses.values()) / max(n_partials, 1),
            objects_retained_after_collect=py_objects_retained),
        baml_gc=dict(
            baml, cycles_per_stream=baml['cycles'] / len(partials),
            pause_ms_per_stream=baml.get('pause_s', 0) * 1e3 / len(partials),
            peak_runtime_objects=sampler.peak_runtime_objects,
            peak_reserved_slots=sampler.peak_reserved_slots,
            active_handles_before=baml_before['active_handles'],
            active_handles_after_collect=baml_settled['active_handles'],
        ),
        memory_mb=dict(
            rss_before=rss_before / 2**20, rss_peak=sampler.peak_rss / 2**20, rss_after=rss_after / 2**20,
        ),
    )


if __name__ == '__main__':
    main()

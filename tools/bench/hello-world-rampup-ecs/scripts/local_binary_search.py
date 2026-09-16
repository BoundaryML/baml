#!/usr/bin/env python3
"""Binary-search one public benchmark target with local Vegeta 4-on/1-off cycles."""
import argparse
import json
import shutil
import subprocess
import tempfile
import time
import urllib.request
from pathlib import Path


def midpoint(lower, upper, resolution):
    steps = (upper - lower) // resolution
    return lower + max(1, steps // 2) * resolution


def probe(url):
    try:
        with urllib.request.urlopen(url, timeout=2) as response:
            return response.status == 200 and response.read(32) == b'hello world'
    except OSError:
        return False


def run_candidate(url, target_file, rate, cycles, connections, timeout_ms, output):
    candidate_started = time.monotonic()
    records = []
    for cycle in range(cycles):
        cycle_started = time.monotonic()
        scheduled = rate * 4
        attack = subprocess.run([
            'vegeta', 'attack', f'-targets={target_file}', f'-rate={rate}/1s', '-duration=4s',
            f'-timeout={timeout_ms}ms', '-dns-ttl=1s', f'-workers={connections}',
            f'-max-workers={connections}', f'-connections={connections}', f'-max-connections={connections}',
            '-keepalive=true', '-http2=false', '-redirects=0', '-max-body=32',
        ], stdout=subprocess.PIPE, stderr=subprocess.PIPE, check=True, timeout=15)
        report_process = subprocess.run(['vegeta', 'report', '-type=json'], input=attack.stdout,
                                        stdout=subprocess.PIPE, stderr=subprocess.PIPE, check=True, timeout=10)
        report = json.loads(report_process.stdout)
        statuses = {str(key): int(value) for key, value in report.get('status_codes', {}).items()}
        requests = int(report.get('requests', 0))
        http200 = statuses.get('200', 0)
        errors = report.get('errors', [])
        record = {
            'event': 'local_cycle', 'rate': rate, 'cycle': cycle, 'scheduled': scheduled,
            'requests': requests, 'http200': http200, 'scheduled_ratio': requests / scheduled,
            'success_ratio': http200 / scheduled, 'probe_ok': probe(url), 'errors': errors,
            'latencies_ns': report.get('latencies', {}), 'vegeta_rate': report.get('rate'),
            'vegeta_throughput': report.get('throughput'),
        }
        records.append(record)
        output(record)
        time.sleep(max(0, cycle_started + 5 - time.monotonic()))
    aggregate_scheduled = sum(record['scheduled'] for record in records)
    aggregate_requests = sum(record['requests'] for record in records)
    aggregate_http200 = sum(record['http200'] for record in records)
    passed = (aggregate_requests / aggregate_scheduled >= 0.99 and
              aggregate_http200 / aggregate_scheduled >= 0.99 and
              min(record['success_ratio'] for record in records) >= 0.95 and
              all(record['probe_ok'] for record in records))
    summary = {
        'event': 'local_candidate', 'rate': rate, 'cycles': cycles, 'passed': passed,
        'scheduled': aggregate_scheduled, 'requests': aggregate_requests, 'http200': aggregate_http200,
        'scheduled_ratio': aggregate_requests / aggregate_scheduled,
        'success_ratio': aggregate_http200 / aggregate_scheduled,
        'minimum_cycle_success_ratio': min(record['success_ratio'] for record in records),
        'elapsed_seconds': time.monotonic() - candidate_started,
    }
    output(summary)
    return passed, summary


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--url', required=True)
    parser.add_argument('--lower', type=int, default=5000)
    parser.add_argument('--upper', type=int, default=10000)
    parser.add_argument('--resolution', type=int, default=100)
    parser.add_argument('--seconds', type=int, default=30)
    parser.add_argument('--connections', type=int, default=8000)
    parser.add_argument('--timeout-ms', type=int, default=800)
    parser.add_argument('--verify-lower', action='store_true')
    parser.add_argument('--output', type=Path)
    args = parser.parse_args()
    if not shutil.which('vegeta'):
        parser.error('vegeta is not installed')
    if not args.url.startswith('http://') or '\n' in args.url:
        parser.error('--url must be one HTTP URL')
    if not 1 <= args.lower < args.upper <= 50000:
        parser.error('expected 1 <= lower < upper <= 50000')
    if args.resolution < 1 or args.seconds < 5 or args.seconds % 5:
        parser.error('--resolution must be positive and --seconds must be a positive multiple of five')
    if not 100 <= args.timeout_ms < 1000:
        parser.error('--timeout-ms must fit inside the one-second off-window')
    cycles = args.seconds // 5
    lines = []

    def emit(record):
        line = json.dumps(record, separators=(',', ':'))
        lines.append(line)
        print(line, flush=True)
        if args.output:
            args.output.parent.mkdir(parents=True, exist_ok=True)
            args.output.write_text('\n'.join(lines) + '\n')

    with tempfile.NamedTemporaryFile('w', prefix='vegeta-target-', suffix='.txt') as target:
        target.write(f'GET {args.url}\n')
        target.flush()
        lower, upper = args.lower, args.upper
        if args.verify_lower:
            passed, _ = run_candidate(args.url, target.name, lower, cycles, args.connections, args.timeout_ms, emit)
            if not passed:
                emit({'event': 'local_search_stopped', 'reason': 'lower_bound_failed', 'rate': lower})
                raise SystemExit(2)
        tested = set()
        while upper - lower > args.resolution:
            candidate = midpoint(lower, upper, args.resolution)
            if candidate in tested or candidate in (lower, upper):
                break
            tested.add(candidate)
            passed, _ = run_candidate(args.url, target.name, candidate, cycles, args.connections, args.timeout_ms, emit)
            if passed:
                lower = candidate
            else:
                upper = candidate
        emit({'event': 'local_search_finished', 'highest_pass': lower, 'lowest_fail': upper,
              'resolution': args.resolution, 'seconds_per_candidate': args.seconds})


if __name__ == '__main__':
    main()

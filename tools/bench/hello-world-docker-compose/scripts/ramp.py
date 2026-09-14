#!/usr/bin/env python3
"""Run three-minute Vegeta stages on the 10-RPS project; stop at Debian's first failure."""
import datetime
import json
import os
from pathlib import Path
import subprocess
import time
import urllib.parse
import urllib.request

ROOT = Path(__file__).resolve().parents[1]
PROJECT = 'baml-local-hello-world-10rps'
NAMES = [e['name'] for e in json.loads((ROOT / 'experiments.json').read_text())]
LOADS = ['load-' + name for name in NAMES]
ENV_FILE = ROOT / '.env.10rps'
DEST = ROOT / 'results' / PROJECT / ('ramp-' + datetime.datetime.now().strftime('%Y%m%d-%H%M%S'))
DOCKER = ['docker', '--context', os.environ.get('LOCAL_DOCKER_CONTEXT', 'colima-baml-hello-world')]
COMPOSE = [str(ROOT / 'scripts/compose.sh'), '--env-file', str(ENV_FILE), '-p', PROJECT]


def compose(*args):
    subprocess.run(COMPOSE + list(args), cwd=ROOT, check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)


def inspect(name):
    return json.loads(subprocess.check_output(DOCKER + ['inspect', f'{PROJECT}-{name}-1']))[0]


def query(expression):
    url = 'http://localhost:9190/api/v1/query?query=' + urllib.parse.quote(expression)
    with urllib.request.urlopen(url, timeout=10) as response:
        return json.load(response)['data']['result']


def scalar(expression):
    rows = query(expression)
    return float(rows[0]['value'][1]) if rows else None


def set_rate(rate):
    lines = ENV_FILE.read_text().splitlines()
    assert any(line.startswith('RATE_PER_TARGET=') for line in lines)
    ENV_FILE.write_text('\n'.join(f'RATE_PER_TARGET={rate}' if line.startswith('RATE_PER_TARGET=') else line for line in lines) + '\n')


def save(report):
    temporary = DEST / 'summary.tmp'
    temporary.write_text(json.dumps(report, indent=2) + '\n')
    temporary.replace(DEST / 'summary.json')


def main():
    DEST.mkdir(parents=True)
    report = {'project': PROJECT, 'stage_seconds': 180, 'rates': [10, 20, 40, 80, 160, 320, 640, 1280],
              'status': 'starting', 'stages': [], 'note': 'Apps restart once before the ramp, not between stages. Failure rate is not an independent sustainable-throughput threshold.'}
    previous = inspect('baml-debian')
    report['previous_debian'] = {key: previous[key] for key in ('Created', 'State', 'RestartCount')}
    print(f'Report directory: {DEST}', flush=True)
    save(report)
    try:
        compose('stop', *LOADS)
        compose('restart', *NAMES)
        # Confirm the clean baseline is listening before beginning the attacks.
        deadline = time.monotonic() + 60
        for e in json.loads((ROOT / 'experiments.json').read_text()):
            while True:
                try:
                    with urllib.request.urlopen(f'http://localhost:{8180 + e["port"] % 10}/', timeout=2) as response:
                        assert response.read() == b'hello world'
                    break
                except Exception:
                    if time.monotonic() > deadline:
                        raise RuntimeError(f'{e["name"]} did not become ready')
                    time.sleep(1)
        baseline = inspect('baml-debian')
        report['debian_baseline'] = {key: baseline[key] for key in ('State', 'RestartCount')}
        report['started_unix'] = time.time()
        report['status'] = 'running'
        bad_windows = 0
        for rate in report['rates']:
            set_rate(rate)
            compose('up', '-d', '--no-deps', '--force-recreate', *LOADS)
            stage = {'rate_per_target': rate, 'started_unix': time.time(), 'samples': 0}
            report['stages'].append(stage)
            stage_start = time.monotonic()
            print(f'STAGE {rate} RPS per experiment for 180 seconds', flush=True)
            save(report)
            while time.monotonic() - stage_start < 180:
                info = inspect('baml-debian')
                sample = {'unix': time.time(), 'rate_per_target': rate, 'stage_elapsed': time.monotonic() - stage_start,
                          'restarts': info['RestartCount'], 'state': info['State']}
                reason = None
                if info['RestartCount'] != baseline['RestartCount'] or info['State']['StartedAt'] != baseline['State']['StartedAt'] or not info['State']['Running']:
                    reason = 'Debian exited or restarted'
                sample['rss_bytes'] = scalar('process_resident_memory_bytes{experiment="baml-debian"}')
                sample['working_set_bytes'] = scalar('hello_container_memory_working_set_bytes{experiment="baml-debian"}')
                sample['total_requests_this_attack'] = scalar('sum(request_seconds_count{experiment="baml-debian"})')
                sample['transport_errors_this_attack'] = scalar('sum(request_seconds_count{experiment="baml-debian",status="0"})') or 0
                sample['rps'] = {row['metric']['experiment']: float(row['value'][1]) for row in query('sum by (experiment) (rate(request_seconds_count[20s]))')}
                # Allow the previous attack's scrape samples to expire after replacement.
                if sample['stage_elapsed'] >= 30:
                    if sample['transport_errors_this_attack'] > 0:
                        reason = reason or 'Debian transport errors'
                    actual = sample['rps'].get('baml-debian', 0)
                    controls_ok = all(sample['rps'].get(name, 0) >= rate * .95 for name in NAMES if name != 'baml-debian')
                    bad_windows = bad_windows + 1 if actual < rate * .95 else 0
                    if bad_windows >= 3:
                        reason = reason or ('Debian sustained throughput below 95% of target' if controls_ok else 'Load delivery is below target across experiments; capacity finding is inconclusive')
                with (DEST / 'samples.jsonl').open('a') as stream:
                    stream.write(json.dumps(sample) + '\n')
                stage['samples'] += 1
                stage['latest'] = sample
                report['elapsed_seconds'] = time.time() - report['started_unix']
                if reason:
                    stage['ended_unix'] = time.time()
                    report.update(status='failure_observed', reason=reason, failure_rate=rate, failure_sample=sample)
                    print(f'STOP: {reason} at {rate} RPS, elapsed {report["elapsed_seconds"]:.1f}s', flush=True)
                    save(report)
                    return
                save(report)
                time.sleep(min(5, max(0, 180 - (time.monotonic() - stage_start))))
            stage['ended_unix'] = time.time()
            stage['completed'] = True
            bad_windows = 0
            print(f'PASS {rate} RPS: 180 seconds without Debian failure; RSS {stage["latest"]["rss_bytes"]}', flush=True)
        report.update(status='ceiling_reached', reason='No Debian failure observed through 1280 RPS; further ramp requires a generator-capacity assessment.')
    except BaseException as exc:
        report.update(status='interrupted', reason=repr(exc))
        raise
    finally:
        # Preserve app state and monitoring, but stop this project's test traffic.
        compose('stop', *LOADS)
        report['stopped_unix'] = time.time()
        if 'started_unix' in report:
            events = subprocess.check_output(DOCKER + ['events', '--since', str(int(report['started_unix'])),
                '--until', str(int(time.time())), '--filter', f'container={PROJECT}-baml-debian-1', '--format', '{{json .}}'], text=True)
            (DEST / 'docker-events.jsonl').write_text(events)
        save(report)
        print(f'Load stopped. Evidence: {DEST / "summary.json"}', flush=True)


if __name__ == '__main__':
    main()

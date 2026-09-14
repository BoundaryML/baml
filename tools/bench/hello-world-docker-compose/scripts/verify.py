#!/usr/bin/env python3
"""Verify one Compose project's responses, quotas, Vegeta load, and dashboard."""
import argparse
import http.client
import json
import math
import os
from pathlib import Path
import subprocess
import time
import urllib.parse
import urllib.request

ROOT = Path(__file__).resolve().parents[1]
EXPERIMENTS = json.loads((ROOT / 'experiments.json').read_text())


def fetch(url):
    with urllib.request.urlopen(url, timeout=15) as response:
        return json.load(response)


def docker(*args):
    return subprocess.check_output(['docker', '--context', os.environ.get('LOCAL_DOCKER_CONTEXT', 'colima-baml-hello-world'),
                                    *args], text=True, cwd=ROOT)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--seconds', type=int, default=60)
    parser.add_argument('-p', '--project-name')
    parser.add_argument('--env-file')
    args = parser.parse_args()
    if args.seconds < 15:
        parser.error('--seconds must be at least 15')
    command = [str(ROOT / 'scripts/compose.sh')]
    if args.project_name:
        command += ['-p', args.project_name]
    if args.env_file:
        command += ['--env-file', args.env_file]

    def compose(*parts):
        return subprocess.check_output(command + list(parts), text=True, cwd=ROOT)

    config = json.loads(compose('config', '--format', 'json'))
    project = config['name']

    def inspect(service):
        ids = compose('ps', '-a', '-q', service).split()
        assert len(ids) == 1, (service, 'expected exactly one container', ids)
        info = json.loads(docker('inspect', ids[0]))[0]
        assert info['Config']['Labels']['com.docker.compose.project'] == project
        return info

    def endpoint(service, port):
        return 'http://' + compose('port', service, str(port)).strip()

    prometheus = endpoint('prometheus', 9090)
    grafana = endpoint('grafana', 3000)

    def query(expression):
        return fetch(prometheus + '/api/v1/query?query=' + urllib.parse.quote(expression))['data']['result']

    def counts():
        values = {e['name']: {'total': 0, 'successes': 0, 'transport_errors': 0} for e in EXPERIMENTS}
        for row in query('request_seconds_count{job="vegeta"}'):
            v = values[row['metric']['experiment']]
            count = float(row['value'][1])
            v['total'] += count
            if row['metric']['status'] == '200':
                v['successes'] += count
            if row['metric']['status'] == '0':
                v['transport_errors'] += count
        for row in query('max by (experiment) (timestamp(request_seconds_count{job="vegeta"}))'):
            values[row['metric']['experiment']]['sample_time'] = float(row['value'][1])
        assert all('sample_time' in v for v in values.values()), 'Wait for the first Vegeta scrape before verifying'
        return values

    report = {'project': project, 'verified_unix': time.time(), 'seconds': args.seconds, 'experiments': {}}
    initial = {}
    for experiment in EXPERIMENTS:
        name = experiment['name']
        info = inspect(name)
        host = info['HostConfig']
        assert info['State']['Running'], (name, info['State'])
        assert host['NanoCpus'] == 1_000_000_000, (name, host['NanoCpus'])
        assert host['Memory'] == host['MemorySwap'] == 1_073_741_824, (name, host['Memory'])
        image = json.loads(docker('image', 'inspect', info['Image']))[0]
        assert image['Architecture'] == 'arm64', (name, image['Architecture'])
        address = urllib.parse.urlparse(endpoint(name, 8080))
        conn = http.client.HTTPConnection(address.hostname, address.port, timeout=15)
        for _ in range(2):
            conn.request('GET', '/')
            response = conn.getresponse()
            assert response.status == 200, (name, response.status)
            assert response.read() == b'hello world', name
            assert response.getheader('Content-Type') == 'text/plain; charset=utf-8', name
            assert response.getheader('Cache-Control') == 'no-store', name
            assert response.getheader('Content-Length') == '11', name
            assert not response.will_close, (name, 'keepalive disabled')
        conn.close()
        load = inspect('load-' + name)
        assert load['State']['Running'], (name, load['State'])
        rate_arg = next(part for part in config['services']['load-' + name]['command'] if part.startswith('-rate='))
        expected_rate = float(rate_arg.removeprefix('-rate=').removesuffix('/1s'))
        assert math.isfinite(expected_rate) and expected_rate > 0, rate_arg
        assert rate_arg in load['Config']['Cmd'], (name, 'Running load differs from selected configuration', rate_arg)
        initial[name] = (info['RestartCount'], load['State']['StartedAt'])
        report['experiments'][name] = {'container': info['Id'], 'image': info['Image'], 'memory_limit': host['Memory'],
                                      'cpus': host['NanoCpus'] / 1e9, 'architecture': image['Architecture'],
                                      'expected_rps': expected_rate}
    before = counts()
    print(f'{project}: exact responses and hard quotas pass for all {len(EXPERIMENTS)} experiments. Measuring {args.seconds}s of Vegeta load...', flush=True)
    time.sleep(args.seconds)
    after = counts()
    failures = []
    for name, result in report['experiments'].items():
        start, end = before[name], after[name]
        elapsed = end['sample_time'] - start['sample_time']
        assert elapsed > 0, (name, 'Prometheus stopped scraping')
        completed = end['total'] - start['total']
        rps = completed / elapsed
        successes = end['successes'] - start['successes']
        errors = end['transport_errors'] - start['transport_errors']
        info, load = inspect(name), inspect('load-' + name)
        restarts = info['RestartCount'] - initial[name][0]
        result.update(completed_requests=completed, observed_rps=round(rps, 3), successes=successes,
                      transport_errors=errors, restarts=info['RestartCount'], new_restarts=restarts)
        if not (result['expected_rps'] * .95 <= rps <= result['expected_rps'] * 1.05 and completed == successes and not errors and not restarts
                and info['State']['Running'] and load['State']['Running']
                and load['State']['StartedAt'] == initial[name][1]):
            failures.append(name)
    expected_names = {e['name'] for e in EXPERIMENTS}
    for expression in ['hello_container_cpu_seconds_total', 'process_resident_memory_bytes',
                       'request_seconds_count{job="vegeta"}']:
        rows = query(expression)
        assert {r['metric']['experiment'] for r in rows} == expected_names, (expression, rows)
    for expression in ['nodejs_heap_size_used_bytes', 'python_traced_memory_current_bytes']:
        rows = query(expression)
        assert len(rows) == 2 and all(float(row['value'][1]) > 0 for row in rows), expression
    baml_names = {'node-baml', 'python-baml', 'baml-debian'}
    heap_metrics = ['baml_heap_total_object_slots', 'baml_heap_compile_time_object_slots',
                    'baml_heap_runtime_object_slots', 'baml_heap_active_handles', 'baml_heap_tlab_chunks']
    report['baml_heap_metrics'] = {}
    # One evaluation time keeps all five fields on the same scrape per target.
    rows = query('{__name__=~"baml_heap_.*"}')
    for metric in heap_metrics:
        samples = [r for r in rows if r['metric']['__name__'] == metric]
        assert len(samples) == len(baml_names) and {r['metric']['experiment'] for r in samples} == baml_names, (metric, samples)
        values = {r['metric']['experiment']: float(r['value'][1]) for r in samples}
        assert all(math.isfinite(v) and v >= 0 for v in values.values()), (metric, values)
        if metric in {'baml_heap_total_object_slots', 'baml_heap_compile_time_object_slots'}:
            assert all(v > 0 for v in values.values()), (metric, values)
        report['baml_heap_metrics'][metric] = values
    for name in baml_names:
        values = report['baml_heap_metrics']
        assert values['baml_heap_total_object_slots'][name] == (
            values['baml_heap_compile_time_object_slots'][name] + values['baml_heap_runtime_object_slots'][name]), name
    targets = fetch(prometheus + '/api/v1/targets')['data']['activeTargets']
    assert len(targets) == 11 and all(t['health'] == 'up' for t in targets), targets
    dashboard = fetch(grafana + '/api/dashboards/uid/baml-local-hello-world')['dashboard']
    comparison_panels = [p for p in dashboard['panels'] if p['title'].startswith('Comparison · ')]
    assert len(comparison_panels) in {0, 4}
    assert len(dashboard['panels']) - len(comparison_panels) == 4 * len(EXPERIMENTS) + len(baml_names)
    for name in baml_names:
        assert any(p['title'] == f'{name} · BAML heap slots' and
                   any(t['expr'] == f'baml_heap_runtime_object_slots{{experiment="{name}"}}'
                       for t in p.get('targets', [])) for p in dashboard['panels']), name
    report['failed_experiments'] = failures
    destination = ROOT / 'results' / project / 'verification.json'
    destination.parent.mkdir(parents=True, exist_ok=True)
    destination.write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(report, indent=2))
    if failures:
        raise SystemExit('Load verification failed: ' + ', '.join(failures) + f'. Evidence saved in {destination}.')


if __name__ == '__main__':
    main()

#!/usr/bin/env python3
"""Snapshot a named ECS run and compare its latest complete CloudWatch minutes."""
import argparse
import datetime as dt
import json
import os
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[1]
MATRIX = json.loads((ROOT / 'matrix.json').read_text())


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--name', required=True)
    p.add_argument('--region', default=os.environ.get('AWS_REGION', 'us-east-1'))
    p.add_argument('--minutes', type=int, default=2)
    a = p.parse_args()
    if not 1 <= a.minutes <= 60:
        p.error('--minutes must be 1..60')
    aws = ['aws', '--region', a.region]

    def fetch(*args):
        return json.loads(subprocess.check_output(aws + list(args) + ['--output', 'json']))

    # Leave two minutes for metric ingestion; retain exact bounds in the report.
    end = dt.datetime.now(dt.timezone.utc).replace(second=0, microsecond=0) - dt.timedelta(minutes=2)
    start = end - dt.timedelta(minutes=a.minutes)
    queries, identities = [], {}
    custom = [('Requests', 'Sum'), ('Http200', 'Sum'), ('TransportErrors', 'Sum'),
              ('HttpErrors', 'Sum'), ('BodyMismatches', 'Sum'), ('LatencyMs', 'p90')]
    for variant in MATRIX['variants']:
        for arch in MATRIX['architectures']:
            cell = f'{variant}-{arch}'
            process = [('ProcessRssBytes', 'Average'), ('ProcessMetricsUp', 'Minimum')] if variant != 'baml-only' else []
            for metric, stat in custom + process + [('CPUUtilization', 'Average'), ('MemoryUtilization', 'Average')]:
                service = metric.endswith('Utilization')
                dims = {'ClusterName': a.name, 'ServiceName': f'{a.name}-{cell}'} if service else {'RunName': a.name, 'Variant': variant, 'Architecture': arch}
                key = f'm{len(queries)}'
                identities[key] = (cell, metric)
                queries.append({'Id': key, 'MetricStat': {'Metric': {'Namespace': 'AWS/ECS' if service else 'BAML/HelloWorld',
                    'MetricName': metric, 'Dimensions': [{'Name': k, 'Value': v} for k, v in dims.items()]}, 'Period': 60, 'Stat': stat}})
    output = ROOT / 'artifacts' / a.name / ('verify-' + dt.datetime.now(dt.timezone.utc).strftime('%Y%m%dT%H%M%SZ'))
    output.mkdir(parents=True, exist_ok=True)
    query_file = output / 'queries.json'
    query_file.write_text(json.dumps(queries, indent=2) + '\n')
    metrics = fetch('cloudwatch', 'get-metric-data', '--metric-data-queries', 'file://' + str(query_file),
                    '--start-time', start.isoformat(), '--end-time', end.isoformat())
    cells = {}
    for result in metrics['MetricDataResults']:
        cell, metric = identities[result['Id']]
        cells.setdefault(cell, {})[metric] = dict(zip(result['Timestamps'], result['Values']))
    services = fetch('ecs', 'list-services', '--cluster', a.name)['serviceArns']
    state = []
    for i in range(0, len(services), 10):
        state += fetch('ecs', 'describe-services', '--cluster', a.name, '--services', *services[i:i+10])['services']
    stopped = fetch('ecs', 'list-tasks', '--cluster', a.name, '--desired-status', 'STOPPED')['taskArns']
    report = {'run': a.name, 'start': start.isoformat(), 'end': end.isoformat(), 'cells': cells,
              'services': [{k: s[k] for k in ['serviceName', 'desiredCount', 'runningCount', 'pendingCount']} for s in state],
              'recent_stopped_tasks': stopped, 'note': 'Stopped task listing is time-limited; retained task-events logs are the longer-term source.'}
    (output / 'metrics.json').write_text(json.dumps(metrics, indent=2) + '\n')
    (output / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
    healthy = len(state) == 20 and all(s['desiredCount'] == s['runningCount'] == 1 and s['pendingCount'] == 0 for s in state)
    print(f'{len(state)} services; all desired/running=1 with no pending tasks: {healthy}; recent stopped tasks: {len(stopped)}')
    good = healthy and not stopped
    for cell, values in cells.items():
        requests = sum(values['Requests'].values())
        ok = sum(values['Http200'].values())
        errors = sum(sum(values[key].values()) for key in ['TransportErrors', 'HttpErrors', 'BodyMismatches'])
        complete = all(len(samples) == a.minutes for samples in values.values())
        cpu = list(values['CPUUtilization'].values())
        memory = list(values['MemoryUtilization'].values())
        print(f'{cell}: HTTP200={ok / (60*a.minutes):.2f}/s requests={requests:g} errors={errors:g} complete={complete} cpu={cpu} memory={memory}')
        rss = values.get('ProcessRssBytes')
        if rss is not None:
            print(f'  native RSS bytes: {list(rss.values())}')
            good = good and bool(rss) and all(n > 0 for n in rss.values()) and all(n == 1 for n in values['ProcessMetricsUp'].values())
        good = good and complete and requests > 0 and ok == requests and errors == 0
    print(output / 'report.json')
    raise SystemExit(0 if good else 1)


if __name__ == '__main__':
    main()

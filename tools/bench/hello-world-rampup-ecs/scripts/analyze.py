#!/usr/bin/env python3
"""Download cycle evidence and summarize the first overload point for every cell."""
import argparse
import datetime as dt
import json
import os
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[1]


def summarize(events, stops=()):
    stop_counts = {}
    for stop in stops:
        key = (stop['cell'], stop.get('rate'))
        stop_counts[key] = stop_counts.get(key, 0) + 1
    grouped = {}
    for event in events:
        cell = f"{event['Variant']}-{event['Architecture']}"
        grouped.setdefault(cell, {}).setdefault(event['rate'], []).append(event)
    cells = {}
    for cell, rates in sorted(grouped.items()):
        rows = []
        for rate, cycles in sorted(rates.items()):
            scheduled = sum(item['scheduled'] for item in cycles)
            http200 = sum(item['Http200'] for item in cycles)
            errors = sum(item['TransportErrors'] + item['HttpErrors'] + item['BodyMismatches'] for item in cycles)
            body_mismatches = sum(item['BodyMismatches'] for item in cycles)
            forced = sum(bool(item['forced_stop']) for item in cycles)
            task_stops = stop_counts.get((cell, rate), 0)
            ratio = http200 / scheduled if scheduled else 0
            minimum_cycle_ratio = min((item['completion_ratio'] for item in cycles), default=0)
            stable = len(cycles) >= 5 and ratio >= 0.99 and minimum_cycle_ratio >= 0.95 and errors == 0 and forced == 0 and task_stops == 0
            fell_over = len(cycles) >= 5 and (ratio < 0.99 or minimum_cycle_ratio < 0.95 or body_mismatches > 0 or forced > 0 or task_stops > 0)
            rows.append({'rate': rate, 'cycles': len(cycles), 'scheduled': scheduled, 'http200': http200,
                         'completion_ratio': ratio, 'minimum_cycle_ratio': minimum_cycle_ratio,
                         'errors': errors, 'body_mismatches': body_mismatches, 'forced_stops': forced,
                         'task_stops': task_stops, 'stable': stable, 'fell_over': fell_over})
        passing = [row['rate'] for row in rows if row['stable']]
        first_failure = None
        for index, row in enumerate(rows):
            if not row['fell_over'] or (passing and row['rate'] <= min(passing)):
                continue
            if len(rows) == 1 or (index + 1 < len(rows) and rows[index + 1]['fell_over']):
                first_failure = row['rate']
                break
        uninterrupted_passing = [rate for rate in passing if first_failure is None or rate < first_failure]
        cells[cell] = {'highest_passing_rate': max(uninterrupted_passing, default=None), 'first_failure_rate': first_failure, 'rates': rows}
    return cells


def attach_stop_rates(events, stops):
    timelines = {}
    for event in events:
        cell = f"{event['Variant']}-{event['Architecture']}"
        timelines.setdefault(cell, []).append(event)
    for timeline in timelines.values():
        timeline.sort(key=lambda event: event['log_timestamp'])
    for stop in stops:
        candidates = [event for event in timelines.get(stop['cell'], []) if event['log_timestamp'] <= stop['log_timestamp']]
        stop['rate'] = candidates[-1]['rate'] if candidates else None
    return stops


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--name', required=True)
    parser.add_argument('--region', default=os.environ.get('AWS_REGION', 'us-east-1'))
    parser.add_argument('--aws-profile')
    parser.add_argument('--since-hours', type=float, default=3)
    args = parser.parse_args()
    aws = ['aws', '--region', args.region]
    if args.aws_profile:
        aws += ['--profile', args.aws_profile]
    start = dt.datetime.now(dt.timezone.utc) - dt.timedelta(hours=args.since_hours)

    def fetch(*command):
        return json.loads(subprocess.check_output(aws + list(command) + ['--output', 'json']))

    result = fetch('logs', 'filter-log-events', '--log-group-name', f'/baml/hello-world-rampup/{args.name}/load',
                   '--start-time', str(int(start.timestamp() * 1000)), '--filter-pattern', '{ $.event = "load_cycle_finished" }')
    events = []
    for item in result['events']:
        try:
            event = json.loads(item['message'])
        except json.JSONDecodeError:
            continue
        event['log_timestamp'] = item['timestamp']
        events.append(event)
    task_result = fetch('logs', 'filter-log-events', '--log-group-name', f'/baml/hello-world-rampup/{args.name}/task-events',
                        '--start-time', str(int(start.timestamp() * 1000)), '--filter-pattern', '{ $.detail.lastStatus = "STOPPED" }')
    stops = []
    prefix = f'service:{args.name}-'
    for item in task_result['events']:
        try:
            task_event = json.loads(item['message'])
        except json.JSONDecodeError:
            continue
        detail = task_event.get('detail', {})
        group = detail.get('group', '')
        app = next((container for container in detail.get('containers', []) if container.get('name') == 'app' and container.get('lastStatus') == 'STOPPED'), None)
        if not group.startswith(prefix) or app is None:
            continue
        stops.append({'cell': group.removeprefix(prefix), 'log_timestamp': item['timestamp'], 'time': task_event.get('time'),
                      'exit_code': app.get('exitCode'), 'container_reason': app.get('reason'), 'stopped_reason': detail.get('stoppedReason')})
    attach_stop_rates(events, stops)
    cells = summarize(events, stops)
    stamp = dt.datetime.now(dt.timezone.utc).strftime('%Y%m%dT%H%M%SZ')
    output = ROOT / 'artifacts' / args.name / f'analysis-{stamp}.json'
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps({'run': args.name, 'since': start.isoformat(), 'events': events, 'task_stops': stops, 'cells': cells}, indent=2) + '\n')
    print('| Cell | Highest passing active RPS | First failing active RPS |')
    print('| --- | ---: | ---: |')
    for cell, result in cells.items():
        print(f"| {cell} | {result['highest_passing_rate'] or 'none'} | {result['first_failure_rate'] or 'not observed'} |")
    for stop in stops:
        print(f"STOP {stop['cell']} at approximately {stop['rate']} active RPS: exit={stop['exit_code']} {stop['container_reason'] or stop['stopped_reason']}")
    print(output)


if __name__ == '__main__':
    main()

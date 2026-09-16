#!/usr/bin/env python3
"""Download one retained GC-grid run and render its throughput heatmap."""
import argparse
import csv
import datetime as dt
import json
import os
from pathlib import Path
import subprocess

import matplotlib
matplotlib.use('Agg')
import matplotlib.pyplot as plt
from matplotlib.colors import Normalize
from matplotlib.patches import Patch


def aws_json(profile, region, *args):
    command = ['aws', '--no-cli-pager', '--region', region]
    if profile:
        command += ['--profile', profile]
    command += list(args) + ['--output', 'json']
    return json.loads(subprocess.check_output(command))


def log_messages(profile, region, group, start_ms):
    response = aws_json(profile, region, 'logs', 'filter-log-events', '--log-group-name', group,
                        '--start-time', str(start_ms))
    messages = []
    for item in response.get('events', []):
        try:
            message = json.loads(item['message'])
        except json.JSONDecodeError:
            continue
        messages.append({'cloudwatch_log_timestamp_ms': item['timestamp'], **message})
    return messages


def iso_ms(value):
    return int(dt.datetime.fromisoformat(value.replace('Z', '+00:00')).timestamp() * 1000)


def app_stops(events, run):
    stops = []
    for event in events:
        detail = event.get('detail', {})
        if detail.get('group') != f'service:{run}-baml-only-arm64' or detail.get('lastStatus') != 'STOPPED':
            continue
        containers = detail.get('containers', [])
        container = containers[0] if containers else {}
        reason = ' '.join(str(value) for value in (detail.get('stoppedReason'), container.get('reason')) if value)
        stopped_at = detail.get('stoppedAt') or event.get('time')
        stops.append({
            'timestamp_ms': iso_ms(stopped_at) if stopped_at else event['cloudwatch_log_timestamp_ms'],
            'task_arn': detail.get('taskArn'),
            'exit_code': container.get('exitCode'),
            'reason': reason,
            'oom': container.get('exitCode') == 137 or 'out of memory' in reason.lower() or 'oom' in reason.lower(),
            'event': event,
        })
    return stops


def collect(profile, region, run, start_ms):
    load_group = f'/baml/hello-world-rampup/{run}/load'
    task_group = f'/baml/hello-world-rampup/{run}/task-events'
    load_events = log_messages(profile, region, load_group, start_ms)
    task_events = log_messages(profile, region, task_group, start_ms)
    starts = [event for event in load_events if event.get('event') == 'gc_grid_started']
    if len(starts) != 1:
        raise RuntimeError(f'Expected one gc_grid_started event, found {len(starts)}')
    metadata = starts[0]
    summaries = sorted((event for event in load_events if event.get('event') == 'gc_grid_cell_finished'), key=lambda event: event['started_at_unix_ms'])
    stops = app_stops(task_events, run)
    cells = []
    for index, summary in enumerate(summaries):
        stop_window_end = summaries[index + 1]['started_at_unix_ms'] if index + 1 < len(summaries) else summary['ended_at_unix_ms'] + 120_000
        matching_stops = [stop for stop in stops if summary['started_at_unix_ms'] <= stop['timestamp_ms'] < stop_window_end]
        oom = any(stop['oom'] for stop in matching_stops)
        outcome = 'oom' if oom else ('complete' if summary['completed_full_duration'] else 'incomplete')
        cells.append({**summary, 'outcome': outcome, 'matching_stops': matching_stops})
    expected = {(float(frequency), int(rate)) for frequency in metadata['gc_frequencies_hz'] for rate in metadata['rates_rps']}
    actual = {(float(cell['gc_frequency_hz']), int(cell['rate'])) for cell in cells}
    return {
        'metadata': {
            'generated_at': dt.datetime.now(dt.timezone.utc).isoformat(),
            'run': run,
            'region': region,
            'aws_profile': profile,
            'load_log_group': load_group,
            'task_event_log_group': task_group,
            'rates_rps': metadata['rates_rps'],
            'gc_frequencies_hz': metadata['gc_frequencies_hz'],
            'seconds_per_cell': metadata['seconds_per_cell'],
            'on_seconds': 4,
            'off_seconds': 1,
            'missing_cells': [{'gc_frequency_hz': frequency, 'rate': rate} for frequency, rate in sorted(expected - actual)],
        },
        'cells': cells,
        'app_stops': stops,
    }


def frequency_label(frequency):
    if frequency == 0:
        return '0\n(never)'
    if frequency < 1:
        return f'{frequency:g}\n(every {1 / frequency:g}s)'
    return f'{frequency:g}'


def contrasting_color(rgba):
    red, green, blue, unused = rgba
    luminance = 0.2126 * red + 0.7152 * green + 0.0722 * blue
    return '#111827' if luminance > 0.61 else 'white'


def render(data, output):
    rates = [int(rate) for rate in data['metadata']['rates_rps']]
    frequencies = [float(value) for value in data['metadata']['gc_frequencies_hz']]
    cells = {(float(cell['gc_frequency_hz']), int(cell['rate'])): cell for cell in data['cells']}
    cmap = matplotlib.colormaps['YlGnBu']
    norm = Normalize(vmin=0, vmax=max(rates))
    fig = plt.figure(figsize=(16, 10), facecolor='white')
    axis = fig.add_axes([0.115, 0.215, 0.72, 0.59])
    for row, frequency in enumerate(frequencies):
        for column, rate in enumerate(rates):
            cell = cells.get((frequency, rate))
            if cell is None:
                color, label, text_color = '#6B7280', 'MISSING', 'white'
            elif cell['outcome'] == 'oom':
                color, label, text_color = '#111827', 'OOM', '#FF8577'
            elif cell['outcome'] == 'incomplete':
                color, label, text_color = '#7F1D1D', 'FAIL', 'white'
            else:
                value = cell['achieved_active_rps']
                color = cmap(norm(value))
                label = f'{value:,.0f}'
                text_color = contrasting_color(color)
            axis.add_patch(plt.Rectangle((column, row), 1, 1, facecolor=color, edgecolor=(1, 1, 1, 0.35), linewidth=0.8))
            axis.text(column + 0.5, row + 0.5, label, ha='center', va='center', color=text_color, fontsize=13, fontweight='semibold')
    axis.set_xlim(0, len(rates))
    axis.set_ylim(len(frequencies), 0)
    axis.set_xticks([index + 0.5 for index in range(len(rates))], [f'{rate:,}' for rate in rates], fontsize=12)
    axis.xaxis.tick_top()
    axis.xaxis.set_label_position('top')
    axis.set_xlabel('Target requests / second (RPS)', fontsize=15, fontweight='bold', labelpad=18)
    axis.set_yticks([index + 0.5 for index in range(len(frequencies))], [frequency_label(value) for value in frequencies], fontsize=11)
    axis.set_ylabel('Configured forced GC frequency\n(GCs / second)', fontsize=14, fontweight='bold', labelpad=20)
    axis.tick_params(length=0)
    for spine in axis.spines.values():
        spine.set_visible(False)
    fig.text(0.03, 0.95, 'BAML Throughput vs. Forced GC Frequency', fontsize=25, fontweight='bold', color='#111827')
    fig.text(0.03, 0.905, 'Achieved HTTP 200 responses/sec during active windows (60s per cell, 80% duty cycle — 4s on, 1s off)', fontsize=15, color='#52647F')
    color_axis = fig.add_axes([0.875, 0.29, 0.025, 0.49])
    colorbar = fig.colorbar(matplotlib.cm.ScalarMappable(norm=norm, cmap=cmap), cax=color_axis)
    colorbar.outline.set_visible(False)
    colorbar.ax.tick_params(labelsize=10)
    fig.text(0.855, 0.805, 'Achieved RPS', fontsize=12, fontweight='bold')
    fig.legend(handles=[Patch(facecolor='#111827', edgecolor='#111827', label='OOM'), Patch(facecolor='#7F1D1D', edgecolor='#7F1D1D', label='Incomplete')], loc='center left', bbox_to_anchor=(0.855, 0.235), frameon=False, fontsize=11)
    footer = plt.Rectangle((0.025, 0.035), 0.93, 0.12, transform=fig.transFigure, facecolor='#F8FAFC', edgecolor='#D6DEE8', linewidth=0.8)
    fig.patches.append(footer)
    fig.text(0.045, 0.126, 'Environment', fontsize=11, fontweight='bold', color='#172033')
    fig.text(0.045, 0.101, 'ARM64 c7g.medium host · 1-vCPU / 1-GiB ECS task', fontsize=10, color='#52647F')
    fig.text(0.045, 0.078, 'AWS us-east-1 · same VPC load generator', fontsize=10, color='#52647F')
    fig.text(0.37, 0.126, 'Cell value', fontsize=11, fontweight='bold', color='#172033')
    fig.text(0.37, 0.101, 'HTTP 200 / 48 active seconds', fontsize=10, color='#52647F')
    fig.text(0.37, 0.078, 'Configured target applies only during active windows', fontsize=10, color='#52647F')
    fig.text(0.65, 0.126, 'Notes', fontsize=11, fontweight='bold', color='#172033')
    fig.text(0.65, 0.101, 'Each cell begins after a successful health check + cleanup GC', fontsize=10, color='#52647F')
    fig.text(0.65, 0.078, 'Periodic GC calls do not overlap; missed ticks are recorded', fontsize=10, color='#52647F')
    fig.savefig(output, dpi=180, bbox_inches='tight', facecolor='white')
    plt.close(fig)


def write_csv(data, output):
    fields = ['gc_frequency_hz', 'rate', 'outcome', 'achieved_active_rps', 'http200', 'scheduled', 'success_ratio',
              'cycles', 'planned_cycles', 'elapsed_seconds', 'target_available_at_end', 'forced_stop',
              'gc_achieved_frequency_hz', 'gc_attempted', 'gc_successful', 'gc_failed', 'gc_skipped_ticks',
              'gc_duration_ms_median', 'gc_duration_ms_max']
    with output.open('w', newline='') as handle:
        writer = csv.DictWriter(handle, fieldnames=fields)
        writer.writeheader()
        for cell in data['cells']:
            gc = cell['explicit_gc']
            writer.writerow({
                'gc_frequency_hz': cell['gc_frequency_hz'], 'rate': cell['rate'], 'outcome': cell['outcome'],
                'achieved_active_rps': cell['achieved_active_rps'], 'http200': cell['http200'],
                'scheduled': cell['scheduled'], 'success_ratio': cell['success_ratio'], 'cycles': cell['cycles'],
                'planned_cycles': cell['planned_cycles'], 'elapsed_seconds': cell['elapsed_seconds'],
                'target_available_at_end': cell['target_available_at_end'], 'forced_stop': cell['forced_stop'],
                'gc_achieved_frequency_hz': gc['achieved_frequency_hz'], 'gc_attempted': gc['attempted'],
                'gc_successful': gc['successful'], 'gc_failed': gc['failed'], 'gc_skipped_ticks': gc['skipped_ticks'],
                'gc_duration_ms_median': gc['duration_ms_median'], 'gc_duration_ms_max': gc['duration_ms_max'],
            })


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--name', required=True)
    parser.add_argument('--aws-profile', default=os.environ.get('AWS_PROFILE'))
    parser.add_argument('--region', default=os.environ.get('AWS_REGION', 'us-east-1'))
    parser.add_argument('--start-time', default='2026-09-15T00:00:00Z')
    parser.add_argument('--output-dir', type=Path, required=True)
    args = parser.parse_args()
    args.output_dir.mkdir(parents=True, exist_ok=True)
    data = collect(args.aws_profile, args.region, args.name, iso_ms(args.start_time))
    (args.output_dir / 'gc-grid-source.json').write_text(json.dumps(data, indent=2) + '\n')
    write_csv(data, args.output_dir / 'gc-grid-results.csv')
    render(data, args.output_dir / 'baml-throughput-vs-gc-frequency.png')
    print(json.dumps({'cells': len(data['cells']), 'missing_cells': data['metadata']['missing_cells'],
                      'output_dir': str(args.output_dir)}, indent=2))


if __name__ == '__main__':
    main()

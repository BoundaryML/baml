#!/usr/bin/env python3
"""Download retained CloudWatch threshold data and render the benchmark chart set."""
import argparse
import datetime as dt
import json
import math
import os
from pathlib import Path
import subprocess

import matplotlib
matplotlib.use('Agg')
import matplotlib.image as mpimg
import matplotlib.pyplot as plt
from matplotlib.ticker import FuncFormatter

BENCHMARK_COLORS = {
    'baml-only': ('#6F4EAD', '#A896CF'),
    'python-baml': ('#0072B2', '#70AED2'),
    'python-only': ('#CC79A7', '#E2AFCA'),
    'node-baml': ('#D55E00', '#EBA06B'),
    'node-only': ('#009E73', '#6DC4AA'),
}
ARCHITECTURE = 'arm64'
ON_SECONDS = 4
OFF_SECONDS = 1
BENCHMARKS = [
    {'name': 'baml-only', 'label': 'BAML only', 'run': 'hello-ramp-20260915-01', 'event': 'load_cycle_finished', 'pre': 300, 'post': 400},
    {'name': 'python-baml', 'label': 'Python + BAML', 'run': 'hello-ramp-20260915-01', 'event': 'load_cycle_finished', 'pre': 1000, 'post': 1100},
    {'name': 'python-only', 'label': 'Python only', 'run': 'hello-ramp-20260915-01', 'event': 'load_cycle_finished', 'pre': 1600, 'post': 1700},
    {'name': 'node-baml', 'label': 'Node + BAML', 'run': 'hello-ramp-20260915-01', 'event': 'load_cycle_finished', 'pre': 2600, 'post': 2700},
    {'name': 'node-only', 'label': 'Node only', 'run': 'hello-vpc-node-binary-01', 'event': 'binary_cycle_finished', 'pre': 5200, 'post': 5300},
]


def iso(timestamp_ms):
    return dt.datetime.fromtimestamp(timestamp_ms / 1000, dt.timezone.utc).isoformat().replace('+00:00', 'Z')


def aws_json(profile, region, *args):
    command = ['aws', '--no-cli-pager', '--region', region]
    if profile:
        command += ['--profile', profile]
    command += list(args) + ['--output', 'json']
    return json.loads(subprocess.check_output(command))


def load_events(profile, region, benchmark):
    group = f"/baml/hello-world-rampup/{benchmark['run']}/load"
    response = aws_json(profile, region, 'logs', 'filter-log-events', '--log-group-name', group,
                        '--start-time', str(1789500000 * 1000))
    events = []
    for item in response.get('events', []):
        try:
            message = json.loads(item['message'])
        except json.JSONDecodeError:
            continue
        if (message.get('event') == benchmark['event'] and message.get('Variant') == benchmark['name'] and
                message.get('Architecture') == ARCHITECTURE and message.get('rate') in (benchmark['pre'], benchmark['post'])):
            events.append({'cloudwatch_log_timestamp_ms': item['timestamp'], **message})
    events.sort(key=lambda item: item['cloudwatch_log_timestamp_ms'])
    return {'log_group': group, 'response_event_count': len(response.get('events', [])), 'threshold_events': events}


def metric_points(profile, region, run, service, metric, start_ms, end_ms):
    start = dt.datetime.fromtimestamp((start_ms - 120_000) / 1000, dt.timezone.utc).isoformat()
    end = dt.datetime.fromtimestamp((end_ms + 120_000) / 1000, dt.timezone.utc).isoformat()
    response = aws_json(profile, region, 'cloudwatch', 'get-metric-statistics', '--namespace', 'AWS/ECS',
                        '--metric-name', metric, '--dimensions', f'Name=ClusterName,Value={run}',
                        f'Name=ServiceName,Value={service}', '--start-time', start, '--end-time', end,
                        '--period', '60', '--statistics', 'Average', 'Maximum')
    points = sorted(response.get('Datapoints', []), key=lambda point: point['Timestamp'])
    return {'namespace': 'AWS/ECS', 'metric_name': metric, 'period_seconds': 60,
            'dimensions': {'ClusterName': run, 'ServiceName': service}, 'datapoints': points}


def selected_point(metric, midpoint_ms):
    if not metric['datapoints']:
        raise RuntimeError(f"No {metric['metric_name']} points for {metric['dimensions']}")
    target = dt.datetime.fromtimestamp(midpoint_ms / 1000, dt.timezone.utc)
    point = min(metric['datapoints'], key=lambda item: abs(dt.datetime.fromisoformat(item['Timestamp']) - target))
    return point


def summarize_phase(rate, events, cpu_metric, memory_metric):
    phase = [event for event in events if event['rate'] == rate]
    if len(phase) < 5:
        raise RuntimeError(f'Expected at least five cycles at {rate}, found {len(phase)}')
    start_ms = phase[0]['cloudwatch_log_timestamp_ms'] - (ON_SECONDS + OFF_SECONDS) * 1000
    end_ms = phase[-1]['cloudwatch_log_timestamp_ms']
    midpoint_ms = (start_ms + end_ms) // 2
    http200 = sum(int(event.get('Http200', event.get('http200', 0))) for event in phase)
    scheduled = sum(int(event['scheduled']) for event in phase)
    requests = sum(int(event.get('Requests', event.get('requests', 0))) for event in phase)
    return {
        'configured_active_rps': rate,
        'cycle_count': len(phase),
        'window': {'start': iso(start_ms), 'end': iso(end_ms), 'midpoint': iso(midpoint_ms)},
        'scheduled_requests': scheduled,
        'recorded_requests': requests,
        'http200': http200,
        'completion_ratio': http200 / scheduled,
        'successful_active_rps': http200 / (len(phase) * ON_SECONDS),
        'successful_wall_rps': http200 / (len(phase) * (ON_SECONDS + OFF_SECONDS)),
        'cpu_utilization_percent': selected_point(cpu_metric, midpoint_ms),
        'memory_utilization_percent': selected_point(memory_metric, midpoint_ms),
        'cycles': phase,
    }


def collect(profile, region):
    output = {'metadata': {
        'generated_at': dt.datetime.now(dt.timezone.utc).isoformat(), 'aws_profile': profile, 'region': region,
        'architecture': ARCHITECTURE, 'host_instance_type': 'c7g.medium', 'task_cpu_vcpu': 1,
        'task_memory_gib': 1, 'on_seconds': ON_SECONDS, 'off_seconds': OFF_SECONDS,
        'benchmark_colors': {name: {'passing_target': colors[0], 'failing_target': colors[1]}
                             for name, colors in BENCHMARK_COLORS.items()},
        'metric_granularity_note': 'AWS/ECS CPU and memory points have 60-second granularity; each 30-second threshold phase uses the nearest point to its midpoint and can blend an adjacent phase.',
    }, 'benchmarks': {}}
    for benchmark in BENCHMARKS:
        evidence = load_events(profile, region, benchmark)
        events = evidence['threshold_events']
        all_times = [event['cloudwatch_log_timestamp_ms'] for event in events]
        if not all_times:
            raise RuntimeError(f"No threshold events for {benchmark['name']}")
        service = f"{benchmark['run']}-{benchmark['name']}-{ARCHITECTURE}"
        cpu = metric_points(profile, region, benchmark['run'], service, 'CPUUtilization', min(all_times), max(all_times))
        memory = metric_points(profile, region, benchmark['run'], service, 'MemoryUtilization', min(all_times), max(all_times))
        output['benchmarks'][benchmark['name']] = {
            'label': benchmark['label'], 'run': benchmark['run'], 'service': service,
            'cloudwatch_logs': evidence, 'cloudwatch_metrics': {'cpu': cpu, 'memory': memory},
            'pre': summarize_phase(benchmark['pre'], events, cpu, memory),
            'post': summarize_phase(benchmark['post'], events, cpu, memory),
        }
    return output


def style():
    plt.rcParams.update({
        'font.family': 'DejaVu Sans', 'font.size': 11, 'axes.titlesize': 15, 'axes.labelsize': 11,
        'axes.edgecolor': '#333333', 'axes.linewidth': 0.8, 'axes.grid': True, 'axes.axisbelow': True,
        'grid.color': '#E6E6E6', 'grid.linewidth': 0.8, 'figure.facecolor': 'white', 'axes.facecolor': 'white',
    })


def value_label(value, unit):
    if unit == 'RPS':
        return f'{value:,.0f}'
    if unit == 'CPU':
        return f'{value:.2f}'
    return f'{value:.2f} GiB'


def chart_values(data):
    result = {}
    for item in BENCHMARKS:
        name = item['name']
        benchmark = data['benchmarks'][name]
        result[name] = {}
        for phase_name in ('pre', 'post'):
            phase = benchmark[phase_name]
            result[name][phase_name] = {
                'rps': phase['successful_active_rps'],
                'cpu': phase['cpu_utilization_percent']['Maximum'] / 100 * data['metadata']['task_cpu_vcpu'],
                'memory': phase['memory_utilization_percent']['Maximum'] / 100 * data['metadata']['task_memory_gib'],
            }
    return result


def shared_axis_limits(values):
    maximum_rps = max(values[name][phase]['rps'] for name in values for phase in ('pre', 'post'))
    return {
        'rps': max(500, math.ceil(maximum_rps * 1.12 / 500) * 500),
        'cpu': 1.0,
        'memory': 1.0,
    }


def two_bar_chart(path, title, ylabel, values, labels, colors, note, unit, ylim):
    fig, ax = plt.subplots(figsize=(6.4, 4.5))
    bars = ax.bar(labels, values, color=colors, width=0.58)
    ax.set_ylim(0, ylim)
    ax.set_ylabel(ylabel)
    ax.set_title(title, loc='left', fontweight='bold')
    ax.spines[['top', 'right']].set_visible(False)
    ax.grid(axis='x', visible=False)
    for bar, value in zip(bars, values):
        near_ceiling = value > ylim * 0.88
        label_y = value - ylim * 0.025 if near_ceiling else value + ylim * 0.018
        red, green, blue, unused = bar.get_facecolor()
        luminance = 0.2126 * red + 0.7152 * green + 0.0722 * blue
        ax.text(bar.get_x() + bar.get_width() / 2, label_y, value_label(value, unit),
                ha='center', va='top' if near_ceiling else 'bottom',
                color='white' if near_ceiling and luminance < 0.55 else '#111111', fontweight='bold')
    fig.text(0.5, 0.015, note, ha='center', va='bottom', fontsize=8.5, color='#555555')
    fig.tight_layout(rect=(0, 0.06, 1, 1))
    fig.savefig(path, dpi=200, bbox_inches='tight')
    plt.close(fig)


def render_individual(data, values, limits, output_dir):
    paths = []
    for item in BENCHMARKS:
        name = item['name']
        benchmark = data['benchmarks'][name]
        pre, post = benchmark['pre'], benchmark['post']
        phase_labels = [f"{pre['configured_active_rps']:,} RPS\ntarget", f"{post['configured_active_rps']:,} RPS\ntarget"]
        rps_path = output_dir / f'{name}-rps.png'
        two_bar_chart(rps_path, f"{benchmark['label']} — successful RPS", 'HTTP 200 / active second',
                      [values[name]['pre']['rps'], values[name]['post']['rps']], phase_labels, BENCHMARK_COLORS[name],
                      f"Aggregate across six 4-second active cycles · completion: {pre['configured_active_rps']:,} RPS {pre['completion_ratio']:.2%}, {post['configured_active_rps']:,} RPS {post['completion_ratio']:.2%}", 'RPS', limits['rps'])
        paths.append(rps_path)
        cpu_path = output_dir / f'{name}-cpu.png'
        two_bar_chart(cpu_path, f"{benchmark['label']} — CPU at threshold", 'CPU',
                      [values[name]['pre']['cpu'], values[name]['post']['cpu']], phase_labels, BENCHMARK_COLORS[name],
                      'Maximum of nearest 60-second AWS/ECS sample; 1.0 is the full task allocation', 'CPU', limits['cpu'])
        paths.append(cpu_path)
        memory_path = output_dir / f'{name}-memory.png'
        two_bar_chart(memory_path, f"{benchmark['label']} — memory at threshold", 'Memory (GiB)',
                      [values[name]['pre']['memory'], values[name]['post']['memory']], phase_labels, BENCHMARK_COLORS[name],
                      'Maximum of nearest 60-second AWS/ECS sample; 1-GiB container limit', 'GiB', limits['memory'])
        paths.append(memory_path)
    return paths


def render_overview(data, limits, output_dir):
    path = output_dir / 'max-sustained-rps.png'
    names = [data['benchmarks'][item['name']]['label'] for item in BENCHMARKS]
    values = [data['benchmarks'][item['name']]['pre']['configured_active_rps'] for item in BENCHMARKS]
    fig, ax = plt.subplots(figsize=(12, 5.3))
    bars = ax.bar(names, values, color=[BENCHMARK_COLORS[item['name']][0] for item in BENCHMARKS], width=0.62)
    ax.set_ylabel('Max sustained active-window RPS')
    fig.text(0.08, 0.965, 'Max sustained RPS — ARM64, 1-vCPU / 1-GiB ECS task',
             ha='left', va='top', fontsize=18, fontweight='bold')
    fig.text(0.08, 0.915, 'c7g.medium host · six 4s-on/1s-off cycles per threshold candidate',
             ha='left', va='top', fontsize=10, color='#555555')
    ax.spines[['top', 'right']].set_visible(False)
    ax.yaxis.set_major_formatter(FuncFormatter(lambda value, unused: f'{value:,.0f}'))
    ax.set_ylim(0, limits['rps'])
    for bar, value in zip(bars, values):
        ax.text(bar.get_x() + bar.get_width() / 2, value + limits['rps'] * 0.018,
                f'{value:,.0f}', va='bottom', ha='center', fontweight='bold')
    fig.text(0.5, 0.012, 'These retained CloudWatch runs were not performed on t4g.micro.',
             ha='center', va='bottom', fontsize=9, color='#555555')
    fig.tight_layout(rect=(0, 0.05, 1, 0.87))
    fig.savefig(path, dpi=200, bbox_inches='tight')
    plt.close(fig)
    return path


def render_contact_sheet(overview, chart_paths, output_dir):
    path = output_dir / 'all-threshold-charts.png'
    if len(chart_paths) != len(BENCHMARKS) * 3:
        raise ValueError(f'Expected three charts for each of {len(BENCHMARKS)} benchmarks')
    fig = plt.figure(figsize=(20, 27), facecolor='white')
    grid = fig.add_gridspec(6, 3, height_ratios=[1.05, 1, 1, 1, 1, 1])
    overview_axis = fig.add_subplot(grid[0, :])
    overview_axis.imshow(mpimg.imread(overview))
    overview_axis.axis('off')
    for row in range(len(BENCHMARKS)):
        for column in range(3):
            axis = fig.add_subplot(grid[row + 1, column])
            axis.imshow(mpimg.imread(chart_paths[row * 3 + column]))
            axis.axis('off')
    fig.subplots_adjust(left=0.01, right=0.99, bottom=0.005, top=0.995, hspace=0.02, wspace=0.01)
    fig.savefig(path, dpi=140, bbox_inches='tight')
    plt.close(fig)
    return path


def write_index(data, values, chart_paths, overview, contact_sheet, output_dir):
    lines = ['# Hello-world ARM64 threshold charts', '', 'Source: retained passing/failing threshold events from CloudWatch Logs and AWS/ECS metrics from the 2026-09-15 ramp and in-VPC binary-search runs.', '', 'The source hosts were c7g.medium, while each measured ECS task had a hard 1-vCPU and 1-GiB allocation. These are not t4g.micro measurements.', '', 'Color identifies the implementation across every chart. The darker shade is the passing target and the lighter shade is the first failing target.', '', f'![Overview]({overview.name})', '', f'![All threshold charts]({contact_sheet.name})', '']
    lines += ['## Selected CloudWatch values', '', '| Benchmark | Passing target RPS | Failing target RPS | Passing successful RPS | Failing successful RPS | Passing CPU max (vCPU) | Failing CPU max (vCPU) | Passing memory max (GiB) | Failing memory max (GiB) |', '| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |']
    for benchmark in BENCHMARKS:
        name = benchmark['name']
        value = data['benchmarks'][name]
        pre, post = value['pre'], value['post']
        lines.append(f"| {value['label']} | {pre['configured_active_rps']:,} | {post['configured_active_rps']:,} | {pre['successful_active_rps']:,.1f} | {post['successful_active_rps']:,.1f} | {values[name]['pre']['cpu']:.2f} | {values[name]['post']['cpu']:.2f} | {values[name]['pre']['memory']:.2f} | {values[name]['post']['memory']:.2f} |")
    lines += ['', 'The failing-target memory drops for BAML-only and Node+BAML reflect OOM task replacement near the selected 60-second sample; they do not indicate successful memory recovery within one uninterrupted task.', '']
    for benchmark in BENCHMARKS:
        name = benchmark['name']
        label = data['benchmarks'][name]['label']
        lines += [f'## {label}', '', f'![{label} RPS]({name}-rps.png)', '', f'![{label} CPU]({name}-cpu.png)', '', f'![{label} memory]({name}-memory.png)', '']
    lines += ['## Method note', '', 'AWS/ECS CPU and memory points have 60-second granularity; each 30-second threshold phase uses the nearest point to its midpoint and can blend an adjacent phase. RPS is aggregated from the six four-second active cycles in each phase.', '']
    (output_dir / 'README.md').write_text('\n'.join(lines))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--aws-profile', default=os.environ.get('AWS_PROFILE'))
    parser.add_argument('--region', default=os.environ.get('AWS_REGION', 'us-east-1'))
    parser.add_argument('--output-dir', type=Path, required=True)
    parser.add_argument('--input-json', type=Path, help='Render previously downloaded data without querying AWS')
    args = parser.parse_args()
    args.output_dir.mkdir(parents=True, exist_ok=True)
    style()
    if args.input_json:
        data = json.loads(args.input_json.read_text())
    else:
        data = collect(args.aws_profile, args.region)
    data_path = args.output_dir / 'cloudwatch-threshold-data.json'
    data_path.write_text(json.dumps(data, indent=2) + '\n')
    values = chart_values(data)
    limits = shared_axis_limits(values)
    chart_paths = render_individual(data, values, limits, args.output_dir)
    overview = render_overview(data, limits, args.output_dir)
    contact_sheet = render_contact_sheet(overview, chart_paths, args.output_dir)
    write_index(data, values, chart_paths, overview, contact_sheet, args.output_dir)
    print(json.dumps({'data': str(data_path), 'charts': [str(path) for path in chart_paths],
                      'overview': str(overview), 'contact_sheet': str(contact_sheet)}, indent=2))


if __name__ == '__main__':
    main()

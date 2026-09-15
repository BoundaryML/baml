"""Run repeated bounded Vegeta attacks with a measurable 4-on/1-off ramp."""
import json
import math
import os
from pathlib import Path
import signal
import subprocess
import threading
import time
import urllib.request

NAMESPACE = 'BAML/HelloWorldRampup'
COUNTERS = ('Requests', 'Http200', 'TransportErrors', 'HttpErrors', 'BodyMismatches')


def emf(dimensions, values, units, timestamp=None):
    for value in values.values():
        samples = value if isinstance(value, list) else [value]
        if not 1 <= len(samples) <= 100 or not all(math.isfinite(n) and n >= 0 for n in samples):
            raise ValueError('EMF metrics require 1..100 finite nonnegative samples')
    return dict(dimensions, **values, _aws={
        'Timestamp': int((time.time() if timestamp is None else timestamp) * 1000),
        'CloudWatchMetrics': [{'Namespace': NAMESPACE, 'Dimensions': [list(dimensions)],
                              'Metrics': [{'Name': key, 'Unit': units[key], 'StorageResolution': 1}
                                          for key in values]}]})


def rate_at(start, increase, every, maximum, elapsed):
    return min(maximum, start + increase * int(elapsed // every))


def env_int(name, default, minimum, maximum):
    value = int(os.environ.get(name, default))
    if not minimum <= value <= maximum:
        raise ValueError(f'{name} must be {minimum}..{maximum}')
    return value


def summarize_report(report, scheduled, body_mismatch):
    requests = int(report.get('requests', 0))
    status_codes = {str(key): int(value) for key, value in report.get('status_codes', {}).items()}
    responses = sum(status_codes.values())
    http200 = status_codes.get('200', 0)
    transport_errors = max(0, requests - responses)
    http_errors = sum(value for code, value in status_codes.items() if code != '200')
    latencies = report.get('latencies', {})
    values = {
        'ScheduledRequests': scheduled,
        'Requests': requests,
        'Http200': http200,
        'TransportErrors': transport_errors,
        'HttpErrors': http_errors,
        'BodyMismatches': int(body_mismatch),
    }
    for source, target in [('50th', 'LatencyP50Ms'), ('90th', 'LatencyP90Ms'), ('99th', 'LatencyP99Ms'), ('max', 'LatencyMaxMs')]:
        if source in latencies:
            values[target] = int(latencies[source]) / 1_000_000
    return values


class Collector:
    def __init__(self, dimensions, output=print):
        self.dimensions = dimensions
        self.output = output
        self.lock = threading.Lock()

    def emit(self, values, units):
        with self.lock:
            self.output(json.dumps(emf(self.dimensions, values, units), separators=(',', ':')))

    def scrape(self, url):
        try:
            with urllib.request.urlopen(url, timeout=3) as response:
                text = response.read(65536).decode()
            names = {'process_resident_memory_bytes': 'ProcessRssBytes',
                     'nodejs_heap_size_used_bytes': 'NodeHeapBytes',
                     'python_traced_memory_current_bytes': 'PythonTracedBytes'}
            values = {}
            for line in text.splitlines():
                parts = line.split()
                if len(parts) == 2 and parts[0] in names:
                    value = float(parts[1])
                    if math.isfinite(value) and value >= 0:
                        values[names[parts[0]]] = value
            if not values:
                raise ValueError('No process metrics')
            self.emit(values, dict.fromkeys(values, 'Bytes'))
            self.emit({'ProcessMetricsUp': 1}, {'ProcessMetricsUp': 'Count'})
        except (OSError, ValueError):
            self.emit({'ProcessMetricsUp': 0}, {'ProcessMetricsUp': 'Count'})


def probe_body(url):
    try:
        with urllib.request.urlopen(url, timeout=1) as response:
            return response.status != 200 or response.read(32) != b'hello world', True
    except OSError:
        return False, False


def main():
    dimensions = {key: os.environ[key] for key in ('RunName', 'Variant', 'Architecture')}
    start_rate = env_int('START_RATE_PER_TARGET', '100', 1, 50000)
    rate_step = env_int('RATE_STEP_PER_TARGET', '100', 0, 50000)
    step_seconds = env_int('RATE_STEP_SECONDS', '30', 5, 3600)
    maximum_rate = env_int('MAX_RATE_PER_TARGET', '25000', start_rate, 50000)
    on_seconds = env_int('ON_SECONDS', '4', 1, 60)
    off_seconds = env_int('OFF_SECONDS', '1', 1, 60)
    if on_seconds != 4 or off_seconds != 1:
        raise ValueError('This benchmark requires a 4-second-on, 1-second-off duty cycle')
    if step_seconds % (on_seconds + off_seconds):
        raise ValueError('RATE_STEP_SECONDS must be a multiple of the duty-cycle duration')
    url = os.environ['TARGET_URL']
    if not url.startswith('http://') or '\n' in url:
        raise ValueError('Expected a single private HTTP target')
    Path('/tmp/target.txt').write_text('GET ' + url + '\n')
    collector = Collector(dimensions, lambda line: print(line, flush=True))
    stop = threading.Event()
    children_lock = threading.Lock()
    children = []

    def interrupt(*unused):
        stop.set()
        with children_lock:
            active_children = list(children)
        for child in active_children:
            if child.poll() is None:
                child.send_signal(signal.SIGINT)

    signal.signal(signal.SIGTERM, interrupt)
    signal.signal(signal.SIGINT, interrupt)

    def scrape_process():
        while not stop.wait(5):
            if os.environ.get('PROCESS_METRICS_URL'):
                collector.scrape(os.environ['PROCESS_METRICS_URL'])

    scraper = threading.Thread(target=scrape_process, daemon=True)
    scraper.start()
    benchmark_started = time.monotonic()
    cycle = 0
    print(json.dumps(dict(dimensions, event='load_started', start_rate=start_rate, rate_step=rate_step,
                          step_seconds=step_seconds, maximum_rate=maximum_rate, on_seconds=on_seconds,
                          off_seconds=off_seconds, target=url)), flush=True)
    try:
        while not stop.is_set():
            cycle_started = time.monotonic()
            elapsed = cycle_started - benchmark_started
            step = int(elapsed // step_seconds)
            rate = rate_at(start_rate, rate_step, step_seconds, maximum_rate, elapsed)
            scheduled = rate * on_seconds
            print(json.dumps(dict(dimensions, event='load_cycle_started', cycle=cycle, ramp_step=step,
                                  rate=rate, scheduled=scheduled)), flush=True)
            attack = subprocess.Popen(['vegeta', 'attack', '-targets=/tmp/target.txt', f'-rate={rate}/1s',
                                       f'-duration={on_seconds}s', '-timeout=10s', '-dns-ttl=1s', '-workers=10',
                                       '-max-workers=50000', '-connections=100', '-max-connections=50000',
                                       '-keepalive=true', '-http2=false', '-redirects=0', '-max-body=32'],
                                      stdout=subprocess.PIPE)
            reporter = subprocess.Popen(['vegeta', 'report', '-type=json'], stdin=attack.stdout, stdout=subprocess.PIPE)
            attack.stdout.close()
            with children_lock:
                children[:] = [attack, reporter]
            stop.wait(max(0, cycle_started + on_seconds + off_seconds - time.monotonic()))
            forced_stop = False
            for child in (attack, reporter):
                if child.poll() is None:
                    forced_stop = True
                    child.kill()
            for child in (attack, reporter):
                try:
                    child.wait(timeout=2)
                except subprocess.TimeoutExpired:
                    child.kill()
                    child.wait()
            report_bytes = reporter.stdout.read()
            with children_lock:
                children.clear()
            try:
                report = json.loads(report_bytes) if report_bytes else {}
                report_error = None
            except json.JSONDecodeError as error:
                report = {}
                report_error = repr(error)
            body_mismatch, probe_up = probe_body(url)
            values = summarize_report(report, scheduled, body_mismatch)
            values.update(TargetRps=rate, RampStep=step, ProbeUp=int(probe_up), ForcedStop=int(forced_stop))
            units = dict.fromkeys(values, 'Count')
            units.update(TargetRps='Count/Second')
            for key in ('LatencyP50Ms', 'LatencyP90Ms', 'LatencyP99Ms', 'LatencyMaxMs'):
                if key in values:
                    units[key] = 'Milliseconds'
            collector.emit(values, units)
            counts = {key: values[key] for key in COUNTERS}
            completion_ratio = counts['Http200'] / scheduled
            print(json.dumps(dict(dimensions, event='load_cycle_finished', cycle=cycle, ramp_step=step,
                                  rate=rate, scheduled=scheduled, completion_ratio=completion_ratio,
                                  forced_stop=forced_stop, probe_up=probe_up, report_error=report_error,
                                  vegeta_errors=report.get('errors', []), **counts)), flush=True)
            cycle += 1
    finally:
        stop.set()
        with children_lock:
            active_children = list(children)
            children.clear()
        for child in active_children:
            if child.poll() is None:
                child.kill()
                child.wait()
        scraper.join(timeout=5)
        print(json.dumps(dict(dimensions, event='load_stopped')), flush=True)


if __name__ == '__main__':
    main()

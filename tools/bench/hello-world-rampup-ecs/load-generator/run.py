"""Run repeated bounded Vegeta attacks with a measurable 4-on/1-off ramp."""
import json
import math
import os
from pathlib import Path
import signal
import statistics
import subprocess
import threading
import time
import urllib.request

NAMESPACE = 'BAML/HelloWorldRampup'
COUNTERS = ('Requests', 'Http200', 'TransportErrors', 'HttpErrors', 'BodyMismatches')
OUTPUT_LOCK = threading.Lock()


def output_line(line):
    with OUTPUT_LOCK:
        print(line, flush=True)


def output_json(value):
    output_line(json.dumps(value))


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


def env_number_list(name, minimum, maximum):
    values = json.loads(os.environ[name])
    if (not isinstance(values, list) or not values or any(isinstance(value, bool) or not isinstance(value, (int, float)) or
            not math.isfinite(value) or value < minimum or value > maximum for value in values)):
        raise ValueError(f'{name} must be a non-empty JSON array of numbers from {minimum}..{maximum}')
    if len(set(values)) != len(values) or any(value <= values[index - 1] for index, value in enumerate(values) if index):
        raise ValueError(f'{name} must be strictly increasing with no duplicates')
    return values


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


def request_explicit_gc(url):
    started = time.monotonic()
    try:
        with urllib.request.urlopen(url, timeout=10) as response:
            body = response.read(32)
            status = response.status
        return {'ok': status == 200 and body == b'ok', 'status': status,
                'duration_ms': (time.monotonic() - started) * 1000, 'error': None}
    except OSError as error:
        return {'ok': False, 'status': None, 'duration_ms': (time.monotonic() - started) * 1000,
                'error': repr(error)}


def finish_attack(attack, reporter, grace_seconds):
    deadline = time.monotonic() + grace_seconds
    forced_stop = False
    for child in (attack, reporter):
        try:
            child.wait(timeout=max(0, deadline - time.monotonic()))
        except subprocess.TimeoutExpired:
            forced_stop = True
            child.kill()
            child.wait()
    return forced_stop


def wait_until_ready(url, gc_url, stop, timeout_seconds=300):
    started = time.monotonic()
    last_gc = None
    while not stop.is_set() and time.monotonic() - started < timeout_seconds:
        body_mismatch, probe_up = probe_body(url)
        if probe_up and not body_mismatch:
            last_gc = request_explicit_gc(gc_url)
            if last_gc['ok']:
                stop.wait(1)
                return {'ok': True, 'duration_ms': (time.monotonic() - started) * 1000,
                        'last_gc': last_gc}
        stop.wait(2)
    return {'ok': False, 'duration_ms': (time.monotonic() - started) * 1000, 'last_gc': last_gc}


def gc_summary(results, skipped, frequency_hz, elapsed_seconds):
    durations = sorted(result['duration_ms'] for result in results)
    successful = sum(result['ok'] for result in results)
    return {
        'configured_frequency_hz': frequency_hz,
        'attempted': len(results),
        'successful': successful,
        'failed': len(results) - successful,
        'skipped_ticks': skipped,
        'achieved_frequency_hz': successful / elapsed_seconds if elapsed_seconds else 0,
        'duration_ms_min': durations[0] if durations else None,
        'duration_ms_median': statistics.median(durations) if durations else None,
        'duration_ms_max': durations[-1] if durations else None,
    }


def run_gc_grid(url, target_path, collector, stop, children, children_lock, on_seconds, off_seconds):
    rates = [int(rate) for rate in env_number_list('GC_GRID_RATES_JSON', 1, 50000)]
    frequencies = env_number_list('GC_GRID_FREQUENCIES_JSON', 0, 10)
    seconds = env_int('GC_GRID_SECONDS_PER_CELL', '60', 5, 3600)
    connections = env_int('VEGETA_CONNECTIONS', '1000', 1, 10000)
    timeout_ms = env_int('REQUEST_TIMEOUT_MS', '800', 100, 999)
    explicit_gc_url = os.environ['EXPLICIT_GC_URL']
    if seconds % (on_seconds + off_seconds):
        raise ValueError('GC_GRID_SECONDS_PER_CELL must be a multiple of the duty-cycle duration')
    planned_cycles = seconds // (on_seconds + off_seconds)
    dimensions = collector.dimensions
    output_json(dict(dimensions, event='gc_grid_started', rates_rps=rates, gc_frequencies_hz=frequencies,
                     seconds_per_cell=seconds, planned_cycles=planned_cycles, connections=connections,
                     request_timeout_ms=timeout_ms))
    for frequency_hz in frequencies:
        for rate in rates:
            if stop.is_set():
                break
            readiness = wait_until_ready(url, explicit_gc_url, stop)
            output_json(dict(dimensions, event='gc_grid_cell_target_ready', rate=rate,
                             gc_frequency_hz=frequency_hz, **readiness))
            if not readiness['ok']:
                raise RuntimeError(f'target did not recover before testing {rate} RPS at {frequency_hz} GC/s')
            wall_started = time.time()
            started = time.monotonic()
            cell_stop = threading.Event()
            gc_results = []
            gc_skipped = [0]

            def collect_periodically():
                if frequency_hz == 0:
                    return
                interval = 1 / frequency_hz
                next_due = started
                while not stop.is_set() and not cell_stop.is_set() and next_due < started + seconds:
                    if cell_stop.wait(max(0, next_due - time.monotonic())):
                        break
                    result = request_explicit_gc(explicit_gc_url)
                    gc_results.append(result)
                    output_json(dict(dimensions, event='gc_grid_gc_finished', rate=rate,
                                     gc_frequency_hz=frequency_hz, sequence=len(gc_results) - 1,
                                     **result))
                    collector.emit({'ExplicitGcUp': int(result['ok']), 'ExplicitGcDurationMs': result['duration_ms'],
                                    'ConfiguredGcFrequencyHz': frequency_hz},
                                   {'ExplicitGcUp': 'Count', 'ExplicitGcDurationMs': 'Milliseconds',
                                    'ConfiguredGcFrequencyHz': 'Count/Second'})
                    next_due += interval
                    now = time.monotonic()
                    if next_due < now:
                        missed = math.floor((now - next_due) / interval) + 1
                        gc_skipped[0] += missed
                        next_due += missed * interval

            gc_thread = threading.Thread(target=collect_periodically, daemon=True)
            gc_thread.start()
            cycles = []
            output_json(dict(dimensions, event='gc_grid_cell_started', rate=rate,
                             gc_frequency_hz=frequency_hz, planned_cycles=planned_cycles,
                             started_at_unix_ms=int(wall_started * 1000)))
            for cycle in range(planned_cycles):
                if stop.is_set():
                    break
                cycle_started = time.monotonic()
                scheduled = rate * on_seconds
                attack = subprocess.Popen(['vegeta', 'attack', f'-targets={target_path}', f'-rate={rate}/1s',
                                           f'-duration={on_seconds}s', f'-timeout={timeout_ms}ms', '-dns-ttl=1s',
                                           f'-workers={connections}', f'-max-workers={connections}',
                                           f'-connections={connections}', f'-max-connections={connections}',
                                           '-keepalive=true', '-http2=false', '-redirects=0', '-max-body=32'],
                                          stdout=subprocess.PIPE)
                reporter = subprocess.Popen(['vegeta', 'report', '-type=json'], stdin=attack.stdout, stdout=subprocess.PIPE)
                attack.stdout.close()
                with children_lock:
                    children[:] = [attack, reporter]
                stop.wait(max(0, cycle_started + on_seconds + off_seconds - time.monotonic()))
                forced_stop = finish_attack(attack, reporter, 0)
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
                values.update(TargetRps=rate, RampStep=0, ProbeUp=int(probe_up), ForcedStop=int(forced_stop),
                              ConfiguredGcFrequencyHz=frequency_hz)
                units = dict.fromkeys(values, 'Count')
                units.update(TargetRps='Count/Second', ConfiguredGcFrequencyHz='Count/Second')
                for key in ('LatencyP50Ms', 'LatencyP90Ms', 'LatencyP99Ms', 'LatencyMaxMs'):
                    if key in values:
                        units[key] = 'Milliseconds'
                collector.emit(values, units)
                record = dict(rate=rate, gc_frequency_hz=frequency_hz, cycle=cycle, scheduled=scheduled,
                              requests=values['Requests'], http200=values['Http200'],
                              success_ratio=values['Http200'] / scheduled, forced_stop=forced_stop,
                              probe_up=probe_up, body_mismatch=body_mismatch, report_error=report_error,
                              vegeta_errors=report.get('errors', []))
                cycles.append(record)
                output_json(dict(dimensions, event='gc_grid_cycle_finished', **record))
                if not probe_up:
                    break
            cell_stop.set()
            gc_thread.join(timeout=12)
            elapsed_seconds = time.monotonic() - started
            gc = gc_summary(gc_results, gc_skipped[0], frequency_hz, elapsed_seconds)
            scheduled = sum(record['scheduled'] for record in cycles)
            requests = sum(record['requests'] for record in cycles)
            http200 = sum(record['http200'] for record in cycles)
            summary = dict(rate=rate, gc_frequency_hz=frequency_hz, cycles=len(cycles),
                           planned_cycles=planned_cycles, completed_full_duration=len(cycles) == planned_cycles,
                           scheduled=scheduled, requests=requests, http200=http200,
                           achieved_active_rps=http200 / (len(cycles) * on_seconds) if cycles else 0,
                           scheduled_ratio=requests / scheduled if scheduled else 0,
                           success_ratio=http200 / scheduled if scheduled else 0,
                           minimum_cycle_success_ratio=min((record['success_ratio'] for record in cycles), default=0),
                           target_available_at_end=bool(cycles and cycles[-1]['probe_up']),
                           forced_stop=any(record['forced_stop'] for record in cycles),
                           elapsed_seconds=elapsed_seconds, started_at_unix_ms=int(wall_started * 1000),
                           ended_at_unix_ms=int(time.time() * 1000), explicit_gc=gc)
            output_json(dict(dimensions, event='gc_grid_cell_finished', **summary))
        if stop.is_set():
            break
    output_json(dict(dimensions, event='gc_grid_finished'))
    while not stop.wait(60):
        pass


def run_binary_search(url, target_path, collector, stop, children, children_lock, on_seconds, off_seconds):
    lower = env_int('SEARCH_LOWER_RPS', '5000', 1, 50000)
    upper = env_int('SEARCH_UPPER_RPS', '10000', lower + 1, 50000)
    resolution = env_int('SEARCH_RESOLUTION_RPS', '100', 1, upper - lower)
    seconds = env_int('SEARCH_SECONDS_PER_CANDIDATE', '30', 5, 3600)
    connections = env_int('VEGETA_CONNECTIONS', '1000', 1, 10000)
    timeout_ms = env_int('REQUEST_TIMEOUT_MS', '800', 100, 999)
    explicit_gc_url = os.environ.get('EXPLICIT_GC_URL')
    if seconds % (on_seconds + off_seconds):
        raise ValueError('SEARCH_SECONDS_PER_CANDIDATE must be a multiple of the duty-cycle duration')
    cycles = seconds // (on_seconds + off_seconds)
    dimensions = collector.dimensions

    def run_candidate(rate):
        records = []
        if explicit_gc_url:
            readiness = wait_until_ready(url, explicit_gc_url, stop)
            output_json(dict(dimensions, event='binary_candidate_target_ready', rate=rate, **readiness))
            if not readiness['ok']:
                raise RuntimeError(f'target did not recover before testing {rate} RPS')
        output_json(dict(dimensions, event='binary_candidate_started', rate=rate, cycles=cycles))
        for cycle in range(cycles):
            cycle_started = time.monotonic()
            scheduled = rate * on_seconds
            attack = subprocess.Popen(['vegeta', 'attack', f'-targets={target_path}', f'-rate={rate}/1s',
                                       f'-duration={on_seconds}s', f'-timeout={timeout_ms}ms', '-dns-ttl=1s',
                                       f'-workers={connections}', f'-max-workers={connections}',
                                       f'-connections={connections}', f'-max-connections={connections}',
                                       '-keepalive=true', '-http2=false', '-redirects=0', '-max-body=32'],
                                      stdout=subprocess.PIPE)
            reporter = subprocess.Popen(['vegeta', 'report', '-type=json'], stdin=attack.stdout, stdout=subprocess.PIPE)
            attack.stdout.close()
            with children_lock:
                children[:] = [attack, reporter]
            if explicit_gc_url:
                stop.wait(max(0, cycle_started + on_seconds - time.monotonic()))
                forced_stop = finish_attack(attack, reporter, timeout_ms / 1000 + 2)
            else:
                stop.wait(max(0, cycle_started + on_seconds + off_seconds - time.monotonic()))
                forced_stop = finish_attack(attack, reporter, 0)
            report_bytes = reporter.stdout.read()
            with children_lock:
                children.clear()
            try:
                report = json.loads(report_bytes) if report_bytes else {}
                report_error = None
            except json.JSONDecodeError as error:
                report = {}
                report_error = repr(error)
            gc_result = request_explicit_gc(explicit_gc_url) if explicit_gc_url else None
            if gc_result:
                output_json(dict(dimensions, event='explicit_gc_finished', rate=rate, cycle=cycle, **gc_result))
                collector.emit({'ExplicitGcUp': int(gc_result['ok']), 'ExplicitGcDurationMs': gc_result['duration_ms']},
                               {'ExplicitGcUp': 'Count', 'ExplicitGcDurationMs': 'Milliseconds'})
                stop.wait(off_seconds)
            body_mismatch, probe_up = probe_body(url)
            values = summarize_report(report, scheduled, body_mismatch)
            values.update(TargetRps=rate, RampStep=0, ProbeUp=int(probe_up), ForcedStop=int(forced_stop))
            units = dict.fromkeys(values, 'Count')
            units.update(TargetRps='Count/Second')
            for key in ('LatencyP50Ms', 'LatencyP90Ms', 'LatencyP99Ms', 'LatencyMaxMs'):
                if key in values:
                    units[key] = 'Milliseconds'
            collector.emit(values, units)
            record = dict(rate=rate, cycle=cycle, scheduled=scheduled, requests=values['Requests'],
                          http200=values['Http200'], success_ratio=values['Http200'] / scheduled,
                          scheduled_ratio=values['Requests'] / scheduled, forced_stop=forced_stop,
                          probe_up=probe_up, body_mismatch=body_mismatch, report_error=report_error,
                          explicit_gc=gc_result,
                          vegeta_errors=report.get('errors', []))
            records.append(record)
            output_json(dict(dimensions, event='binary_cycle_finished', **record))
            if explicit_gc_url and (not gc_result['ok'] or not probe_up or forced_stop):
                break
        scheduled = sum(record['scheduled'] for record in records)
        requests = sum(record['requests'] for record in records)
        http200 = sum(record['http200'] for record in records)
        passed = (len(records) == cycles and requests / scheduled >= 0.99 and http200 / scheduled >= 0.99 and
                  min(record['success_ratio'] for record in records) >= 0.95 and
                  all(record['probe_up'] and not record['body_mismatch'] and not record['forced_stop'] and
                      (record['explicit_gc'] is None or record['explicit_gc']['ok']) for record in records))
        summary = dict(rate=rate, cycles=len(records), planned_cycles=cycles, passed=passed, scheduled=scheduled, requests=requests,
                       http200=http200, scheduled_ratio=requests / scheduled, success_ratio=http200 / scheduled,
                       minimum_cycle_success_ratio=min(record['success_ratio'] for record in records))
        output_json(dict(dimensions, event='binary_candidate_finished', **summary))
        return passed

    output_json(dict(dimensions, event='binary_search_started', lower=lower, upper=upper,
                     resolution=resolution, seconds_per_candidate=seconds, connections=connections,
                     request_timeout_ms=timeout_ms, explicit_gc_url=explicit_gc_url))
    upper_passed = run_candidate(upper)
    if upper_passed:
        result = dict(status='upper_bound_passed', highest_pass=upper, lowest_fail=None)
    else:
        lower_passed = run_candidate(lower)
        if not lower_passed:
            result = dict(status='lower_bound_failed', highest_pass=None, lowest_fail=lower)
        else:
            highest_pass, lowest_fail = lower, upper
            while lowest_fail - highest_pass > resolution and not stop.is_set():
                candidate = highest_pass + max(1, ((lowest_fail - highest_pass) // resolution) // 2) * resolution
                if run_candidate(candidate):
                    highest_pass = candidate
                else:
                    lowest_fail = candidate
            result = dict(status='complete', highest_pass=highest_pass, lowest_fail=lowest_fail)
    output_json(dict(dimensions, event='binary_search_finished', resolution=resolution,
                     seconds_per_candidate=seconds, **result))
    while not stop.wait(60):
        if os.environ.get('PROCESS_METRICS_URL'):
            collector.scrape(os.environ['PROCESS_METRICS_URL'])


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
    collector = Collector(dimensions, output_line)
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
    load_mode = os.environ.get('LOAD_MODE', 'ramp')
    if load_mode in ('binary', 'gc-grid'):
        try:
            if load_mode == 'binary':
                run_binary_search(url, '/tmp/target.txt', collector, stop, children, children_lock, on_seconds, off_seconds)
            else:
                run_gc_grid(url, '/tmp/target.txt', collector, stop, children, children_lock, on_seconds, off_seconds)
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
            output_json(dict(dimensions, event='load_stopped'))
        return
    benchmark_started = time.monotonic()
    cycle = 0
    output_json(dict(dimensions, event='load_started', start_rate=start_rate, rate_step=rate_step,
                     step_seconds=step_seconds, maximum_rate=maximum_rate, on_seconds=on_seconds,
                     off_seconds=off_seconds, target=url))
    try:
        while not stop.is_set():
            cycle_started = time.monotonic()
            elapsed = cycle_started - benchmark_started
            step = int(elapsed // step_seconds)
            rate = rate_at(start_rate, rate_step, step_seconds, maximum_rate, elapsed)
            scheduled = rate * on_seconds
            output_json(dict(dimensions, event='load_cycle_started', cycle=cycle, ramp_step=step,
                             rate=rate, scheduled=scheduled))
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
            output_json(dict(dimensions, event='load_cycle_finished', cycle=cycle, ramp_step=step,
                             rate=rate, scheduled=scheduled, completion_ratio=completion_ratio,
                             forced_stop=forced_stop, probe_up=probe_up, report_error=report_error,
                             vegeta_errors=report.get('errors', []), **counts))
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
        output_json(dict(dimensions, event='load_stopped'))


if __name__ == '__main__':
    main()

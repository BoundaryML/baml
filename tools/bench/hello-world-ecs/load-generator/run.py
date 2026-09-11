"""One stock Vegeta attack; bounded CloudWatch EMF batches, no raw result files."""
import base64
import json
import math
import os
from pathlib import Path
import signal
import subprocess
import threading
import time
import urllib.request

NAMESPACE = 'BAML/HelloWorld'
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


class Collector:
    def __init__(self, dimensions, output=print):
        self.dimensions = dimensions
        self.output = output
        self.lock = threading.Lock()
        self.counts = dict.fromkeys(COUNTERS, 0)
        self.latencies = []

    def emit(self, values, units):
        self.output(json.dumps(emf(self.dimensions, values, units), separators=(',', ':')))

    def record(self, result):
        code = int(result['code'])
        latency = int(result['latency']) / 1_000_000
        if latency < 0:
            raise ValueError('Negative latency')
        # Vegeta encodes a missing response body as JSON null on transport errors.
        encoded_body = result.get('body')
        body = base64.b64decode('' if encoded_body is None else encoded_body, validate=True)
        with self.lock:
            self.counts['Requests'] += 1
            self.counts['Http200'] += int(code == 200)
            self.counts['TransportErrors'] += int(code == 0)
            self.counts['HttpErrors'] += int(code not in (0, 200))
            self.counts['BodyMismatches'] += int(code == 200 and body != b'hello world')
            self.latencies.append(latency)
            if len(self.latencies) == 100:
                self.emit({'LatencyMs': self.latencies}, {'LatencyMs': 'Milliseconds'})
                self.latencies = []

    def flush(self):
        with self.lock:
            self.emit(self.counts, dict.fromkeys(COUNTERS, 'Count'))
            self.counts = dict.fromkeys(COUNTERS, 0)
            if self.latencies:
                self.emit({'LatencyMs': self.latencies}, {'LatencyMs': 'Milliseconds'})
                self.latencies = []

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
            with self.lock:
                self.emit(values, dict.fromkeys(values, 'Bytes'))
                self.emit({'ProcessMetricsUp': 1}, {'ProcessMetricsUp': 'Count'})
        except (OSError, ValueError):
            with self.lock:
                self.emit({'ProcessMetricsUp': 0}, {'ProcessMetricsUp': 'Count'})


def main():
    dimensions = {key: os.environ[key] for key in ('RunName', 'Variant', 'Architecture')}
    rate = int(os.environ.get('RATE_PER_TARGET', '100'))
    if not 1 <= rate <= 10000:
        raise ValueError('RATE_PER_TARGET must be 1..10000')
    url = os.environ['TARGET_URL']
    if not url.startswith('http://') or '\n' in url:
        raise ValueError('Expected a single private HTTP target')
    Path('/tmp/target.txt').write_text('GET ' + url + '\n')
    collector = Collector(dimensions, lambda line: print(line, flush=True))
    stop = threading.Event()
    attack = subprocess.Popen(['vegeta', 'attack', '-targets=/tmp/target.txt', f'-rate={rate}/1s',
                               '-duration=0s', '-timeout=10s', '-dns-ttl=1s', '-workers=10',
                               '-max-workers=2000', '-connections=100', '-max-connections=2000',
                               '-keepalive=true', '-http2=false', '-redirects=0', '-max-body=32'],
                              stdout=subprocess.PIPE)
    encoder = None
    reporter = None

    def interrupt(*unused):
        stop.set()
        if attack.poll() is None:
            attack.send_signal(signal.SIGINT)
        # Bound shutdown even when a child fails to close its pipe.
        def kill_children():
            for child in (attack, encoder):
                if child is not None and child.poll() is None:
                    child.kill()
        timer = threading.Timer(15, kill_children)
        timer.daemon = True
        timer.start()

    signal.signal(signal.SIGTERM, interrupt)
    signal.signal(signal.SIGINT, interrupt)

    def report():
        while not stop.wait(10):
            collector.flush()
            if os.environ.get('PROCESS_METRICS_URL'):
                collector.scrape(os.environ['PROCESS_METRICS_URL'])

    try:
        encoder = subprocess.Popen(['vegeta', 'encode', '-to=json'], stdin=attack.stdout, stdout=subprocess.PIPE)
        attack.stdout.close()
        reporter = threading.Thread(target=report, daemon=True)
        reporter.start()
        print(json.dumps(dict(dimensions, event='load_started', rate=rate, target=url)), flush=True)
        for line in encoder.stdout:
            collector.record(json.loads(line))
        code = encoder.wait(timeout=15)
        attack_code = attack.wait(timeout=15)
        if code or attack_code:
            raise RuntimeError(f'Vegeta exited: attack={attack_code}, encoder={code}')
        if not stop.is_set():
            raise RuntimeError('Continuous attack stopped unexpectedly')
    finally:
        stop.set()
        for child in (attack, encoder):
            if child is not None and child.poll() is None:
                child.kill()
                child.wait()
        if reporter is not None:
            reporter.join(timeout=5)
        collector.flush()
        print(json.dumps(dict(dimensions, event='load_stopped')), flush=True)


if __name__ == '__main__':
    main()

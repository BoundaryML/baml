"""Process gauges and Python allocator tracing, served only on Fly's private scrape port."""
import gc
import os
import threading
import time
import tracemalloc
from http.server import BaseHTTPRequestHandler, HTTPServer
from pathlib import Path

START_TIME = time.time()

def render_metrics():
    # Linux statm reports current resident pages, unlike ru_maxrss's high-water mark.
    rss = int(Path('/proc/self/statm').read_text().split()[1]) * os.sysconf('SC_PAGE_SIZE')
    current, peak = tracemalloc.get_traced_memory()
    values = [
        ('process_resident_memory_bytes', 'Resident memory of the complete serving process, including native allocations.', rss),
        ('python_traced_memory_current_bytes', 'Currently allocated blocks tracked by Python tracemalloc; not the complete native heap.', current),
        ('python_traced_memory_peak_bytes', 'Peak traced allocation size since process start.', peak),
        ('python_tracemalloc_overhead_bytes', 'Memory used internally by tracemalloc to track allocations.', tracemalloc.get_tracemalloc_memory()),
        ('python_tracemalloc_enabled', 'Whether Python allocation tracing is active.', int(tracemalloc.is_tracing())),
        ('process_start_time_seconds', 'Serving process metrics initialization time in Unix seconds.', START_TIME),
    ]
    lines = []
    for name, help_text, value in values:
        lines.extend([f'# HELP {name} {help_text}', f'# TYPE {name} gauge', f'{name} {value}'])
    lines.extend(['# HELP python_gc_collections_total Automatic and explicit GC collections by generation; exporter never forces GC.', '# TYPE python_gc_collections_total counter'])
    for generation, stats in enumerate(gc.get_stats()):
        lines.append(f'python_gc_collections_total{{generation="{generation}"}} {stats["collections"]}')
    return ('\n'.join(lines) + '\n').encode('utf-8')

class MetricsHandler(BaseHTTPRequestHandler):
    def do_GET(self):
        if self.path != '/metrics':
            self.send_response(404); self.send_header('Content-Length', '0'); self.end_headers(); return
        body = render_metrics()
        self.send_response(200)
        self.send_header('Content-Type', 'text/plain; version=0.0.4; charset=utf-8')
        self.send_header('Content-Length', str(len(body)))
        self.send_header('Cache-Control', 'no-store')
        self.end_headers()
        self.wfile.write(body)

    def log_message(self, *args):
        pass

    def setup(self):
        super().setup()
        self.connection.settimeout(5)

def start_metrics_server():
    if not tracemalloc.is_tracing():
        raise RuntimeError('Set PYTHONTRACEMALLOC=1 before Python starts')
    server = HTTPServer(('0.0.0.0', 9091), MetricsHandler)
    threading.Thread(target=server.serve_forever, name='fly-metrics', daemon=True).start()
    return server

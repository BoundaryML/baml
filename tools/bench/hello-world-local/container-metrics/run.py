#!/usr/bin/env python3
"""Export measured containers' Docker cgroup counters without polling their apps."""
import concurrent.futures
import http.client
import http.server
import json
import os
from pathlib import Path
import socket
import threading
import time
import urllib.parse

PROJECT = os.environ.get('COMPOSE_PROJECT_NAME', 'baml-local-hello-world')
BODY = b''
LOCK = threading.Lock()


class DockerConnection(http.client.HTTPConnection):
    def __init__(self):
        super().__init__('localhost', timeout=5)

    def connect(self):
        self.sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self.sock.settimeout(self.timeout)
        self.sock.connect('/var/run/docker.sock')


def get(path):
    conn = DockerConnection()
    try:
        conn.request('GET', path)
        response = conn.getresponse()
        if response.status != 200:
            raise RuntimeError(f'Docker {path}: {response.status}')
        return json.load(response)
    finally:
        conn.close()


def collect(container):
    name = container['Labels']['com.docker.compose.service']
    label = f'experiment="{name}"'
    identity = container['Id']
    info = get(f'/containers/{identity}/json')
    lines = [f'hello_container_running{{{label}}} {int(info["State"]["Running"])}',
             f'hello_container_restarts_total{{{label}}} {info["RestartCount"]}',
             f'hello_container_oom_killed{{{label}}} {int(info["State"]["OOMKilled"])}',
             f'hello_container_memory_limit_bytes{{{label}}} {info["HostConfig"]["Memory"]}',
             f'hello_container_cpu_limit{{{label}}} {info["HostConfig"]["NanoCpus"] / 1e9}']
    if info['State']['Running']:
        if name == 'baml-debian':
            # Docker reports the PID in the Linux daemon's namespace. Read its
            # actual RSS rather than labelling cgroup working set as process RSS.
            status = Path(f'/host/proc/{info["State"]["Pid"]}/status').read_text()
            rss = next(int(line.split()[1]) * 1024 for line in status.splitlines() if line.startswith('VmRSS:'))
            lines.append(f'process_resident_memory_bytes{{{label}}} {rss}')
        stats = get(f'/containers/{identity}/stats?stream=false&one-shot=true')
        cpu = stats.get('cpu_stats', {})
        memory = stats.get('memory_stats', {})
        usage = memory.get('usage', 0)
        inactive = memory.get('stats', {}).get('inactive_file', memory.get('stats', {}).get('total_inactive_file', 0))
        lines += [f'hello_container_cpu_seconds_total{{{label}}} {cpu.get("cpu_usage", {}).get("total_usage", 0) / 1e9}',
                  f'hello_container_memory_usage_bytes{{{label}}} {usage}',
                  f'hello_container_memory_working_set_bytes{{{label}}} {max(0, usage - inactive)}',
                  f'hello_container_cpu_throttled_seconds_total{{{label}}} {cpu.get("throttling_data", {}).get("throttled_time", 0) / 1e9}']
    lines.append(f'hello_container_metrics_up{{{label}}} 1')
    return lines


def refresh():
    global BODY
    filters = urllib.parse.quote(json.dumps({'label': [f'com.docker.compose.project={PROJECT}', 'hello.experiment=true']}))
    while True:
        lines = []
        try:
            containers = get('/containers/json?all=true&filters=' + filters)
            with concurrent.futures.ThreadPoolExecutor(max_workers=5) as pool:
                futures = {pool.submit(collect, container): container for container in containers}
                for future, container in futures.items():
                    try:
                        lines.extend(future.result())
                    except Exception as exc:
                        name = container['Labels']['com.docker.compose.service']
                        lines.append(f'hello_container_metrics_up{{experiment="{name}"}} 0')
                        print(f'{name}: {exc}', flush=True)
            lines.append('hello_docker_metrics_up 1')
        except Exception as exc:
            print(str(exc), flush=True)
            lines.append('hello_docker_metrics_up 0')
        with LOCK:
            BODY = ('\n'.join(lines) + '\n').encode()
        time.sleep(5)


class Handler(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        if self.path != '/metrics':
            self.send_error(404)
            return
        with LOCK:
            body = BODY
        self.send_response(200)
        self.send_header('Content-Type', 'text/plain; version=0.0.4')
        self.send_header('Content-Length', str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def log_message(self, *args):
        pass


if __name__ == '__main__':
    threading.Thread(target=refresh, daemon=True).start()
    http.server.ThreadingHTTPServer(('0.0.0.0', 9090), Handler).serve_forever()

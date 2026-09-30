#!/usr/bin/env python3
"""Fake LLM provider endpoint for the benchmark, run in its own process so neither
its Python objects nor BAML heap objects show up in the measured process.

Replay: every POST streams the recorded SSE body back one event at a time.
Record: every POST is forwarded to the real API and the SSE body is teed to disk.

Prints the bound port on stdout, then serves until killed.
"""
import argparse
import sys
import time
import urllib.error
import urllib.request
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

FORWARDED = ('content-type', 'authorization', 'x-api-key', 'anthropic-version', 'anthropic-beta')


def make_handler(args):
    events = []
    if args.recording and not args.record:
        events = [e + b'\n\n' for e in args.recording.read_bytes().split(b'\n\n') if e.strip()]

    class Handler(BaseHTTPRequestHandler):
        protocol_version = 'HTTP/1.1'

        def log_message(self, *_):
            pass

        def _start_sse(self):
            self.send_response(200)
            self.send_header('content-type', 'text/event-stream')
            self.send_header('transfer-encoding', 'chunked')
            self.end_headers()

        def _chunk(self, data):
            self.wfile.write(b'%x\r\n%s\r\n' % (len(data), data))
            self.wfile.flush()

        def do_POST(self):
            body = self.rfile.read(int(self.headers.get('content-length', 0)))
            if args.record:
                self._proxy(body)
                return
            self._start_sse()
            for event in events:
                self._chunk(event)
                if args.event_delay_ms:
                    time.sleep(args.event_delay_ms / 1000)
            self._chunk(b'')

        def _proxy(self, body):
            req = urllib.request.Request(
                args.upstream + self.path, data=body, method='POST',
                headers={k: v for k, v in self.headers.items() if k.lower() in FORWARDED})
            try:
                resp = urllib.request.urlopen(req, timeout=600)
            except urllib.error.HTTPError as err:
                detail = err.read()
                print(f'upstream {err.code}: {detail.decode(errors="replace")}', file=sys.stderr)
                self.send_response(err.code)
                self.send_header('content-type', err.headers.get('content-type', 'application/json'))
                self.send_header('content-length', str(len(detail)))
                self.end_headers()
                self.wfile.write(detail)
                return
            with resp, args.recording.open('wb') as out:
                self._start_sse()
                while line := resp.readline():
                    out.write(line)
                    self._chunk(line)
            self._chunk(b'')

    return Handler


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--recording', type=Path, required=True)
    parser.add_argument('--record', action='store_true', help='Proxy to --upstream and overwrite --recording')
    parser.add_argument('--upstream', default='https://api.anthropic.com', help='Base URL the client path is appended to')
    parser.add_argument('--event-delay-ms', type=float, default=0.5,
                        help='Replay pause after each SSE event (0 lets the client coalesce reads)')
    args = parser.parse_args()
    server = ThreadingHTTPServer(('127.0.0.1', 0), make_handler(args))
    print(server.server_address[1], flush=True)
    sys.stdout.close()
    server.serve_forever()


if __name__ == '__main__':
    main()

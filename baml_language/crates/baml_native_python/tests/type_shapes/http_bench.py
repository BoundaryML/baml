"""One local HTTP round trip through a BAML function (`http_get`), one
call after another: today's SDK or the native bindings, whichever
PYTHONPATH finds. The server runs in this process, on its own thread.

    http_bench.py <calls>      prints the median ns per call of 5 runs
"""

import asyncio
import statistics
import sys
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

import baml_sdk  # noqa: F401
from baml_sdk.bridge_test import http_get


class Handler(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def log_message(self, *args):
        pass

    def do_GET(self):
        self.send_response(200)
        self.send_header("content-length", "5")
        self.end_headers()
        self.wfile.write(b"hello")


def main():
    calls = int(sys.argv[1])
    server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    url = f"http://127.0.0.1:{server.server_address[1]}/hello"

    async def run():
        for _ in range(calls):
            await http_get(url, 5000)

    asyncio.run(run())  # warm: connections, first-run costs
    runs = []
    for _ in range(5):
        started = time.perf_counter()
        asyncio.run(run())
        runs.append((time.perf_counter() - started) / calls)
    print(f"{statistics.median(runs) * 1e9:.0f}")


if __name__ == "__main__":
    main()

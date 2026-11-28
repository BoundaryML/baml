"""HTTP requests that BAML sends itself (`baml.http`), against a server on
a loopback port this file starts. These run against today's SDK and the
native bindings alike (PYTHONPATH=.../vm or .../native): the same answers,
and the same errors, from both."""

import asyncio
import json
import socket
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

import pytest

import baml_sdk  # noqa: F401
from baml_bridge.errors import BamlError
from baml_sdk.bridge_test import (
    Echo,
    Greeting,
    HttpReply,
    http_get,
    http_get_sync,
    http_post_json,
)


class Handler(BaseHTTPRequestHandler):
    def log_message(self, *args):
        pass

    def reply(self, status, body):
        data = body.encode()
        self.send_response(status)
        self.send_header("content-length", str(len(data)))
        self.send_header("connection", "close")
        self.end_headers()
        self.wfile.write(data)

    def do_GET(self):
        if self.path == "/hello":
            self.reply(200, "hello")
        elif self.path.startswith("/slow/"):
            time.sleep(int(self.path[len("/slow/") :]) / 1000)
            self.reply(200, "slow")
        else:
            self.reply(404, "no")

    def do_POST(self):
        body = json.loads(self.rfile.read(int(self.headers["content-length"])))
        body["served_by"] = "python-test"
        self.reply(200, json.dumps(body))


class Server(ThreadingHTTPServer):
    # Read when the socket starts listening: room for 100 connections at once.
    request_queue_size = 256
    daemon_threads = True


@pytest.fixture(scope="module")
def server():
    httpd = Server(("127.0.0.1", 0), Handler)
    thread = threading.Thread(target=httpd.serve_forever, daemon=True)
    thread.start()
    yield f"http://127.0.0.1:{httpd.server_address[1]}"
    httpd.shutdown()


def closed_port():
    with socket.socket() as s:
        s.bind(("127.0.0.1", 0))
        return s.getsockname()[1]


async def test_one_get(server):
    assert await http_get(f"{server}/hello", 5000) == HttpReply(status=200, body="hello")
    assert http_get_sync(f"{server}/hello", 5000) == HttpReply(status=200, body="hello")


async def test_one_post_with_a_json_body_and_a_class_back(server):
    echo = await http_post_json(f"{server}/echo", Greeting(name="Ada", count=3))
    assert echo == Echo(name="Ada", count=3, served_by="python-test")


async def test_a_hundred_requests_at_once_overlap(server):
    started = time.monotonic()
    replies = await asyncio.gather(*(http_get(f"{server}/slow/300", 10_000) for _ in range(100)))
    elapsed = time.monotonic() - started
    assert all(r == HttpReply(status=200, body="slow") for r in replies)
    # One after another they would take 30 s.
    assert elapsed < 5, elapsed


async def test_a_refused_connection(server):
    with pytest.raises(BamlError) as raised:
        await http_get(f"http://127.0.0.1:{closed_port()}/hello", 5000)
    assert raised.value.class_name == "baml.errors.Io", raised.value


async def test_a_timeout(server):
    started = time.monotonic()
    with pytest.raises(BamlError) as raised:
        await http_get(f"{server}/slow/3000", 200)
    assert raised.value.class_name == "baml.errors.Timeout", raised.value
    assert time.monotonic() - started < 2

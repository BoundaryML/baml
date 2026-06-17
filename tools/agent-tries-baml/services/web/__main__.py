"""Run the combined api+ingress app under uvicorn (dual-stack on Fly).

Mirrors services.api.__main__: when HOST is IPv6 (``::`` on Fly) we bind a
dual-stack socket so both the 6PN ``.internal`` callers and the IPv4 http_service
proxy reach us; locally (``0.0.0.0``) it's a plain run.
"""

import os
import socket

import uvicorn

from services.web.app import app

if __name__ == "__main__":
    host = os.environ.get("HOST", "0.0.0.0")
    port = int(os.environ.get("PORT", "8080"))
    log_level = os.environ.get("LOG_LEVEL", "info").lower()

    if ":" in host:  # IPv6 -> dual-stack so the IPv4 http_service proxy can reach us
        sock = socket.socket(socket.AF_INET6, socket.SOCK_STREAM)
        sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        sock.setsockopt(socket.IPPROTO_IPV6, socket.IPV6_V6ONLY, 0)
        sock.bind((host, port))
        sock.listen()
        uvicorn.Server(uvicorn.Config(app, log_level=log_level)).run(sockets=[sock])
    else:
        uvicorn.run(app, host=host, port=port, log_level=log_level)

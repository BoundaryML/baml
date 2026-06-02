"""Module entrypoint that runs the API app under uvicorn.

On Fly the api is reached two ways: other services hit it over `.internal`
(the 6PN IPv6 network), while the public `http_service` proxy connects over
IPv4 loopback. An IPv6-only socket (`HOST=::`) serves the former but is
unreachable by the latter, so when HOST is IPv6 we bind a dual-stack socket
(`IPV6_V6ONLY=0`) that accepts both. Locally (`HOST=0.0.0.0`) it's a plain run.
"""

import os
import socket

import uvicorn

if __name__ == "__main__":
    host = os.environ.get("HOST", "0.0.0.0")
    port = int(os.environ.get("PORT", "8080"))
    log_level = os.environ.get("LOG_LEVEL", "info").lower()

    if ":" in host:  # IPv6 -> bind dual-stack so the IPv4 http_service proxy can reach us
        sock = socket.socket(socket.AF_INET6, socket.SOCK_STREAM)
        sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        sock.setsockopt(socket.IPPROTO_IPV6, socket.IPV6_V6ONLY, 0)
        sock.bind((host, port))
        sock.listen()
        uvicorn.Server(
            uvicorn.Config("services.api.app:app", log_level=log_level)
        ).run(sockets=[sock])
    else:
        uvicorn.run("services.api.app:app", host=host, port=port, log_level=log_level)

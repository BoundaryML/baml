"""Pytest configuration + the end-to-end integration harness.

Tiers:
  * Fast tests (no marker) - pure logic / TestClient; run anywhere, no Docker.
  * Integration / system (``@pytest.mark.integration`` / ``system``) - boot
    ``api`` / ``ingress`` / ``fake_proxy`` as host ``uvicorn`` subprocesses and drive
    the whole pipeline through the real claim loops.

By default the api uses the in-process MemoryGateway (``CONVEX_BACKEND=memory``), so
the heavy tiers need no Convex deployment and no Docker - they run in seconds against
the editable install. Set ``BAML_BENCH_REAL_CONVEX=1`` to boot the real Convex
backend in a container instead (Docker + Node), for fidelity against the actual
backend; that path self-skips when Docker is unavailable.
"""

from __future__ import annotations

import os

# Dummy env so service modules that read os.environ at *import* time don't KeyError
# during fast unit tests (ingress needs SERVICE_URL; api needs CONVEX_URL). These are
# placeholders, never real secrets.
os.environ.setdefault("CONVEX_URL", "http://localhost:3210")
os.environ.setdefault("SERVICE_URL", "http://localhost:8080")
os.environ.setdefault("ATB_SERVICE_TOKEN", "devservicetoken")
os.environ.setdefault("ATB_CLAUDE_PROXY_TOKEN", "devproxytoken")
os.environ.setdefault("ATB_SLACK_SIGNING_SECRET", "devsigningsecret")

import shutil
import socket
import subprocess
import time
from contextlib import closing
from pathlib import Path

import httpx
import pytest

ROOT = Path(__file__).resolve().parents[1]


def pytest_configure(config: pytest.Config) -> None:
    """Register the ``integration`` marker so marked tests are recognized.

    Args:
        config: The pytest config the marker line is added to.
    """
    config.addinivalue_line(
        "markers", "integration: per-hop pipeline test that boots the stack (api/ingress/fake_proxy)"
    )
    config.addinivalue_line(
        "markers", "system: full end-to-end pipeline flow, /bug through fix dispatch"
    )


def _docker_ready() -> bool:
    """Report whether a usable Docker daemon is available.

    Returns:
        True if the ``docker`` CLI is on PATH and ``docker info`` succeeds.
    """
    if not shutil.which("docker"):
        return False
    return subprocess.run(["docker", "info"], capture_output=True).returncode == 0


def _wait_http(url: str, timeout: float = 60.0) -> None:
    """Poll a URL until it responds below 500 or the timeout elapses.

    Args:
        url: The URL to GET.
        timeout: Maximum seconds to wait before giving up.

    Raises:
        RuntimeError: If the URL never returns a sub-500 status in time.
    """
    deadline = time.time() + timeout
    last = ""
    while time.time() < deadline:
        try:
            r = httpx.get(url, timeout=2.0)
            if r.status_code < 500:
                return
        except Exception as e:  # noqa: BLE001
            last = str(e)
        time.sleep(0.5)
    raise RuntimeError(f"timed out waiting for {url}: {last}")


def _free_port() -> int:
    """Reserve and return an unused local TCP port.

    Returns:
        A port number the OS just assigned on 127.0.0.1 (released immediately,
        so it is free for a child process to bind).
    """
    with closing(socket.socket()) as s:
        s.bind(("127.0.0.1", 0))
        return s.getsockname()[1]


def _spawn_uvicorn(target: str, port: int, env: dict[str, str]) -> subprocess.Popen:
    """Start ``uvicorn <target> --port <port>`` as a host subprocess.

    Args:
        target: The ``module:app`` import string uvicorn serves.
        port: The local port to bind on 127.0.0.1.
        env: Extra environment variables layered over the current environment.

    Returns:
        The spawned process handle (stdout/stderr discarded).
    """
    full = {**os.environ, **env}
    return subprocess.Popen(
        ["python", "-m", "uvicorn", target, "--host", "127.0.0.1", "--port", str(port)],
        cwd=str(ROOT), env=full,
        stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
    )


@pytest.fixture(scope="session")
def bench_stack():
    """Boot api/ingress/fake_proxy (host uvicorn) and yield their base URLs.

    By default the api runs against the in-process MemoryGateway
    (CONVEX_BACKEND=memory), so the heavy tiers need no Convex deployment and no
    Docker. Set BAML_BENCH_REAL_CONVEX=1 to instead boot the real Convex backend in
    a container (mint an admin key, push schema/functions) for fidelity against the
    actual backend; that path needs Docker + Node and self-skips when Docker is
    absent (unless BAML_BENCH_REQUIRE_DOCKER=1, which makes it fail loudly).

    Yields:
        A dict with the ``api``, ``ingress``, and ``proxy`` base URLs.
    """
    real_convex = os.environ.get("BAML_BENCH_REAL_CONVEX") == "1"
    procs: list[subprocess.Popen] = []
    convex_started = False
    # Ephemeral host ports so the harness is immune to whatever else is running
    # locally (e.g. a stale stack squatting on 8080).
    proxy_port, api_port, ingress_port = _free_port(), _free_port(), _free_port()
    # Backend-specific env for the api: in-memory by default, real Convex
    # (CONVEX_URL + admin key) when BAML_BENCH_REAL_CONVEX=1.
    api_env = {"SERVICE_TOKEN": "devservicetoken", "BLOB_DIR": str(ROOT / ".pytest-blobs")}
    try:
        if real_convex:
            if not _docker_ready():
                # BAML_BENCH_REQUIRE_DOCKER=1 makes a broken daemon fail loudly instead
                # of passing as "skipped"; otherwise self-skip on a bare machine.
                if os.environ.get("BAML_BENCH_REQUIRE_DOCKER") == "1":
                    raise RuntimeError(
                        "Docker required (BAML_BENCH_REAL_CONVEX=1) but not available"
                    )
                pytest.skip("Docker not available - skipping real-Convex stack")
            # 1. Convex backend (only container) on an ephemeral host port.
            convex_port, convex_site = _free_port(), _free_port()
            convex_url = f"http://localhost:{convex_port}"
            up_env = {**os.environ, "CONVEX_PORT": str(convex_port),
                      "CONVEX_SITE_PORT": str(convex_site)}
            subprocess.run(["docker", "compose", "up", "-d", "convex"],
                           cwd=str(ROOT), env=up_env, check=True)
            convex_started = True
            _wait_http(f"{convex_url}/version", timeout=90)
            # 2. Admin key from the running backend.
            key = subprocess.run(
                ["docker", "compose", "exec", "-T", "convex", "./generate_admin_key.sh"],
                cwd=str(ROOT), env=up_env, capture_output=True, text=True, check=True,
            ).stdout.strip().splitlines()[-1].strip()
            # 3. Push schema + functions (self-hosted push reads these from the env).
            push_env = {**os.environ, "CONVEX_SELF_HOSTED_URL": convex_url,
                        "CONVEX_SELF_HOSTED_ADMIN_KEY": key}
            if not (ROOT / "node_modules").exists():
                subprocess.run(["npm", "ci"], cwd=str(ROOT), check=True)
            subprocess.run(["npx", "convex", "dev", "--once"],
                           cwd=str(ROOT), env=push_env, check=True)
            api_env.update({"CONVEX_URL": convex_url, "CONVEX_ADMIN_KEY": key,
                            "ATB_CONVEX_ADMIN_KEY": key})
        else:
            # In-memory Convex backend: no deployment, no Docker, no Node.
            api_env["CONVEX_BACKEND"] = "memory"

        # fake_proxy (stub claude-proxy), api, ingress as host uvicorn processes.
        procs.append(_spawn_uvicorn("tests.fake_proxy:app", proxy_port, {}))
        procs.append(_spawn_uvicorn("services.api.app:app", api_port, api_env))
        procs.append(_spawn_uvicorn("services.ingress.app:app", ingress_port, {
            "SERVICE_URL": f"http://localhost:{api_port}", "SERVICE_TOKEN": "devservicetoken",
            "SLACK_SIGNING_SECRET": "devsigningsecret",
        }))
        _wait_http(f"http://localhost:{api_port}/healthz")
        _wait_http(f"http://localhost:{ingress_port}/healthz")
        _wait_http(f"http://localhost:{proxy_port}/docs")

        # Env the in-process drivers read (worker/dedup/fixdispatch run here). The fake
        # CURSOR_* point FixDispatch's cloud-agent launch at fake_proxy's stub.
        os.environ.update({
            "SERVICE_URL": f"http://localhost:{api_port}",
            "INGRESS_URL": f"http://localhost:{ingress_port}",
            "CLAUDE_PROXY_URLS": f"http://localhost:{proxy_port}",
            "CLAUDE_PROXY_TOKEN": "devproxytoken",
            "SLACK_SIGNING_SECRET": "devsigningsecret",
            "SERVICE_TOKEN": "devservicetoken",
            # FixDispatch reads the ATB_-prefixed key (Infisical naming); CURSOR_API_BASE
            # stays unprefixed (it's config, not a secret) and points at fake_proxy.
            "ATB_CURSOR_API_KEY": "devcursorkey",
            "CURSOR_API_BASE": f"http://localhost:{proxy_port}",
        })
        yield {"api": f"http://localhost:{api_port}",
               "ingress": f"http://localhost:{ingress_port}",
               "proxy": f"http://localhost:{proxy_port}"}
    finally:
        for p in procs:
            p.terminate()
        for p in procs:
            try:
                p.wait(timeout=10)
            except Exception:  # noqa: BLE001
                p.kill()
        if convex_started:
            subprocess.run(["docker", "compose", "down", "-v"], cwd=str(ROOT))

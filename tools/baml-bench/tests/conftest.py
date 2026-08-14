"""Pytest configuration + the end-to-end integration harness.

Two tiers:
  * Fast tests (no marker) - pure logic / TestClient; run anywhere, no Docker.
  * Integration tests (``@pytest.mark.integration``) - boot a real Convex backend
    (one container) plus ``api`` / ``ingress`` / ``fake_proxy`` as host processes and
    drive the whole pipeline. They self-skip when Docker is unavailable.

The integration harness deliberately runs only Convex as a container (a prebuilt
image - just a pull). ``api``/``ingress``/``fake_proxy`` run as host ``uvicorn``
subprocesses against the editable install, so we never pay to build the slow
``Dockerfile.python`` image just to test.
"""

from __future__ import annotations

import os

# Dummy env so service modules that read os.environ at *import* time don't KeyError
# during fast unit tests (ingress needs SERVICE_URL; api needs CONVEX_URL). These are
# placeholders, never real secrets.
os.environ.setdefault("CONVEX_URL", "http://localhost:3210")
os.environ.setdefault("SERVICE_URL", "http://localhost:8080")
os.environ.setdefault("SERVICE_TOKEN", "devservicetoken")
os.environ.setdefault("CLAUDE_PROXY_TOKEN", "devproxytoken")
os.environ.setdefault("SLACK_SIGNING_SECRET", "devsigningsecret")

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
        "markers", "integration: end-to-end test that boots the stack (needs Docker)"
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
    """Boot Convex (container) + api/ingress/fake_proxy (host) and yield their URLs.

    Brings up the Convex backend container on ephemeral ports, mints an admin key,
    pushes the schema/functions, then starts ``fake_proxy``/``api``/``ingress`` as
    host uvicorn subprocesses and exports the env the in-process drivers read
    (including a fake ``CURSOR_API_KEY``/``CURSOR_API_BASE`` pointed at fake_proxy so
    FixDispatch exercises the cloud-agent launch). Tears the whole stack down on exit.
    Skips when Docker isn't available so the suite still runs on a bare machine.

    Yields:
        A dict with the ``api``, ``ingress``, and ``proxy`` base URLs.
    """
    if not _docker_ready():
        pytest.skip("Docker not available - skipping integration stack")

    procs: list[subprocess.Popen] = []
    # Ephemeral host ports so the harness is immune to whatever else is running
    # locally (e.g. a stale stack squatting on 8080). The Convex container port is
    # overridden the same way.
    convex_port, convex_site = _free_port(), _free_port()
    proxy_port, api_port, ingress_port = _free_port(), _free_port(), _free_port()
    convex_url = f"http://localhost:{convex_port}"
    try:
        # 1. Convex backend (only container) on an ephemeral host port.
        up_env = {**os.environ, "CONVEX_PORT": str(convex_port),
                  "CONVEX_SITE_PORT": str(convex_site)}
        subprocess.run(["docker", "compose", "up", "-d", "convex"],
                       cwd=str(ROOT), env=up_env, check=True)
        _wait_http(f"{convex_url}/version", timeout=90)

        # 2. Admin key from the running backend.
        key = subprocess.run(
            ["docker", "compose", "exec", "-T", "convex", "./generate_admin_key.sh"],
            cwd=str(ROOT), env=up_env, capture_output=True, text=True, check=True,
        ).stdout.strip().splitlines()[-1].strip()

        # 3. Push schema + functions (self-hosted push reads these from the env).
        push_env = {**os.environ,
                    "CONVEX_SELF_HOSTED_URL": convex_url,
                    "CONVEX_SELF_HOSTED_ADMIN_KEY": key}
        if not (ROOT / "node_modules").exists():
            subprocess.run(["npm", "ci"], cwd=str(ROOT), check=True)
        subprocess.run(["npx", "convex", "dev", "--once"],
                       cwd=str(ROOT), env=push_env, check=True)

        # 4. fake_proxy (stub claude-proxy), api, ingress as host processes.
        procs.append(_spawn_uvicorn("tests.fake_proxy:app", proxy_port, {}))
        procs.append(_spawn_uvicorn("services.api.app:app", api_port, {
            "CONVEX_URL": convex_url, "CONVEX_ADMIN_KEY": key,
            "SERVICE_TOKEN": "devservicetoken", "BLOB_DIR": str(ROOT / ".pytest-blobs"),
        }))
        procs.append(_spawn_uvicorn("services.ingress.app:app", ingress_port, {
            "SERVICE_URL": f"http://localhost:{api_port}", "SERVICE_TOKEN": "devservicetoken",
            "SLACK_SIGNING_SECRET": "devsigningsecret",
        }))
        _wait_http(f"http://localhost:{api_port}/healthz")
        _wait_http(f"http://localhost:{ingress_port}/healthz")
        _wait_http(f"http://localhost:{proxy_port}/docs")

        # 5. Env the in-process drivers read (worker/dedup/fixdispatch run here). The
        # fake CURSOR_* point FixDispatch's cloud-agent launch at fake_proxy's stub.
        os.environ.update({
            "SERVICE_URL": f"http://localhost:{api_port}",
            "INGRESS_URL": f"http://localhost:{ingress_port}",
            "CLAUDE_PROXY_URLS": f"http://localhost:{proxy_port}",
            "CLAUDE_PROXY_TOKEN": "devproxytoken",
            "SLACK_SIGNING_SECRET": "devsigningsecret",
            "SERVICE_TOKEN": "devservicetoken",
            "CURSOR_API_KEY": "devcursorkey",
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
        subprocess.run(["docker", "compose", "down", "-v"], cwd=str(ROOT))

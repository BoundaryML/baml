"""Fast import/wiring check: every public FastAPI app constructs and serves /healthz.

No network, no Convex, no secrets - just proves the apps import cleanly (catching the
kind of breakage a refactor introduces) and the health route responds.
"""

import pytest
from fastapi.testclient import TestClient

from services.api.app import app as api_app
from services.ingress.app import app as ingress_app
from services.claude_proxy.app import app as proxy_app


@pytest.mark.parametrize(
    "app", [api_app, ingress_app, proxy_app], ids=["api", "ingress", "claude_proxy"]
)
def test_healthz_ok(app):
    """GET /healthz returns 200/"ok" for each constructed app.

    Args:
        app: The FastAPI app under test (api, ingress, or claude_proxy).
    """
    with TestClient(app) as client:
        r = client.get("/healthz")
    assert r.status_code == 200
    assert "ok" in r.text.lower()

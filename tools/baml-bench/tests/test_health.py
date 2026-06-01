"""Fast import/wiring check: the api and claude_proxy apps construct and serve /healthz.

No network, no Convex, no secrets - just proves the apps import cleanly (catching the
kind of breakage a refactor introduces) and the health route responds. The ingress app
is added to this check when ingress lands.
"""

import pytest
from fastapi.testclient import TestClient

from services.api.app import app as api_app
from services.claude_proxy.app import app as proxy_app


@pytest.mark.parametrize("app", [api_app, proxy_app], ids=["api", "claude_proxy"])
def test_healthz_ok(app):
    """GET /healthz returns 200/"ok" for each constructed app."""
    with TestClient(app) as client:
        r = client.get("/healthz")
    assert r.status_code == 200
    assert "ok" in r.text.lower()

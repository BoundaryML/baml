"""Combined public web app: the api gateway + the ingress webhook gateway served
by one ASGI app, so a single machine (bammy-service) exposes both on one port.

The api uses explicit per-table routers (`/issues`, `/tasks`, …) — there is no
generic ``/{table}`` catch-all — so the only path the two apps share is
``/healthz``. We therefore graft the ingress routes onto the api app, skipping
that single duplicate. The standalone ``services.api`` / ``services.ingress``
entrypoints still exist unchanged for local dev, tests, and the split deploy.
"""

from __future__ import annotations

from services.api.app import app
from services.ingress.app import app as _ingress


def _route_key(route: object) -> tuple:
    """Identity of a route for dedupe: (path, frozenset(methods))."""
    return (getattr(route, "path", None), frozenset(getattr(route, "methods", None) or ()))


_existing = {_route_key(r) for r in app.routes}
for _r in _ingress.routes:
    if _route_key(_r) in _existing:
        continue  # /healthz is defined by both; keep the api's
    app.router.routes.append(_r)
    _existing.add(_route_key(_r))

__all__ = ["app"]

"""Client for the claude-proxy /run-agent endpoint.

BAML workers and BAML dedup pick a proxy from a comma-separated CLAUDE_PROXY_URLS
list (random selection spreads load across N stateless proxies).
"""

from __future__ import annotations

import os
import random

import httpx

from .schemas import AgentResult, CheckBamlRequest, CheckBamlResult, RunAgentRequest


class ProxyClient:
    """Load-spreading client for a pool of stateless claude-proxy instances."""

    def __init__(self, urls: list[str], token: str):
        """Initialize the client from a pool of proxy URLs and a bearer token.

        Args:
            urls: Candidate proxy base URLs; blanks are dropped and trailing
                slashes stripped.
            token: Bearer token sent on requests; an empty string sends no
                Authorization header.

        Raises:
            ValueError: If no non-empty proxy URL is provided.
        """
        self.urls = [u.rstrip("/") for u in urls if u.strip()]
        if not self.urls:
            raise ValueError("no valid proxy URLs provided for ProxyClient")
        self._headers = {"Authorization": f"Bearer {token}"} if token else {}

    @classmethod
    def from_env(cls) -> "ProxyClient":
        """Build a client from the CLAUDE_PROXY_URLS and CLAUDE_PROXY_TOKEN env vars.

        Returns:
            A ProxyClient configured from the comma-separated URL list (default
            "http://claude-proxy:9090") and the proxy token.
        """
        raw = os.environ.get("CLAUDE_PROXY_URLS", "http://claude-proxy:9090")
        return cls(raw.split(","), os.environ.get("CLAUDE_PROXY_TOKEN", ""))

    async def run_agent(self, req: RunAgentRequest, *, timeout: float = 7200.0) -> AgentResult:
        """Run an agent by POSTing /run-agent to a randomly chosen proxy.

        Args:
            req: The run-agent request describing the cell, model, and prompt.
            timeout: Per-request timeout in seconds for the long-lived agent run.

        Returns:
            The parsed AgentResult returned by the proxy.

        Raises:
            httpx.HTTPStatusError: If the proxy returns a non-2xx response.
        """
        url = random.choice(self.urls)
        async with httpx.AsyncClient(timeout=timeout) as c:
            r = await c.post(
                f"{url}/run-agent", json=req.model_dump(), headers=self._headers
            )
            r.raise_for_status()
            return AgentResult.model_validate(r.json())

    async def check_baml(self, req: CheckBamlRequest, *, timeout: float = 180.0) -> CheckBamlResult:
        """Check a baml repro by POSTing /check-baml to a randomly chosen proxy.

        Args:
            req: The check request describing the repro files and command.
            timeout: Per-request timeout in seconds.

        Returns:
            The parsed CheckBamlResult returned by the proxy.

        Raises:
            httpx.HTTPStatusError: If the proxy returns a non-2xx response.
        """
        url = random.choice(self.urls)
        async with httpx.AsyncClient(timeout=timeout) as c:
            r = await c.post(
                f"{url}/check-baml", json=req.model_dump(), headers=self._headers
            )
            r.raise_for_status()
            return CheckBamlResult.model_validate(r.json())

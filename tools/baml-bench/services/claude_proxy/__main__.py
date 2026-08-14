"""Module entrypoint that serves the claude-proxy FastAPI app via uvicorn."""

import os

import uvicorn

if __name__ == "__main__":
    uvicorn.run(
        "services.claude_proxy.app:app",
        host=os.environ.get("HOST", "0.0.0.0"),  # "::" on Fly for IPv6 .internal
        port=int(os.environ.get("PROXY_LISTEN_PORT", "9090")),
        log_level=os.environ.get("LOG_LEVEL", "info").lower(),
    )

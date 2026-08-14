"""Module entrypoint that runs the API app under uvicorn."""

import os

import uvicorn

if __name__ == "__main__":
    uvicorn.run(
        "services.api.app:app",
        host=os.environ.get("HOST", "0.0.0.0"),  # ":" on Fly for IPv6 .internal
        port=int(os.environ.get("PORT", "8080")),
        log_level=os.environ.get("LOG_LEVEL", "info").lower(),
    )

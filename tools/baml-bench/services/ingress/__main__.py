import os

import uvicorn

if __name__ == "__main__":
    uvicorn.run(
        "services.ingress.app:app",
        host=os.environ.get("HOST", "0.0.0.0"),  # "::" on Fly for IPv6 .internal
        port=int(os.environ.get("PORT", "8081")),
        log_level=os.environ.get("LOG_LEVEL", "info").lower(),
    )

from runtime_metrics import start_metrics_server
from fastapi import FastAPI
from fastapi.responses import PlainTextResponse

app = FastAPI(openapi_url=None, docs_url=None, redoc_url=None)


@app.get("/", response_class=PlainTextResponse)
async def hello():
    return PlainTextResponse("hello world", headers={"Cache-Control": "no-store"})

metrics_server = start_metrics_server()

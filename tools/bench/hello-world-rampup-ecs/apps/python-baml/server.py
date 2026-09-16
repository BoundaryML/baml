from runtime_metrics import start_metrics_server
from fastapi import FastAPI
from fastapi.responses import PlainTextResponse
from baml_sdk import hello_world_async

app = FastAPI(openapi_url=None, docs_url=None, redoc_url=None)


@app.get("/", response_class=PlainTextResponse)
async def hello():
    return PlainTextResponse(await hello_world_async(), headers={"Cache-Control": "no-store"})

metrics_server = start_metrics_server()

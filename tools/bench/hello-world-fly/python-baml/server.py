from runtime_metrics import start_metrics_server
from starlette.applications import Starlette
from starlette.responses import PlainTextResponse
from starlette.routing import Route
from baml_sdk import hello_world_async

async def hello(request):
    return PlainTextResponse(await hello_world_async(), headers={"Cache-Control": "no-store"})

app = Starlette(routes=[Route("/", hello, methods=["GET"])])

metrics_server = start_metrics_server()

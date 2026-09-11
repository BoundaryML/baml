from runtime_metrics import start_metrics_server
from starlette.applications import Starlette
from starlette.responses import PlainTextResponse
from starlette.routing import Route

async def hello(request):
    return PlainTextResponse("hello world", headers={"Cache-Control": "no-store"})

app = Starlette(routes=[Route("/", hello, methods=["GET"])])

metrics_server = start_metrics_server()

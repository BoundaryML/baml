import gc as python_gc

from starlette.applications import Starlette
from starlette.responses import PlainTextResponse
from starlette.routing import Route

from baml_sdk import collect_and_report_async, hello_world_async


async def hello(request):
    return PlainTextResponse(await hello_world_async(), headers={"Cache-Control": "no-store"})


async def gc(request):
    report = await collect_and_report_async()
    collected = python_gc.collect(2)
    return PlainTextResponse(report, headers={"Cache-Control": "no-store", "X-Python-GC-Collected": str(collected)})


app = Starlette(routes=[Route("/", hello, methods=["GET"]), Route("/gc", gc, methods=["GET"])])

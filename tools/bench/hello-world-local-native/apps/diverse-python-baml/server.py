import gc as python_gc
import json
from pathlib import Path

from baml_bridge.baml_py import BamlImage as Image
from baml_sdk import (
    force_gc_async,
    hello_world_async,
    image_base64_async,
    json_1m_async,
    json_64k_async,
)
from starlette.applications import Starlette
from starlette.responses import Response
from starlette.routing import Route

IMAGE_PATH = Path(__file__).with_name("benchmark-image.png")
NO_STORE = {"Cache-Control": "no-store"}


async def hello(request):
    return Response(
        await hello_world_async(), media_type="text/plain", headers=NO_STORE
    )


async def json_64k(request):
    value = await json_64k_async()
    return Response(
        json.dumps(value, separators=(",", ":")),
        media_type="application/json",
        headers=NO_STORE,
    )


async def json_1m(request):
    value = await json_1m_async()
    return Response(
        json.dumps(value, separators=(",", ":")),
        media_type="application/json",
        headers=NO_STORE,
    )


async def image(request):
    value = Image.from_file(str(IMAGE_PATH), mime_type="image/png")
    return Response(
        await image_base64_async(value), media_type="text/plain", headers=NO_STORE
    )


async def gc(request):
    await force_gc_async()
    collected = python_gc.collect(2)
    return Response(
        "ok",
        media_type="text/plain",
        headers={**NO_STORE, "X-Python-GC-Collected": str(collected)},
    )


app = Starlette(
    routes=[
        Route("/hello", hello, methods=["GET"]),
        Route("/json-64k", json_64k, methods=["GET"]),
        Route("/json-1m", json_1m, methods=["GET"]),
        Route("/image", image, methods=["GET"]),
        Route("/gc", gc, methods=["GET"]),
    ]
)

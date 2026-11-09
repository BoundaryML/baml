"""The four benchmark apps in Python, instrumented the way each vendor documents.

    python py_apps.py <stack> <app> --text ... [--n N] [--vary]

stack: off | otel | logfire | datadog | bridge (Python calling the BAML functions)
app:   hello | classify | extract | agent | chat (--text is JSON {system, users})

Model calls go to MODEL_BASE_URL with the official OpenAI SDK. Each stack's
exporter endpoint comes from TELEMETRY_URL (a loopback relay). Setup lines for
each stack sit between the `# setup:<stack>` markers so the lines-of-code
experiment can count them.
"""
import time

T0 = time.perf_counter()

import argparse
import json
import os
import sys
import time

from openai import OpenAI
from pydantic import BaseModel

STACK, APP = sys.argv[1], sys.argv[2]
TELEMETRY_URL = os.environ.get("TELEMETRY_URL", "")


class Invoice(BaseModel):
    vendor: str
    total: float
    currency: str
    paid: bool


def passthrough(fn):
    return fn


trace_app = trace_tool = passthrough
flush = lambda: None

if STACK == "logfire":
    # setup:logfire
    import logfire
    logfire.configure(service_name="bench", console=False, metrics=False,
                      advanced=logfire.AdvancedOptions(base_url=TELEMETRY_URL))
    logfire.instrument_openai()
    trace_app = trace_tool = logfire.instrument()
    # end:logfire
    flush = logfire.force_flush

elif STACK == "datadog":
    # setup:datadog
    os.environ.update(DD_LLMOBS_OVERRIDE_ORIGIN=TELEMETRY_URL, DD_APM_TRACING_ENABLED="false")
    from ddtrace.llmobs import LLMObs
    from ddtrace.llmobs.decorators import tool, workflow
    LLMObs.enable(ml_app="bench", agentless_enabled=True, integrations_enabled=True)
    trace_app, trace_tool = workflow, tool
    # end:datadog
    flush = LLMObs.flush

elif STACK == "otel":
    # setup:otel
    os.environ.setdefault("OTEL_INSTRUMENTATION_GENAI_CAPTURE_MESSAGE_CONTENT", "true")
    from opentelemetry import trace
    from opentelemetry.exporter.otlp.proto.http import Compression
    from opentelemetry.exporter.otlp.proto.http.trace_exporter import OTLPSpanExporter
    from opentelemetry.instrumentation.openai_v2 import OpenAIInstrumentor
    from opentelemetry.sdk.resources import Resource
    from opentelemetry.sdk.trace import TracerProvider
    from opentelemetry.sdk.trace.export import BatchSpanProcessor
    provider = TracerProvider(resource=Resource.create({"service.name": "bench"}))
    provider.add_span_processor(BatchSpanProcessor(
        OTLPSpanExporter(endpoint=TELEMETRY_URL + "/v1/traces", compression=Compression.Gzip)))
    trace.set_tracer_provider(provider)
    OpenAIInstrumentor().instrument()
    tracer = trace.get_tracer("bench")

    def traced(fn):
        def wrapper(*args):
            with tracer.start_as_current_span(fn.__name__) as span:
                span.set_attribute("input", json.dumps(args))
                result = fn(*args)
                span.set_attribute("output", json.dumps(result if not isinstance(result, BaseModel) else result.model_dump()))
                return result
        return wrapper
    trace_app = trace_tool = traced
    # end:otel
    flush = provider.force_flush

model = OpenAI(base_url=os.environ["MODEL_BASE_URL"], api_key=os.environ["MODEL_API_KEY"], max_retries=1)
MODEL = os.environ.get("MODEL_NAME", "gpt-4.1-mini")


def ask(prompt):
    reply = model.chat.completions.create(model=MODEL, messages=[{"role": "user", "content": prompt}])
    return reply.choices[0].message.content


# App 1: no model, no captured values.
@trace_app
def hello():
    return "hello world"


# App 2: a yes/no question about a document.
@trace_app
def classify(text):
    return ask(f"Is this document an invoice? Reply with only true or false.\n{text}").strip() == "true"


# App 3: structured extraction into a class.
@trace_app
def extract(text):
    answer = ask(f"Extract the invoice. Answer in JSON with keys vendor, total, currency, paid.\n{text}")
    return Invoice.model_validate_json(answer)


# App 4: a two-step agent with a tool and a retried model failure.
@trace_tool
def stock(sku):
    return {"ACME-1": 7}.get(sku, 0)


@trace_app
def agent(ticket):
    sku = json.loads(ask(f"Choose the SKU for inventory lookup. Answer in JSON with key sku.\nTicket: {ticket}"))["sku"]
    return ask(f"Respond to the customer. Units available: {stock(sku)}")


# App 5: a support chatbot that resends the whole conversation on every model call.
ORDER_FORMAT = 'Answer in JSON using this schema:\n{\n  kind: "tool" or "reply",\n  text: string,\n}'


@trace_tool
def lookup_order(order_id):
    return {"id": order_id, "status": "shipped", "carrier": "UPS Ground", "tracking": f"1Z999AA1{order_id}0000784",
            "eta": "2026-11-12", "address": "1200 Market Street, Apartment 4B, San Francisco, CA 94102, United States",
            "items": [{"sku": "TRAIL-RUNNER-42", "name": "Trail running shoe, size 42, slate blue", "qty": 1, "price": 129.0},
                      {"sku": "WOOL-SOCK-M", "name": "Merino wool hiking socks, medium, 3-pack", "qty": 2, "price": 24.5},
                      {"sku": "BOTTLE-750", "name": "Insulated steel water bottle, 750 ml", "qty": 1, "price": 32.0}]}


@trace_app
def respond(system, history, photo=None):
    messages = [{"role": "system", "content": f"{system}\n{ORDER_FORMAT}"}]
    if photo:
        messages.append({"role": "user", "content": [
            {"type": "text", "text": "Here is a photo of the damaged item from my order:"},
            {"type": "image_url", "image_url": {"url": f"data:image/jpeg;base64,{photo}"}}]})
    messages += history
    reply = model.chat.completions.create(model=MODEL, messages=messages)
    return json.loads(reply.choices[0].message.content)


@trace_app
def converse(system, users, photo=None, files=None):
    history, calls = [], 0
    for u in users:
        history.append({"role": "user", "content": u})
        action = respond(system, list(history), photo)
        calls += 1
        if action["kind"] == "tool" and files:
            # The agent variant: the tool reads a file from disk, as a coding agent does.
            history.append({"role": "assistant", "content": f"Reading {action['text']}."})
            with open(os.path.join(files, action["text"])) as f:
                history.append({"role": "user", "content": f"Tool read_file returned:\n{f.read()}"})
            action = respond(system, list(history), photo)
            calls += 1
        elif action["kind"] == "tool":
            history.append({"role": "assistant", "content": f"Looking up order {action['text']}."})
            history.append({"role": "user", "content": f"Tool lookup_order returned: {json.dumps(lookup_order(action['text']))}"})
            action = respond(system, list(history), photo)
            calls += 1
        history.append({"role": "assistant", "content": action["text"]})
    return calls


if STACK == "bridge":
    # setup:bridge
    import baml_sdk
    # end:bridge
    hello = baml_sdk.Hello_sync
    classify = baml_sdk.Classify_sync
    extract = baml_sdk.Extract_sync
    agent = baml_sdk.Agent_sync


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("stack")
    parser.add_argument("app")
    parser.add_argument("--text", default="")
    parser.add_argument("--n", type=int, default=1)
    parser.add_argument("--vary", action="store_true")
    args = parser.parse_args()
    setup_ms = (time.perf_counter() - T0) * 1000
    fn = {"hello": lambda t: hello(), "classify": classify, "extract": extract, "agent": agent,
          "chat": lambda t: converse(*json.loads(t).values())}[args.app]
    timings = []
    for i in range(args.n):
        # With --vary, --text holds one document per call, separated by "|".
        docs = args.text.split("|")
        text = (docs[i] if len(docs) == args.n else f"{args.text} #{i}") if args.vary else args.text
        start = time.perf_counter_ns()
        fn(text)
        timings.append((time.perf_counter_ns() - start) / 1e6)
    start = time.perf_counter_ns()
    flush()
    flush_ms = (time.perf_counter_ns() - start) / 1e6
    import resource
    maxrss_mb = resource.getrusage(resource.RUSAGE_SELF).ru_maxrss / 2**20  # bytes on macOS
    print(json.dumps({"setup_ms": setup_ms, "call_ms": timings, "flush_ms": flush_ms, "maxrss_mb": maxrss_mb}))


if __name__ == "__main__":
    main()

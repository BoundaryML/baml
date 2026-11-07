"""E12: which ordinary support tickets does Logfire's default scrubbing hide?

A traced `classify(ticket_id, text)` function (the same `@logfire.instrument()`
decorator as the benchmark apps) sends each ticket to the mock model through
the instrumented OpenAI SDK. The spans go to the real Logfire project. Then the
SQL API shows what Logfire stored for the function argument and for the model
prompt.

    python experiments/scrub.py --out results/scrub.json
"""
import argparse
import json
import os
import subprocess
import sys
import time
import uuid
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "lab"))
from logfire_sql import query  # noqa: E402
from servers import mock_llm  # noqa: E402

TICKETS = [
    "Where is my package? The tracking page has not changed since Monday.",
    "I never got the password reset email, so I cannot check my order.",
    "Checkout keeps saying my session expired before I can pay.",
    "My credit card was charged twice for order 8812.",
    "The cookie banner covers the Buy button on mobile.",
    "Can you send me the docs for creating an API key?",
    "Please correct the author name printed on my invoice.",
    "We want to discuss authentication options for our SSO rollout.",
    "My secret santa gift arrived damaged.",
    "Is there a private key ring option for the office locker?",
]

APP = """
import json, os, sys
import logfire
from openai import OpenAI
logfire.configure(service_name="bench-scrub", console=False, metrics=False)
logfire.instrument_openai()
model = OpenAI(base_url=os.environ["MODEL_BASE_URL"], api_key="fixture")

@logfire.instrument()
def classify(ticket_id, text):
    reply = model.chat.completions.create(model="gpt-4.1-mini", messages=[
        {"role": "user", "content": f"Is this ticket about an order? Reply true or false.\\n{text}"}])
    return reply.choices[0].message.content

for ticket_id, text in json.loads(sys.argv[1]):
    classify(ticket_id, text)
logfire.force_flush()
"""


def main():
    p = argparse.ArgumentParser()
    p.add_argument("--out", required=True)
    args = p.parse_args()
    secrets = dict(line.strip().split("=", 1) for line in open(ROOT / ".env") if "=" in line)
    run = uuid.uuid4().hex[:8]
    tickets = [(f"{run}-{i}", t) for i, t in enumerate(TICKETS)]
    model = mock_llm()
    env = {**os.environ, "LOGFIRE_TOKEN": secrets["LOGFIRE_TOKEN"], "MODEL_BASE_URL": model.url + "/v1"}
    subprocess.run([str(ROOT / "env/py/bin/python"), "-c", APP, json.dumps(tickets)], env=env, check=True)
    model.shutdown()
    rows = []
    for _ in range(60):
        time.sleep(5)
        rows = query(
            "select trace_id, span_name, attributes->>'ticket_id' as ticket_id, attributes->>'text' as text, "
            "attributes->>'gen_ai.input.messages' as gen_in, attributes->>'request_data' as request_data, "
            "attributes->>'logfire.scrubbed' as scrubbed from records "
            f"where service_name = 'bench-scrub' and start_timestamp > now() - interval '30 minutes' "
            f"and trace_id in (select trace_id from records where attributes->>'ticket_id' like '{run}-%')")
        if len({r["ticket_id"] for r in rows if r["ticket_id"]}) == len(TICKETS) and len(rows) >= 2 * len(TICKETS):
            break
    by_trace = {}
    for r in rows:
        by_trace.setdefault(r["trace_id"], []).append(r)
    out = []
    for i, (ticket_id, text) in enumerate(tickets):
        spans = next((s for s in by_trace.values() if any(r["ticket_id"] == ticket_id for r in s)), [])
        fn = next((r for r in spans if r["ticket_id"] == ticket_id), {})
        llm = [r for r in spans if r is not fn]
        prompt = json.dumps([r.get("gen_in") or r.get("request_data") for r in llm])
        out.append({"ticket": text, "stored_argument": fn.get("text"), "argument_scrubbed": fn.get("text") != text,
                    "scrubbed_reason": fn.get("scrubbed"), "prompt_kept_text": text in prompt.replace('\\"', '"'),
                    "llm_spans": len(llm)})
        print(out[-1], flush=True)
    (ROOT / args.out).write_text(json.dumps(out, indent=1) + "\n")


if __name__ == "__main__":
    main()

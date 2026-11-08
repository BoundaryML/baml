"""The scripted support conversation used by experiments/chat.py.

Every stack plays the same user messages against the same mock model, so the
model traffic is the same and only the tracing differs. Text comes from the
English corpus (lab/corpus.py), so it compresses like real prose.
"""
import hashlib
import json

from corpus import document

TOOL_MARK = "Where is my order #"

SYSTEM = (
    "You are the customer support agent for Acme Outfitters, an online store for running and hiking gear. "
    "Be friendly and brief. When a customer asks about an order, call the lookup_order tool with the order "
    "number before you answer, and never guess a shipping status. Refunds are allowed within 30 days of delivery. "
    "Escalate to a human when the customer asks for one twice.\n\nReference notes for agents:\n"
    + document(3500, 424242, "english")
)


def users(turns, seed=1):
    """One user message per turn. Every other turn asks about an order, which triggers a tool call."""
    out = []
    for t in range(turns):
        text = document(240, seed * 10_000 + t, "english")
        out.append(f"{TOOL_MARK}{4100 + t}? {text}" if t % 2 == 0 else f"Thanks. One more question: {text}")
    return out


def _content(message):
    c = message.get("content")
    if isinstance(c, list):
        return "".join(p.get("text", "") for p in c if isinstance(p, dict))
    return str(c)


def reply_rule(messages):
    """Mock model: ask for a lookup when the last message is an order question, else answer."""
    last = _content(messages[-1])
    # BAML merges consecutive user messages (the photo and the first question), so search the whole message.
    if messages[-1].get("role") == "user" and TOOL_MARK in last:
        order = last.split(TOOL_MARK, 1)[1].split("?")[0]
        return json.dumps({"kind": "tool", "text": order})
    seed = int(hashlib.sha256(last.encode()).hexdigest()[:8], 16)
    return json.dumps({"kind": "reply", "text": document(600, seed, "english")})


reply_rule.messages = True


# A system prompt over BCS's 16 KiB string threshold, as in a bot that carries a policy manual.
SYSTEM_BIG = SYSTEM + "\n\nPolicy manual:\n" + document(16_500, 515151, "english")


def photo_b64(seed, size=150_000):
    """An incompressible stand-in for a 150 KB JPEG, base64-encoded (about 200 KB)."""
    import base64
    import random
    return base64.b64encode(random.Random(seed).randbytes(size)).decode()

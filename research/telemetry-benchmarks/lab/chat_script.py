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
    if messages[-1].get("role") == "user" and last.startswith(TOOL_MARK):
        order = last[len(TOOL_MARK):].split("?")[0]
        return json.dumps({"kind": "tool", "text": order})
    seed = int(hashlib.sha256(last.encode()).hexdigest()[:8], 16)
    return json.dumps({"kind": "reply", "text": document(600, seed, "english")})


reply_rule.messages = True

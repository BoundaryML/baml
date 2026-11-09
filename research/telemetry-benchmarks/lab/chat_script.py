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


def users(turns, seed=1, chars=240, tools=True):
    """One user message per turn. Every other turn asks about an order, which triggers a tool call."""
    out = []
    for t in range(turns):
        text = document(chars, seed * 10_000 + t, "english")
        out.append(f"{TOOL_MARK}{4100 + t}? {text}" if tools and t % 2 == 0 else f"Thanks. One more question: {text}")
    return out


def _content(message):
    c = message.get("content")
    if isinstance(c, list):
        return "".join(p.get("text", "") for p in c if isinstance(p, dict))
    return str(c)


def make_reply_rule(reply_chars=600):
    def reply_rule(messages):
        """Mock model: ask for a lookup or a file when the last message asks for one, else answer."""
        last = _content(messages[-1])
        if messages[-1].get("role") == "user":
            # BAML merges consecutive user messages (the photo and the first question), so search the whole message.
            if TOOL_MARK in last:
                return json.dumps({"kind": "tool", "text": last.split(TOOL_MARK, 1)[1].split("?")[0]})
            if FILE_MARK in last:
                return json.dumps({"kind": "tool", "text": last.split(FILE_MARK, 1)[1].split(" ")[0]})
        seed = int(hashlib.sha256(last.encode()).hexdigest()[:8], 16)
        return json.dumps({"kind": "reply", "text": document(reply_chars, seed, "english")})
    reply_rule.messages = True
    return reply_rule


reply_rule = make_reply_rule()


# A system prompt over BCS's 16 KiB string threshold, as in a bot that carries a policy manual.
SYSTEM_BIG = SYSTEM + "\n\nPolicy manual:\n" + document(16_500, 515151, "english")


def photo_b64(seed, size=150_000):
    """An incompressible stand-in for a 150 KB JPEG, base64-encoded (about 200 KB)."""
    import base64
    import random
    return base64.b64encode(random.Random(seed).randbytes(size)).decode()


# The agent variant: a coding agent that reads one file per turn. Sizes cycle
# through values below and above BCS's 16 KiB string threshold; 48 turns end
# near 2 MB of context, about 500k tokens.
FILE_MARK = "Please read "
FILE_KB = [6, 40, 12, 90, 3, 64, 20, 150, 10, 30]
SYSTEM_FILES = (
    "You are a coding assistant working in the customer's repository. When the user asks about a file, "
    "call the read_file tool with its name before you answer, and quote line numbers when you refer to code.\n\n"
    "Reference notes:\n" + document(3500, 434343, "english")
)


def write_files(directory, turns, seed):
    """One file per turn, fresh text per run so hosted storage can't reuse an earlier run's blobs."""
    from pathlib import Path
    from corpus import _corpus
    # Consecutive slices of the corpus, so no two files of one run share text.
    text = _corpus()
    total = sum(FILE_KB[t % len(FILE_KB)] * 1000 for t in range(turns))
    assert total < len(text), "corpus too small for this many turns"
    at = (seed * 7919) % (len(text) - total)
    names = []
    for t in range(turns):
        size = FILE_KB[t % len(FILE_KB)] * 1000
        name = f"module_{t:02d}.py"
        Path(directory, name).write_text(text[at:at + size])
        at += size
        names.append(name)
    return names


def file_users(names, seed):
    return [f"{FILE_MARK}{n} and tell me what it does. {document(200, seed * 10_000 + t, 'english')}"
            for t, n in enumerate(names)]

#!/usr/bin/env python3
"""Mock OpenAI-compatible chat-completions server for the BTEL query demo.

Answers any POST with a canned completion chosen from the prompt text:
reviews mentioning "angry" get HTTP 500 (so the LLM function throws), other
reviews get a JSON verdict, and summaries get plain text.

Usage: mock_llm.py PORT
"""
import json
import sys
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer


def reply(prompt: str):
    if "angry" in prompt:
        return 500, {"error": {"message": "mock upstream failure", "type": "server_error"}}
    if "Summarize" in prompt:
        content = "Customers like the support; delivery is slow."
    else:
        positive = any(word in prompt for word in ("great", "friendly", "again"))
        content = json.dumps({
            "sentiment": "positive" if positive else "neutral",
            "score": 0.92 if positive else 0.55,
            "tags": ["support", "speed"] if positive else ["delivery"],
        })
    return 200, {
        "id": "mock",
        "object": "chat.completion",
        "model": "demo-model",
        "choices": [{
            "index": 0,
            "finish_reason": "stop",
            "message": {"role": "assistant", "content": content},
        }],
        "usage": {"prompt_tokens": 10, "completion_tokens": 10, "total_tokens": 20},
    }


class Handler(BaseHTTPRequestHandler):
    def do_POST(self):
        body = json.loads(self.rfile.read(int(self.headers.get("content-length", 0))) or b"{}")
        prompt = json.dumps(body.get("messages", body))
        status, payload = reply(prompt)
        data = json.dumps(payload).encode()
        self.send_response(status)
        self.send_header("content-type", "application/json")
        self.send_header("content-length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def log_message(self, *args):
        pass


if __name__ == "__main__":
    ThreadingHTTPServer(("127.0.0.1", int(sys.argv[1])), Handler).serve_forever()

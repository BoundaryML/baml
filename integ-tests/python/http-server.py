import json
from http.server import SimpleHTTPRequestHandler, HTTPServer


class StaticResponseHandler(SimpleHTTPRequestHandler):
    def do_POST(self):
        # Handle any path
        # response = {
        #     "id": "msg_123",
        #     "type": "message",
        #     "role": "assistant",
        #     "content": [
        #         {
        #             "type": "text",
        #             "text": "Hello world"
        #         }
        #     ],
        #     "model": "gpt-4o",
        #     "stop_reason": "end_turn",
        #     "stop_sequence": None,
        #     "usage": {
        #         "input_tokens": 10,
        #         "output_tokens": 20
        #     }
        # }

        response = {
            "id": "chat-fecaba633f1e41c6ab1aac77ad38c2f3",
            "object": "chat.completion",
            "created": 1743172689,
            "model": "Molmo-7B-D-0924",
            "choices": [
                {
                    "index": 0,
                    "message": {
                        "role": "assistant",
                        "content": " Yes",
                        "tool_calls": [],
                    },
                    "logprobs": {
                        "content": [
                            {
                                "token": " Yes",
                                "logprob": -0.034360505640506744,
                                "bytes": [32, 89, 101, 115],
                                "top_logprobs": [],
                            },
                            {
                                "token": "",
                                "logprob": -0.5018682479858398,
                                "bytes": [],
                                "top_logprobs": [],
                            },
                        ]
                    },
                    "finish_reason": "stop",
                    "stop_reason": None,
                }
            ],
            "usage": {
                "prompt_tokens": 433,
                "total_tokens": 435,
                "completion_tokens": 2,
            },
            "prompt_logprobs": None,
        }

        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.end_headers()
        self.wfile.write(json.dumps(response).encode("utf-8"))


def run(server_class=HTTPServer, handler_class=StaticResponseHandler, port=8000):
    server_address = ("", port)
    httpd = server_class(server_address, handler_class)
    print(f"Serving on port {port}...")
    httpd.serve_forever()


if __name__ == "__main__":
    run()

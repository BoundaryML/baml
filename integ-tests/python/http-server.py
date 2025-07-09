import json
from http.server import SimpleHTTPRequestHandler, HTTPServer


"""
This function is used to test various client_response_type.
It basically creates an HTTP server and returns the specified response for all routes.
Add/uncomment the JSON response you want to return.
"""
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

        # response = {
        #     "id": "chat-fecaba633f1e41c6ab1aac77ad38c2f3",
        #     "object": "chat.completion",
        #     "created": 1743172689,
        #     "model": "Molmo-7B-D-0924",
        #     "choices": [
        #         {
        #             "index": 0,
        #             "message": {
        #                 "role": "assistant",
        #                 "content": " Yes",
        #                 "tool_calls": [],
        #             },
        #             "logprobs": {
        #                 "content": [
        #                     {
        #                         "token": " Yes",
        #                         "logprob": -0.034360505640506744,
        #                         "bytes": [32, 89, 101, 115],
        #                         "top_logprobs": [],
        #                     },
        #                     {
        #                         "token": "",
        #                         "logprob": -0.5018682479858398,
        #                         "bytes": [],
        #                         "top_logprobs": [],
        #                     },
        #                 ]
        #             },
        #             "finish_reason": "stop",
        #             "stop_reason": None,
        #         }
        #     ],
        #     "usage": {
        #         "prompt_tokens": 433,
        #         "total_tokens": 435,
        #         "completion_tokens": 2,
        #     },
        #     "prompt_logprobs": None,
        # }
        # object = json.loads(
        #     """
        #     {"id":"chatcmpl-95d2fc5f-0434-4c2a-98c0-0275212b6ae1","object":"chat.completion","created":1744657880,"model":"llama-3.1-8b-instant","choices":[{"index":0,"message":{"role":"assistant","content":"Hello! It's nice to meet you. Is there something I can help you with or would you like to chat?"},"logprobs":null,"finish_reason":"stop"}],"usage":{"queue_time":0.147255087,"prompt_tokens":40,"prompt_time":0.006949183,"completion_tokens":25,"completion_time":0.033333333,"total_tokens":65,"total_time":0.040282516},"usage_breakdown":{"models":null},"system_fingerprint":"fp_6b54d780f2","x_groq":{"id":"req_01jrtvd4tye14v6tchdwj65nce"}}
        #     """
        # )
        # https://platform.openai.com/docs/api-reference/responses-streaming/response/completed
#         object = json.loads(
#             """
#                 {
#   "id": "resp_67ccd3a9da748190baa7f1570fe91ac604becb25c45c1d41",
#   "object": "response",
#   "created_at": 1741476777,
#   "status": "completed",
#   "error": null,
#   "incomplete_details": null,
#   "instructions": null,
#   "max_output_tokens": null,
#   "model": "gpt-4o-2024-08-06",
#   "output": [
#     {
#       "type": "message",
#       "id": "msg_67ccd3acc8d48190a77525dc6de64b4104becb25c45c1d41",
#       "status": "completed",
#       "role": "assistant",
#       "content": [
#         {
#           "type": "output_text",
#           "text": "The image depicts a scenic landscape with a wooden boardwalk or pathway leading through lush, green grass under a blue sky with some clouds. The setting suggests a peaceful natural area, possibly a park or nature reserve. There are trees and shrubs in the background.",
#           "annotations": []
#         }
#       ]
#     }
#   ],
#   "parallel_tool_calls": true,
#   "previous_response_id": null,
#   "reasoning": {
#     "effort": null,
#     "summary": null
#   },
#   "store": true,
#   "temperature": 1,
#   "text": {
#     "format": {
#       "type": "text"
#     }
#   },
#   "tool_choice": "auto",
#   "tools": [],
#   "top_p": 1,
#   "truncation": "disabled",
#   "usage": {
#     "input_tokens": 328,
#     "input_tokens_details": {
#       "cached_tokens": 0
#     },
#     "output_tokens": 52,
#     "output_tokens_details": {
#       "reasoning_tokens": 0
#     },
#     "total_tokens": 380
#   },
#   "user": null,
#   "metadata": {}
# }
#             """
#         )

        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.end_headers()
        self.wfile.write(json.dumps(object).encode("utf-8"))


def run(server_class=HTTPServer, handler_class=StaticResponseHandler, port=8000):
    server_address = ("", port)
    httpd = server_class(server_address, handler_class)
    print(f"Serving on port {port}...")
    httpd.serve_forever()


if __name__ == "__main__":
    run()

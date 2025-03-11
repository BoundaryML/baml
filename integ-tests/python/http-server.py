import json
from http.server import SimpleHTTPRequestHandler, HTTPServer

class StaticResponseHandler(SimpleHTTPRequestHandler):

    def do_POST(self):
        # Handle any path
        response = {
            "id": "msg_123",
            "type": "message", 
            "role": "assistant",
            "content": [
                {
                    "type": "text",
                    "text": "Hello world"
                }
            ],
            "model": "gpt-4o",
            "stop_reason": "end_turn",
            "stop_sequence": None,
            "usage": {
                "input_tokens": 10,
                "output_tokens": 20
            }
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

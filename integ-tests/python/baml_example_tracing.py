import time
import threading
import asyncio
from dotenv import load_dotenv
from http.server import BaseHTTPRequestHandler, HTTPServer
import json
import os

load_dotenv()
os.environ['BOUNDARY_BASE_URL'] = 'http://localhost:4040'

import baml_py
from baml_client import b
from baml_client.types import NamedArgsSingleEnumList, NamedArgsSingleClass
from baml_client.tracing import trace, set_tags, flush, on_log_event

class TraceRequestHandler(BaseHTTPRequestHandler):

    @staticmethod
    def run_forever():
      address = ('', int(os.environ['BOUNDARY_BASE_URL'].rsplit(':', maxsplit=1)[1]))
      httpd = HTTPServer(address, TraceRequestHandler)
      print(f'Starting BAML event logger on {address}')
      httpd.serve_forever()


    def do_GET(self):
      print(f"Received GET request: {self.path}")
      # self.send_response(200)
      # self.send_header('Content-type', 'text/html')
      # #self.send_header('Content-type', 'application/json')
      # self.end_headers()
      # self.wfile.write('Hello, BAML!'.encode('utf-8'))
      self.send_error(404, "File not found")

    def do_POST(self):
      print(f"Received POST request: {self.path}")
      self.send_error(404, "File not found")
      return
      if self.path == '/log/v2':
          content_length = int(self.headers['Content-Length'])  # Get the size of data
          post_data = self.rfile.read(content_length)  # Read the data
          data = json.loads(post_data.decode('utf-8'))  # Decode it to string

          print(f"Received event log for {data}")
          print(json.dumps(data, indent=2))

          self.send_response(200)
          self.send_header('Content-type', 'application/json')
          self.end_headers()
          response = json.dumps({'message': 'Log received'})
          self.wfile.write(response.encode('utf-8'))  # Send response back to client
      else:
          self.send_error(404, "File not found")

async def main():
  print('Hello, BAML!')
  #TraceRequestHandler.run_forever()
  t = threading.Thread(target=TraceRequestHandler.run_forever, daemon=True)
  t.start()

  print('waiting for server to start')
  time.sleep(5)
  print('server has started ( i think )')

  @trace
  def sync_dummy_fn():
    time.sleep(.05)

  sync_dummy_fn()

  print('before flush')
  t_flush = threading.Thread(target=flush)
  print('wtf?')
  t_flush.start()
  print('after start before join')
  t_flush.join(timeout=5)
  print('after flush')
  
  assert False

if __name__ == '__main__':
  asyncio.run(main())
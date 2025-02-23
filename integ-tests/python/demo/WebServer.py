from flask import request
from baml_client.sync_client import b

from .AbstractWebServer import AbstractWebServer, methods
import time

class WebServer(AbstractWebServer):

    @methods(('POST', ))
    def generate(self):
        request_json = request.get_json()

        for _ in range(100):
            start = time.time()
            # wiki = requests.get('https://en.wikipedia.org/wiki/Special:Random')
            # wiki_page = subprocess.check_output(['curl', 'https://en.wikipedia.org/wiki/Special:Random'])
            response = b.TestOllama(request_json.get('phrase', 'foo bar baz'))
            # res = expensive_cpu_function()
            print('---: ', time.time() - start)

        return {'value': 10}

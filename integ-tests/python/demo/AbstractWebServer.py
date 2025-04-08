from abc import ABC

from flask import Flask
from flask_httpauth import HTTPBasicAuth


def set_prop(prop, value):
    def _set_prop(method):
        setattr(method, prop, value)

        return method

    return _set_prop


def methods(_methods):
    return set_prop('methods', _methods)


def path(_path):
    return set_prop('path', _path)


class AbstractWebServer(ABC):

    def __init__(self, host: str, port: int, username: str, password: str):
        self._app = app = Flask(__name__)
        app.json.ensure_ascii = False

        self._auth = HTTPBasicAuth()

        self._host = host
        self._port = port

        self._username = username
        self._password = password

        self._init_auth()

        print('Initializing api methods...')

        for method in dir(self):
            if not method.startswith('_') and method != 'run':
                self._init_api_method(method)

    def _init_auth(self):
        auth = self._auth

        _username = self._username
        _password = self._password

        @auth.verify_password
        def verify_password(username, password):
            # print(username, _username, password, _password, file = stderr)

            if _username is None or _password is None:
                raise ValueError('Cannot authenticate since reference username or password is not set')

            return username == _username and password == _password

    def _init_api_method(self, name):
        app = self._app
        auth = self._auth

        call = getattr(self, name)
        path = getattr(call, 'path', name)

        @app.route(f'/{path}', methods = getattr(call, 'methods'))
        @auth.login_required
        def method_wrapper(*args, **kwargs):
            return call(*args, **kwargs)

    def run(self):
        self._app.run(host = self._host, port = self._port)

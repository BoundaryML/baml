from click import group, option

from .WebServer import WebServer


@group()
def main():
    pass


@main.command()
@option('--port', type = int, default = 8080)
@option('--host', type = str, default = '0.0.0.0')
@option('--username', '-u', type = str, default = 'foo')
@option('--password', '-p', type = str, default = 'bar')
def start(host: str, port: int, username: str, password: str):
    WebServer(host = host, port = port, username = username, password = password).run()


if __name__ == '__main__':
    main()
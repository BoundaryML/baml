"""Root-owned lazy CLI cache. Builder children never inherit runtime credentials."""
import importlib.util
import json
import os
from pathlib import Path
import re
import signal
import socket
import socketserver
import struct
import subprocess
import sys
import tempfile

SOCKET = '/data/cli-build/request.sock'
ROOT = Path('/data/cli-cache')
VERSION = re.compile(r'[0-9]+\.[0-9]+\.[0-9]+(?:-[A-Za-z0-9.-]+)?\Z')
spec = importlib.util.spec_from_file_location('cache_cli', Path(__file__).with_name('cache-cli.py'))
cache = importlib.util.module_from_spec(spec)
spec.loader.exec_module(cache)


def cached(version):
    if not VERSION.fullmatch(version):
        raise ValueError('invalid CLI version')
    paths = sorted((ROOT/version).glob('*/baml-cli'))
    if len(paths) > 1:
        raise ValueError('ambiguous CLI version; source revision must be resolved by an operator')
    if paths:
        path = paths[0]
        if path.is_symlink() or path.resolve() != path or path.stat().st_uid != 0 or path.stat().st_mode & 0o222:
            raise ValueError('unsafe cached CLI')
        return str(path)
    return None


def ensure(version):
    publication_root = ROOT
    if version == 'canary':
        clean = {'PATH':'/usr/bin:/bin','HOME':'/nonexistent','GIT_CONFIG_NOSYSTEM':'1','GIT_CONFIG_GLOBAL':'/dev/null'}
        ref = subprocess.run(['/usr/bin/git','ls-remote','https://github.com/BoundaryML/baml.git','refs/heads/canary'], env=clean, capture_output=True, text=True, check=True, timeout=30).stdout.split()
        if len(ref) != 2 or not re.fullmatch('[0-9a-f]{40}',ref[0]) or ref[1] != 'refs/heads/canary':
            raise ValueError('invalid canary revision')
        revision = ref[0]
        # Canary builds sharing a version number must not make release lookups ambiguous.
        publication_root = ROOT/'canary'
        paths = list(publication_root.glob('*/'+revision+'/baml-cli'))
        if len(paths) == 1:
            path = paths[0]
            if path.is_symlink() or path.resolve()!=path or path.stat().st_uid!=0 or path.stat().st_mode & 0o222:raise ValueError('unsafe cached CLI')
            return str(path)
        version = 'canary:'+revision
        found = None
    else:
        found = cached(version)
    if found:
        return found
    env = {'PATH': '/usr/local/cargo/bin:/usr/local/bin:/usr/bin:/bin',
           'HOME': '/data/bootstrap/home', 'USER': 'builder', 'LOGNAME': 'builder',
           'LANG': 'C.UTF-8', 'CARGO_HOME': '/data/bootstrap/cargo',
           'RUSTUP_HOME': '/data/bootstrap/rustup', 'CARGO_INCREMENTAL': '0'}
    command = ['setpriv', '--reuid=1001', '--regid=1001', '--clear-groups',
               '--no-new-privs', '--bounding-set=-all', '/usr/bin/python3', '-I',
               '/usr/local/lib/atb2/build-version.py', version]
    # Wait for a complete, successful build before publishing any artifact.
    with tempfile.TemporaryFile() as artifact:
        child = subprocess.Popen(command, env=env, cwd='/', stdout=artifact,
                                 stderr=subprocess.DEVNULL, start_new_session=True)
        try:
            status = child.wait(timeout=2400)
            if status:
                raise subprocess.CalledProcessError(status, command)
        finally:
            # Cargo/build scripts must not survive a timeout and race the next miss.
            try:
                os.killpg(child.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            child.wait()
        artifact.seek(0)
        metadata = json.loads(artifact.readline(512))
        return str(cache.publish(publication_root, metadata['version'], metadata['revision'], artifact))


class Handler(socketserver.StreamRequestHandler):
    def handle(self):
        uid = struct.unpack('3i', self.request.getsockopt(socket.SOL_SOCKET, socket.SO_PEERCRED, 12))[1]
        if uid != 1000:
            return
        self.request.settimeout(5)
        version = self.rfile.readline(128).decode().strip()
        self.request.settimeout(None)
        try:
            response = {'path': ensure(version)}
        except Exception:
            response = {'error': 'requested CLI version could not be built or verified'}
        self.wfile.write((json.dumps(response)+'\n').encode())


def request(version):
    if version != 'canary' and not VERSION.fullmatch(version):
        raise ValueError('invalid CLI version')
    with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as connection:
        connection.settimeout(2500)
        connection.connect(SOCKET)
        connection.sendall((version+'\n').encode())
        response = json.loads(connection.makefile('rb').readline(4096))
        path = response.get('path')
        if not path or not Path(path).is_relative_to(ROOT if version == 'canary' else ROOT/version):
            raise ValueError(response.get('error', 'invalid cache response'))
        return path


if __name__ == '__main__':
    if len(sys.argv) == 2:
        print(request(sys.argv[1]))
    else:
        if os.geteuid() != 0:
            raise SystemExit('cache service must be started by bootstrap')
        cache.directory(ROOT)
        directory = Path(SOCKET).parent
        cache.directory(directory)
        if Path(SOCKET).is_symlink():
            raise SystemExit('unsafe cache service socket')
        Path(SOCKET).unlink(missing_ok=True)
        # Serialized requests mean simultaneous misses build each version only once.
        with socketserver.UnixStreamServer(SOCKET, Handler) as server:
            os.chown(SOCKET, 0, 1000)
            os.chmod(SOCKET, 0o660)
            server.serve_forever()

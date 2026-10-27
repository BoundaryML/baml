#!/usr/bin/env python3
"""OS fixtures only. Scenarios, wrapper invocations and assertions live in BAML."""
import hashlib
import http.server
import io
import json
import os
from pathlib import Path
import shutil
import sys
import tarfile
import tempfile
import time
import zipfile

TARGETS = ['aarch64-apple-darwin', 'x86_64-apple-darwin', 'aarch64-unknown-linux-gnu', 'aarch64-unknown-linux-musl', 'x86_64-unknown-linux-gnu', 'x86_64-unknown-linux-musl', 'x86_64-pc-windows-msvc', 'aarch64-pc-windows-msvc']
VERSIONS = ['0.11.0', '0.12.0', '0.13.0']
SUFFIX = '.exe' if os.name == 'nt' else ''


def write(path, content):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(content, encoding='utf-8')


def stub(path, identity, source):
    path.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(source, path)
    path.chmod(0o755)
    write(path.with_suffix('.identity'), identity)


def release(root, version, mode='valid', identity=None):
    source = Path(os.environ['WRAPPER_STUB'])
    cache = Path(os.environ.get('WRAPPER_E2E_ARCHIVES', str(root / 'cache')))
    cache.mkdir(parents=True, exist_ok=True)
    cache_key = mode + '-' + (identity or 'default')
    if all((cache / (cache_key + ext)).exists() for ext in ['.zip', '.tar.gz']):
        for ext in ['.zip', '.tar.gz']:
            shutil.copyfile(cache / (cache_key + ext), root / 'server' / (version + ext))
        return
    files = {'bin/baml-cli' + SUFFIX: source.read_bytes(), 'bin/baml-pack-host' + SUFFIX: b'pack host', 'assets/baml-vscode.vsix': b'vsix', 'assets/playground/index.html': b'html', 'assets/playground/assets/index.js': b'js', 'assets/playground/assets/index.css': b'css'}
    if identity:
        files['bin/baml-cli.identity'] = identity.encode()
    if mode == 'missing-cli':
        del files['bin/baml-cli' + SUFFIX]
    if mode == 'layout':
        del files['assets/baml-vscode.vsix']
    if mode == 'traversal':
        files['../escaped'] = b'bad'
    if mode == 'link':
        files['bin/link'] = b'link'
    for extension in ['.tar.gz', '.zip']:
        buffer = io.BytesIO()
        if extension == '.zip':
            with zipfile.ZipFile(buffer, 'w') as archive:
                for name, data in files.items():
                    entry = zipfile.ZipInfo(name)
                    entry.create_system = 3
                    entry.external_attr = (0o120777 if name == 'bin/link' else 0o100755) << 16
                    archive.writestr(entry, data)
        else:
            with tarfile.open(fileobj=buffer, mode='w:gz') as archive:
                for name, data in files.items():
                    entry = tarfile.TarInfo(name)
                    entry.mode = 0o755
                    entry.size = len(data)
                    if name == 'bin/link':
                        entry.type = tarfile.SYMTYPE
                        entry.linkname = '../../escaped'
                        entry.size = 0
                    archive.addfile(entry, io.BytesIO(data))
        data = buffer.getvalue()
        (root / 'server' / f'{version}{extension}').write_bytes(data)
        (cache / (cache_key + extension)).write_bytes(data)
    return files


def manifest(root, version, channel='canary', mode='valid'):
    base = (root / 'url').read_text()
    artifacts = {}
    for target in TARGETS:
        extension = '.zip' if 'windows' in target else '.tar.gz'
        archive = root / 'server' / f'{version}{extension}'
        artifacts[target] = {'url': f'{base}/{archive.name}', 'sha256': hashlib.sha256(archive.read_bytes()).hexdigest() if mode != 'checksum' else '0' * 64}
    result = {'schema': 99 if mode == 'schema' else 1, 'version': version, 'channel': channel, 'released_at': '2026-01-01T00:00:00Z', 'artifacts': {} if mode == 'target' else artifacts}
    return json.dumps(result)


def init(name):
    root = Path(tempfile.mkdtemp(prefix='baml-wrapper-' + name + '-')).resolve()
    for directory in ['home', 'baml', 'project/nested/deep', 'outside', 'behaviors', 'server/version']:
        (root / directory).mkdir(parents=True)
    write(root / 'invocations.jsonl', '')
    write(root / 'requests.jsonl', '')
    write(root / 'telemetry.jsonl', '')
    for identity in VERSIONS + ['L1', 'L2', 'A2', 'replacement']:
        write(root / 'behaviors' / (identity + '.json'), json.dumps({'args': [['check']], 'stdout': identity + '\n', 'stderr': '', 'exit_code': 0}))
    for version in VERSIONS:
        stub(root / 'baml/toolchains' / version / 'bin' / ('baml-cli' + SUFFIX), version, os.environ['WRAPPER_STUB'])
        write(root / 'baml/toolchains' / version / 'VERSION', version + '\n')
        release(root, version)
    for sub, identity in [('project/build', 'L1'), ('project/nested/deep/build', 'L2'), ('outside/build', 'L2'), ('home/build', 'L1'), ('project/space ünicode', 'L1')]:
        stub(root / sub / ('baml-cli' + SUFFIX), identity, os.environ['WRAPPER_STUB'])
    write(root / 'project/baml.toml', '[package]\nname = "fixture"\n')
    write(root / 'baml/config.toml', '[default]\nselector = "0.11.0"\n[update]\nauto_check = false\n')
    write(root / 'baml/state.toml', '[channels.canary]\nactive_version = "0.11.0"\nresolved_at = "fixture"\nmanifest_path = "fixture"\n[channels.nightly]\nactive_version = "0.12.0"\nresolved_at = "fixture"\nmanifest_path = "fixture"\n')
    environment = {key: value for key, value in os.environ.items() if not key.startswith(('BAML_', 'WRAPPER_'))}
    environment.update(HOME=str(root / 'home'), USERPROFILE=str(root / 'home'), BAML_HOME=str(root / 'baml'), WRAPPER_FIXTURE_ROOT=str(root), BAML_TELEMETRY_ENABLED='false', DO_NOT_TRACK='1')
    return {'root': str(root), 'cwd': str(root / 'project'), 'env': environment, 'suffix': SUFFIX, 'windows': os.name == 'nt'}


def serve(root):
    class Handler(http.server.BaseHTTPRequestHandler):
        def log_message(self, *_):
            pass
        def do_POST(self):
            body = self.rfile.read(int(self.headers['Content-Length']))
            with (root / 'telemetry.jsonl').open('a') as output:
                output.write(json.dumps(json.loads(body)) + '\n')
            delay = root / 'telemetry.delay'
            if delay.exists():
                time.sleep(float(delay.read_text()))
            status = root / 'telemetry.status'
            self.send_response(int(status.read_text()) if status.exists() else 204)
            self.send_header('Content-Length', '0')
            self.end_headers()
        def do_GET(self):
            with (root / 'requests.jsonl').open('a') as output:
                output.write(json.dumps(self.path) + '\n')
            path = root / 'server' / self.path.lstrip('/')
            if path.with_name(path.name + '.status').exists():
                self.send_error(int(path.with_name(path.name + '.status').read_text()))
                return
            if path.with_name(path.name + '.delay').exists():
                time.sleep(float(path.with_name(path.name + '.delay').read_text()))
            if not path.is_file():
                self.send_error(404)
                return
            data = path.read_bytes()
            self.send_response(200)
            self.send_header('Content-Length', str(len(data) + (1000 if path.with_name(path.name + '.disconnect').exists() else 0)))
            self.end_headers()
            try:
                self.wfile.write(data)
            except (BrokenPipeError, ConnectionResetError):
                pass
    server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Handler)
    server.daemon_threads = True
    url = f'http://127.0.0.1:{server.server_port}'
    write(root / 'url', url)
    for version in VERSIONS:
        write(root / 'server/version' / (version + '.json'), manifest(root, version))
    write(root / 'server/canary.json', manifest(root, VERSIONS[0]))
    write(root / 'server/nightly.json', manifest(root, VERSIONS[1], 'nightly'))
    print(url, flush=True)
    server.serve_forever()


def main():
    command = sys.argv[1]
    if command == 'init':
        print(json.dumps(init(sys.argv[2])))
        return
    root = Path(sys.argv[2])
    if command == 'serve':
        serve(root)
    elif command == 'remove':
        path = root / sys.argv[3]
        if path.is_dir() and not path.is_symlink():
            def writable(function, failing_path, error):
                Path(failing_path).chmod(0o755)
                function(failing_path)
            for attempt in range(40):
                try:
                    shutil.rmtree(path, onerror=writable)
                    break
                except PermissionError:
                    if attempt == 39:
                        raise
                    time.sleep(0.05)
        else:
            path.unlink(missing_ok=True)
    elif command == 'release':
        version, mode, identity = sys.argv[3:6]
        release(root, version, mode, identity or None)
        write(root / 'server/version' / (version + '.json'), manifest(root, version, mode=mode))
    elif command == 'channel':
        channel, version = sys.argv[3:5]
        write(root / 'server' / (channel + '.json'), manifest(root, version, channel))
    elif command == 'symlink':
        path, target = (root / p for p in sys.argv[3:5])
        path.unlink(missing_ok=True)
        path.symlink_to(target)
    elif command == 'copy':
        source, destination = sys.argv[3:5]
        shutil.copyfile(source, root / destination)
        (root / destination).chmod(0o755)
    elif command == 'wrapper':
        data = io.BytesIO()
        extension = '.zip' if os.name == 'nt' else '.tar.gz'
        source = Path(os.environ['WRAPPER_STUB']).read_bytes()
        name = 'bin/baml' + SUFFIX
        if os.name == 'nt':
            with zipfile.ZipFile(data, 'w') as archive:
                archive.writestr(name, source)
        else:
            with tarfile.open(fileobj=data, mode='w:gz') as archive:
                entry = tarfile.TarInfo(name)
                entry.mode, entry.size = 0o755, len(source)
                archive.addfile(entry, io.BytesIO(source))
        body = data.getvalue()
        write(root / 'server/wrapper.json', json.dumps({'schema': 1, 'version': '9.9.9', 'released_at': 'fixture', 'artifacts': {target: {'url': (root / 'url').read_text() + '/wrapper' + extension, 'sha256': '0' * 64 if len(sys.argv) > 3 and sys.argv[3] == 'checksum' else hashlib.sha256(body).hexdigest()} for target in TARGETS}}))
        (root / 'server' / ('wrapper' + extension)).write_bytes(body)
    elif command == 'chmod':
        (root / sys.argv[3]).chmod(int(sys.argv[4], 8))
    elif command == 'age':
        path = root / sys.argv[3]
        stamp = time.time() - int(sys.argv[4])
        os.utime(path, (stamp, stamp))
    else:
        raise ValueError(command)


if __name__ == '__main__':
    main()

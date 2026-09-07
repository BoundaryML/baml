"""Linux process/filesystem boundary and Claude-only credential broker.

The controller runs this root-owned file with python -I. No host HOME, process
namespace, application environment, or controller Git metadata enters bwrap.
"""
import contextlib
import fcntl

import importlib.util
import http.client
import http.server
import json
import os
from pathlib import Path
import secrets
import subprocess
import sys
import threading
import time

DATA = Path('/data')
SAFE_PATH = '/usr/local/cargo/bin:/usr/local/bin:/usr/bin:/bin'
CREDENTIALS = DATA / 'home/.claude/.credentials.json'


def clean_env():
    return {'PATH': SAFE_PATH, 'HOME': '/home/agent', 'LANG': 'C.UTF-8',
            'USER': 'atb2', 'LOGNAME': 'atb2', 'TERM': 'dumb',
            'GIT_CONFIG_GLOBAL': '/dev/null', 'GIT_CONFIG_SYSTEM': '/dev/null',
            'GIT_CONFIG_NOSYSTEM': '1', 'GIT_TERMINAL_PROMPT': '0',
            'CARGO_HOME': '/data/agent-cache/cargo', 'RUSTUP_HOME': '/usr/local/rustup',
            'CARGO_TARGET_DIR': '/data/agent-cache/target', 'CARGO_INCREMENTAL': '0',
            'CARGO_PROFILE_DEV_OPT_LEVEL': '1', 'CARGO_PROFILE_TEST_OPT_LEVEL': '1',
            'CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC': '1', 'DISABLE_AUTOUPDATER': '1'}


_session_spec = importlib.util.spec_from_file_location('atb2_session_home', Path(__file__).with_name('session_home.py'))
session_home = importlib.util.module_from_spec(_session_spec)
_session_spec.loader.exec_module(session_home)


def workspace(cwd):
    """Only individually mounted checkouts/scratch projects are exposed."""
    path = Path(cwd)
    real = path.resolve(strict=True)
    if real != path or not real.is_relative_to(DATA):
        raise ValueError('unsafe sandbox working directory')
    relative = real.relative_to(DATA).parts
    if len(relative) >= 2 and relative[0] in ('worktrees', 'repro-check'):
        root = DATA / relative[0] / relative[1]
        # A linked worktree exposes controller Git metadata; refuse it.
        if (root / '.git').is_symlink() or ((root / '.git').exists() and not (root / '.git').is_dir()):
            raise ValueError('sandbox needs an independent clone')
        return root, False
    if real == DATA / 'repo/baml_language':
        return DATA / 'repo', True
    raise ValueError('working directory is outside sandbox roots')


def command(cwd, argv, extra=None, conversation_home=None):
    root, readonly = workspace(cwd)
    args = ['/usr/bin/bwrap', '--unshare-user', '--unshare-pid', '--unshare-ipc',
            '--unshare-uts', '--die-with-parent', '--new-session', '--cap-drop', 'ALL',
            '--clearenv', '--proc', '/proc', '--dev', '/dev', '--tmpfs', '/tmp',
            '--dir', '/home/agent']
    for item in ('/usr', '/bin', '/lib', '/lib64', '/etc/ssl', '/etc/resolv.conf',
                 '/etc/hosts', '/etc/alternatives', '/etc/passwd', '/etc/group', '/etc/ld.so.cache'):
        if Path(item).exists(): args += ['--ro-bind', item, item]
    # Each workspace gets a separate writable cache. Never share executable
    # build output or Cargo configuration between unrelated agent sessions.
    cache = DATA / 'agent-cache' / root.relative_to(DATA)
    cache.mkdir(mode=0o700, parents=True, exist_ok=True)
    if cache.resolve() != cache:
        raise ValueError('unsafe workspace cache')
    args += ['--ro-bind' if readonly else '--bind', str(root), str(root),
             '--bind', str(cache), '/data/agent-cache']
    baseline = DATA / 'agent-cache/repo/target/debug/baml-cli'
    if root != DATA / 'repo' and baseline.is_file():
        args += ['--ro-bind', str(baseline), str(baseline)]
    # Shared versioned executables are root-published and read-only in every sandbox.
    cli_cache = DATA / 'cli-cache'
    if cli_cache.exists():
        if cli_cache.is_symlink() or cli_cache.stat().st_uid != 0 or cli_cache.stat().st_mode & 0o022:
            raise ValueError('unsafe shared CLI cache')
        args += ['--ro-bind', str(cli_cache), str(cli_cache)]
    # The installed compiler is safe to expose read-only to repro checks.
    if Path('/data/target/debug/baml-cli').is_file():
        args += ['--ro-bind', '/data/target/debug/baml-cli', '/data/target/debug/baml-cli']
    if conversation_home is not None:
        args += ['--bind', str(conversation_home), '/home/agent', '--bind', str(root), '/workspace']
        cwd = Path('/workspace') / Path(cwd).relative_to(root)
    env = clean_env()
    env.update(extra or {})
    for key, value in env.items(): args += ['--setenv', key, value]
    return args + ['--chdir', str(cwd), '--', *argv]


class CredentialStore:
    """Reads the machine's Claude login outside the agent mount namespace."""
    def __init__(self, path=CREDENTIALS):
        self.path = path
        self.lock = threading.Lock()

    def token(self):
        # Different conversations have separate broker processes. Serialize
        # refreshes across them as well as across this server's threads.
        with self.lock, open(self.path.with_name('.atb2-oauth.lock'), 'a', opener=lambda path, flags: os.open(path, flags | os.O_NOFOLLOW, 0o600)) as lock:
            fcntl.flock(lock, fcntl.LOCK_EX)
            data = json.loads(self.path.read_text())
            auth = data['claudeAiOauth']
            if auth.get('expiresAt', 0) < (time.time() + 90) * 1000:
                # Refresh stays outside the sandbox, using Claude Code's public
                # OAuth client. Neither refresh nor access token reaches it.
                conn = http.client.HTTPSConnection('platform.claude.com', timeout=30)
                try:
                    conn.request('POST', '/v1/oauth/token', json.dumps({
                        'grant_type': 'refresh_token', 'refresh_token': auth['refreshToken'],
                        'client_id': '9d1c250a-e61b-44d9-88ed-5944d1962f5e',
                    }), {'Content-Type': 'application/json'})
                    response = conn.getresponse()
                    raw = response.read(65536)
                    if response.status != 200: raise ValueError('Claude refresh failed')
                    result = json.loads(raw)
                    auth['accessToken'] = result['access_token']
                    auth['refreshToken'] = result.get('refresh_token', auth['refreshToken'])
                    auth['expiresAt'] = (time.time() + result['expires_in']) * 1000
                    tmp = self.path.with_name('.atb2-credentials-' + secrets.token_hex(12))
                    fd = os.open(tmp, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
                    with os.fdopen(fd, 'w') as f: json.dump(data, f)
                    os.replace(tmp, self.path)
                finally: conn.close()
            token = auth['accessToken']
            if not isinstance(token, str) or not token: raise ValueError('Claude login unavailable')
            return token


class BrokerHandler(http.server.BaseHTTPRequestHandler):
    protocol_version = 'HTTP/1.0'
    def log_message(self, *_): pass
    def do_POST(self):
        # Never a general proxy: fixed origin, paths, headers and body bound.
        if (self.path not in ('/v1/messages', '/v1/messages?beta=true', '/v1/messages/count_tokens', '/v1/messages/count_tokens?beta=true')
            or self.headers.get('Authorization') != 'Bearer ' + self.server.client_token):
            self.send_error(403); return
        started = False
        try:
            size = int(self.headers.get('Content-Length', '-1'))
            if size < 0 or size > 8 * 1024 * 1024 or self.headers.get('Transfer-Encoding'):
                self.send_error(413); return
            self.connection.settimeout(30)
            body = self.rfile.read(size)
            if len(body) != size: raise ValueError('incomplete body')
            headers = {'Authorization': 'Bearer ' + self.server.credentials.token(),
                       'Content-Type': 'application/json', 'anthropic-version': '2023-06-01'}
            for name in ('anthropic-beta', 'anthropic-dangerous-direct-browser-access', 'x-app', 'user-agent'):
                value = self.headers.get(name)
                if value: headers[name] = value
            conn = self.server.connect()
            try:
                conn.request('POST', self.path, body, headers)
                response = conn.getresponse()
                if response.status != 200:
                    # Authentication diagnostics must not reflect real credentials.
                    self.send_error(502, 'Claude upstream rejected request'); return
                self.send_response(200)
                self.send_header('Content-Type', response.getheader('Content-Type', 'application/json'))
                self.end_headers()
                started = True
                while True:
                    chunk = response.read1(65536)
                    if not chunk: break
                    self.wfile.write(chunk); self.wfile.flush()
            finally: conn.close()
        except Exception:
            # Never log request bodies, tokens, upstream diagnostics or errors.
            if started:
                self.close_connection = True
            else:
                with contextlib.suppress(Exception): self.send_error(502, 'Claude broker unavailable')


@contextlib.contextmanager
def broker(credentials=None):
    server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), BrokerHandler)
    server.daemon_threads = True
    server.client_token = 'sk-ant-oat01-' + secrets.token_urlsafe(32)
    server.credentials = credentials or CredentialStore()
    server.connect = lambda: http.client.HTTPSConnection('api.anthropic.com', timeout=300)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    try: yield server
    finally: server.shutdown(); server.server_close(); thread.join()


def main():
    if sys.platform != 'linux' or not Path('/usr/bin/bwrap').is_file():
        raise ValueError('Linux bubblewrap isolation is required')
    cwd, *argv = sys.argv[1:]
    if not argv: raise ValueError('missing sandbox command')
    transcript = os.environ.get('ATB2_TRANSCRIPT')
    with contextlib.ExitStack() as stack:
        output = None
        extra = {}
        home = None
        if argv[0] == 'claude':
            root, _ = workspace(cwd)
            argv, home = stack.enter_context(session_home.conversation(root, argv))
            proxy = stack.enter_context(broker())
            extra = {'ANTHROPIC_BASE_URL': 'http://127.0.0.1:' + str(proxy.server_port),
                     'CLAUDE_CODE_OAUTH_TOKEN': proxy.client_token,
                     'CLAUDE_CODE_SAFE_MODE': '1'}
            if transcript:
                path = Path(transcript)
                if not path.parent.resolve().is_relative_to(DATA / 'runs') or path.is_symlink():
                    raise ValueError('unsafe transcript path')
                fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_TRUNC | os.O_NOFOLLOW, 0o600)
                output = stack.enter_context(os.fdopen(fd, 'wb'))
        result = subprocess.run(command(cwd, argv, extra, home), env={'PATH': SAFE_PATH},
                                stdout=output, stderr=subprocess.STDOUT if output else None)
        return result.returncode


if __name__ == '__main__':
    try: sys.exit(main())
    except Exception:
        print('atb2: isolated command could not start', file=sys.stderr)
        sys.exit(126)

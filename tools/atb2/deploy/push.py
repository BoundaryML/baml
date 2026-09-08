"""Export objects without credentials, then push from fresh trusted Git metadata."""
import importlib.util
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile

spec = importlib.util.spec_from_file_location('atb2_sandbox', Path(__file__).with_name('sandbox.py'))
sandbox = importlib.util.module_from_spec(spec)
spec.loader.exec_module(sandbox)
scan_spec = importlib.util.spec_from_file_location('atb2_push_scan', Path(__file__).with_name('push-scan.py'))
push_scan = importlib.util.module_from_spec(scan_spec)
scan_spec.loader.exec_module(push_scan)
REPOSITORY = 'https://github.com/BoundaryML/baml.git'


def trusted_env(home, token):
    # No ambient Git configuration, askpass, SSH agent, executable PATH, or
    # repository-provided credential helper is used by the credential process.
    return {'PATH': '/usr/bin:/bin', 'HOME': str(home), 'LANG': 'C.UTF-8',
            'GIT_CONFIG_GLOBAL': '/dev/null', 'GIT_CONFIG_SYSTEM': '/dev/null',
            'GIT_CONFIG_NOSYSTEM': '1', 'GIT_TERMINAL_PROMPT': '0',
            'GH_TOKEN': token, 'GIT_CONFIG_COUNT': '4',
            'GIT_CONFIG_KEY_0': 'core.hooksPath', 'GIT_CONFIG_VALUE_0': '/dev/null',
            'GIT_CONFIG_KEY_1': 'credential.helper', 'GIT_CONFIG_VALUE_1': '',
            'GIT_CONFIG_KEY_2': 'credential.helper',
            'GIT_CONFIG_VALUE_2': '!/usr/bin/python3 -I /usr/local/lib/atb2/push.py credential',
            'GIT_CONFIG_KEY_3': 'protocol.allow', 'GIT_CONFIG_VALUE_3': 'never'}


def main():
    if sys.argv[1:] == ['credential']:
        # git appends get/store/erase, handled below by the shared entry check.
        return 1
    if len(sys.argv) >= 2 and sys.argv[1] == 'credential':
        fields = dict(line.rstrip('\n').split('=', 1) for line in sys.stdin if '=' in line)
        if (sys.argv[2:] == ['get'] and fields.get('protocol') == 'https'
            and fields.get('host') == 'github.com'):
            sys.stdout.write('username=x-access-token\npassword=' + os.environ['GH_TOKEN'] + '\n\n')
        return 0
    cwd, branch, expected, scan_base = sys.argv[1:]
    root, readonly = sandbox.workspace(cwd)
    if readonly or root != Path(cwd) or root.parent != Path('/data/worktrees'):
        raise ValueError('push requires an isolated checkout')
    if not re.fullmatch(r'[0-9a-f]{40}|', expected): raise ValueError('invalid expected head')
    token = os.environ.get('GH_TOKEN') or os.environ.get('ATB_GITHUB_TOKEN') or os.environ.get('GITHUB_TOKEN')
    if not token: raise ValueError('GitHub credential missing')
    with tempfile.TemporaryDirectory(prefix='atb2-push-') as tmp:
        folder = Path(tmp)
        env = trusted_env(folder, token)
        def git(*args, credentialed=False, **kwargs):
            command_env = env if credentialed else {k: v for k, v in env.items() if k != "GH_TOKEN"}
            return subprocess.run(['/usr/bin/git', *args], cwd=folder, env=command_env,
                                  check=True, stderr=subprocess.PIPE, timeout=600, **kwargs)
        git('check-ref-format', 'refs/heads/' + branch, stdout=subprocess.DEVNULL)
        # No controller command loads the checkout's .git/config. Both object
        # export operations run behind the same isolation boundary as the agent.
        def export(*args, **kwargs):
            return subprocess.run(sandbox.command(cwd, ['/usr/bin/git', '-c', 'core.hooksPath=/dev/null',
                '-c', 'core.fsmonitor=false', *args]), env={'PATH': sandbox.SAFE_PATH},
                check=True, stderr=subprocess.PIPE, timeout=600, **kwargs)
        commit = export('rev-parse', '--verify', 'HEAD^{commit}', stdout=subprocess.PIPE).stdout.decode().strip()
        if not re.fullmatch('[0-9a-f]{40}', commit): raise ValueError('invalid proposed commit')
        # Import object data only. Never copy configuration, hooks, refs, or
        # alternates from the agent checkout to the credential-bearing process.
        with (folder / 'objects.pack').open('w+b') as pack:
            export('pack-objects', '--stdout', '--revs', input=(commit + '\n').encode(), stdout=pack)
            pack.seek(0)
            git('init', '--bare', 'trusted.git', stdout=subprocess.DEVNULL)
            git('--git-dir=trusted.git', 'index-pack', '--stdin', '--strict', stdin=pack, stdout=subprocess.DEVNULL)
        git('--git-dir=trusted.git', 'cat-file', '-e', commit + '^{commit}', stdout=subprocess.DEVNULL)
        if expected:
            git('--git-dir=trusted.git', 'merge-base', '--is-ancestor', expected, commit, stdout=subprocess.DEVNULL)
        push_scan.scan(git, scan_base, commit, folder)
        # Exact source SHA, fixed URL and exact destination ref. No origin or
        # URL rewrites from the checkout; an empty lease only permits creation.
        git('--git-dir=trusted.git', '-c', 'protocol.https.allow=always', 'push',
            '--force-with-lease=refs/heads/' + branch + ':' + expected,
            REPOSITORY, commit + ':refs/heads/' + branch, credentialed=True, stdout=subprocess.DEVNULL)
        print(commit)
    return 0


if __name__ == '__main__':
    try: sys.exit(main())
    except Exception:
        print('atb2: trusted push failed; no credential diagnostics are shown', file=sys.stderr)
        sys.exit(1)

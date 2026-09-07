"""Fail closed on unsafe outgoing commits, using only imported Git objects."""
import os
from pathlib import Path
import re
import subprocess

SECRET = re.compile(rb'(?:-----BEGIN [A-Z ]*PRIVATE KEY-----|(?:gh[pousr]_|github_pat_|xox[baprs]-)[A-Za-z0-9_-]{16,}|AKIA[A-Z0-9]{16}|(?:/Users/|/home/)[A-Za-z0-9_.-]+/|/var/folders/)')
INSTALLER = re.compile(rb'(?:curl|wget)[^\n]*\|\s*(?:sudo\s+)?(?:ba)?sh\b')
DDL = re.compile(rb'\b(?:create|alter|drop)\s+(?:table|function|policy|trigger|index)\b', re.I)


def scan(git, base, commit, folder):
    if not re.fullmatch('[0-9a-f]{40}', base): raise ValueError('scan base is required')
    git('--git-dir=trusted.git', 'merge-base', '--is-ancestor', base, commit, stdout=subprocess.DEVNULL)
    revisions = git('--git-dir=trusted.git', 'rev-list', base + '..' + commit, stdout=subprocess.PIPE).stdout.splitlines()
    if not revisions or len(revisions) > 100: raise ValueError('outgoing commit count requires human review')
    # Scan every intermediate commit, including files later removed from HEAD.
    for revision in revisions:
        changes = git('--git-dir=trusted.git', 'diff-tree', '--no-commit-id', '--root', '-r', '-m', '--raw', '-z', revision.decode(), stdout=subprocess.PIPE).stdout.split(b'\0')
        for offset in range(0, len(changes) - 1, 2):
            fields = changes[offset].split()
            path = changes[offset + 1].decode('utf-8', errors='strict')
            if len(fields) != 5: raise ValueError('invalid tree record')
            if fields[4] == b'D': continue
            parts = Path(path).parts
            if any(p in ('.agents', '.claude', '.aws', '.ssh', 'node_modules', '__pycache__', '.cache') for p in parts) or path.endswith(('.sql', '.lock', '.pem', '.key', '.png', '.jpg')) or Path(path).name in ('skills-lock.json', 'package-lock.json', 'bun.lockb') or Path(path).name.startswith('.env'):
                raise ValueError('outgoing file requires human review')
            if fields[1] not in (b'100644', b'100755'): raise ValueError('special files require human review')
            blob = fields[3].decode()
            size = int(git('--git-dir=trusted.git', 'cat-file', '-s', blob, stdout=subprocess.PIPE).stdout)
            if size > 2_000_000: raise ValueError('large file requires human review')
            content = git('--git-dir=trusted.git', 'cat-file', 'blob', blob, stdout=subprocess.PIPE).stdout
            if b'\0' in content or SECRET.search(content) or INSTALLER.search(content) or DDL.search(content):
                raise ValueError('outgoing content requires human review')
            if re.search(rb'^(?:<{7}|={7}|>{7})(?: |$)', content, re.M): raise ValueError('conflict marker')
            if path.startswith('.github/workflows/'):
                for action in re.findall(rb'\buses:\s*[\x22\x27]?([^\s\x22\x27]+)', content):
                    if not action.startswith(b'./') and not re.search(rb'@[0-9a-f]{40}$|@sha256:[0-9a-f]{64}$', action):
                        raise ValueError('action must be pinned')
    # Bare import contains no agent-provided scan config. Token-free environment;
    # scanner failure is a failed push, never an optional warning.
    subprocess.run(['/usr/local/bin/infisical', 'scan', '--source', str(folder / 'trusted.git'),
                    '--log-opts=' + base + '..' + commit, '--redact', '--no-color', '--telemetry=false'],
                   cwd=folder, env={'PATH': '/usr/local/bin:/usr/bin:/bin', 'HOME': str(folder),
                                    'INFISICAL_DISABLE_UPDATE_CHECK': 'true'},
                   stdout=subprocess.PIPE, stderr=subprocess.PIPE, check=True, timeout=120)

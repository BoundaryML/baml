"""Credential-free builder child. Stdout is only the requested CLI executable."""
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys

version = sys.argv[1]
canary = re.fullmatch(r'canary:([0-9a-f]{40})', version)
if not canary and not re.fullmatch(r'[0-9]+\.[0-9]+\.[0-9]+(?:-[A-Za-z0-9.-]+)?', version):
    raise SystemExit('invalid version')
if os.geteuid() != 1001:
    raise SystemExit('builder UID required')
root = Path('/data/bootstrap')
pinned = root / 'target/debug/baml-cli'
def run(args, **kwargs):
    return subprocess.run(args, check=True, stdout=subprocess.PIPE, stderr=sys.stderr, **kwargs)
# Reuse the bootstrap compiler when it is exactly the requested version.
actual = run([str(pinned), '--version']).stdout.decode().strip().split()[-1] if pinned.exists() else ''
if actual == version:
    revision = (root / 'target/.baml-cli-rev').read_text().strip()
    binary = pinned
else:
    repo = root / 'version-repo'
    if not repo.exists():
        run(['git', 'clone', '--no-checkout', 'https://github.com/BoundaryML/baml.git', str(repo)])
    tag = canary.group(1) if canary else 'refs/tags/baml-language-' + version
    run(['git', '-C', str(repo), 'fetch', '--no-tags', 'origin', tag])
    revision = run(['git', '-C', str(repo), 'rev-parse', 'FETCH_HEAD^{commit}']).stdout.decode().strip()
    run(['git', '-C', str(repo), 'checkout', '--detach', revision])
    env = dict(os.environ, CARGO_TARGET_DIR=str(root / 'version-target' / revision))
    # Build diagnostics are sent to stderr; stdout remains a framed artifact.
    subprocess.run(['cargo', 'build', '-p', 'baml_cli', '--bin', 'baml-cli'],
                   cwd=repo/'baml_language', env=env, stdout=sys.stderr, check=True)
    binary = Path(env['CARGO_TARGET_DIR']) / 'debug/baml-cli'
    actual = run([str(binary), '--version']).stdout.decode().strip().split()[-1]
    if canary and revision != canary.group(1):
        raise SystemExit("canary revision changed")
    if not canary and actual != version:
        raise SystemExit('built CLI version does not match request')
if not re.fullmatch('[0-9a-f]{40}', revision):
    raise SystemExit('invalid source revision')
sys.stdout.buffer.write((json.dumps({'revision':revision,'version':actual}) + '\n').encode())
with binary.open('rb') as source:
    shutil.copyfileobj(source, sys.stdout.buffer)

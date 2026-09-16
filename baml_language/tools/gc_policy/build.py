#!/usr/bin/env python3
"""Build an immutable experiment binary with its source and build provenance."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess


def sha256(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def source_state(workspace):
    def git(*args):
        return subprocess.check_output(['git', *args], cwd=workspace)
    names = git('ls-files', '-z', '--cached', '--others', '--exclude-standard', '--', '.')
    files = {}
    for name in sorted(set(names.decode().split('\0')) - {''}):
        path = workspace / name
        files[name] = sha256(path) if path.is_file() else None
    return {
        'commit': git('rev-parse', 'HEAD').decode().strip(),
        'files': files,
        'tree_sha256': hashlib.sha256(json.dumps(files, sort_keys=True).encode()).hexdigest(),
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', required=True, type=Path,
                        help='New directory outside the checkout')
    parser.add_argument('--profile', default='fasttest')
    parser.add_argument('--features', default='gc_profiling')
    args = parser.parse_args()
    workspace = Path(__file__).resolve().parents[2]
    out = args.output.resolve()
    if out.is_relative_to(workspace):
        parser.error('Keep build artifacts outside the source checkout')
    out.mkdir(parents=True, exist_ok=False)
    before = source_state(workspace)
    diff = subprocess.check_output(['git', 'diff', '--binary', 'HEAD', '--', '.'], cwd=workspace)
    (out / 'source.patch').write_bytes(diff)
    untracked = subprocess.check_output(
        ['git', 'ls-files', '-z', '--others', '--exclude-standard', '--', '.'], cwd=workspace)
    for name in filter(None, untracked.decode().split('\0')):
        source = workspace / name
        if source.is_file():
            target = out / 'untracked' / name
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(source, target)
    command = ['cargo', 'test', '--locked', '-p', 'bex_engine', '--test',
               'gc_policy_experiment', '--profile', args.profile, '--no-run',
               '--message-format=json']
    if args.features:
        command += ['--features', args.features]
    with (out / 'build.jsonl').open('w') as stdout, (out / 'build.log').open('w') as stderr:
        result = subprocess.run(command, cwd=workspace, stdout=stdout, stderr=stderr)
    if result.returncode:
        raise SystemExit(f'Build failed; see {out / "build.log"}')
    if source_state(workspace) != before:
        raise SystemExit('Source changed during build; discard this build and retry')
    artifacts = [json.loads(line) for line in (out / 'build.jsonl').read_text().splitlines()]
    executables = [a['executable'] for a in artifacts
                   if a.get('reason') == 'compiler-artifact' and a.get('executable')
                   and a['target']['name'] == 'gc_policy_experiment']
    if len(executables) != 1:
        raise SystemExit(f'Expected one test executable, got {executables}')
    binary = out / 'experiment'
    shutil.copy2(executables[0], binary)
    manifest = dict(schema_version=1, source=before, command=command,
                    rustc=subprocess.check_output(['rustc', '-vV'], text=True),
                    cargo=subprocess.check_output(['cargo', '-V'], text=True).strip(),
                    platform=platform.platform(), libc=platform.libc_ver(),
                    allocator='Not inferred: record allocator overrides with the run',
                    build_environment={k: os.environ[k] for k in
                                       ['RUSTFLAGS', 'CARGO_ENCODED_RUSTFLAGS', 'CARGO_BUILD_TARGET']
                                       if k in os.environ},
                    binary_sha256=sha256(binary))
    (out / 'build-manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')
    print(binary)


if __name__ == '__main__':
    main()

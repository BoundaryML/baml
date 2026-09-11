#!/usr/bin/env python3
"""Build every BAML artifact on the host; Docker only assembles runtime images."""
import argparse
import functools
import hashlib
import http.server
import json
import os
from pathlib import Path
import shutil
import subprocess
import tarfile
import threading
import urllib.request

ROOT = Path(__file__).resolve().parents[1]
BUILD = ROOT / '.build'
GNU = 'aarch64-unknown-linux-gnu'
MUSL = 'aarch64-unknown-linux-musl'


def run(*args, cwd=ROOT, env=None):
    print('+', ' '.join(map(str, args)), flush=True)
    subprocess.run(list(map(str, args)), cwd=cwd, env=env, check=True)


def digest(path):
    result = hashlib.sha256()
    with path.open('rb') as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b''):
            result.update(chunk)
    return result.hexdigest()


def copy_tree(source, destination):
    if destination.exists():
        shutil.rmtree(destination)
    shutil.copytree(source, destination, ignore=shutil.ignore_patterns(
        '__pycache__', '*.pyc', '*.so', '*.dylib', '*.node', 'node_modules', 'dist', '.baml'))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--baml-source', type=Path,
                        default=ROOT.parents[1] / 'baml_language',
                        help='BAML language workspace (default: this repository’s baml_language)')
    parser.add_argument('--skip-rust', action='store_true', help='Reuse existing release artifacts after a previous build')
    args = parser.parse_args()
    source = args.baml_source.expanduser().resolve()
    if not source.is_dir():
        parser.error(f'BAML source not found: {source}. Pass --baml-source with a checkout supporting baml.sys.heap_stats().')
    revision = subprocess.run(['jj', '--ignore-working-copy', 'log', '-r', '@', '--no-graph', '-T', 'commit_id'],
                              cwd=source, text=True, capture_output=True)
    if revision.returncode == 0:
        revision_id = revision.stdout.strip()
    else:
        revision_id = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=source, text=True).strip()
    if args.skip_rust:
        manifest_path = BUILD / 'manifest.json'
        previous = json.loads(manifest_path.read_text()) if manifest_path.exists() else {}
        if previous.get('source') != str(source) or previous.get('revision') != revision_id:
            parser.error('--skip-rust requires a completed build from this checkout and revision. Run a normal build first.')
    target = source / 'target/local-hello-world'
    cli = source / 'target/release/baml-cli'
    BUILD.mkdir(exist_ok=True)
    # The CLI, pack host and bridges require matching bytecode fingerprints.
    # Pin it once so a commit during a long build cannot split the toolchain.
    env = dict(os.environ, BAML_GIT_SHA=revision_id, BAML_PROFILE='0', BAML_TELEMETRY_DISABLED='1', BAML_LOG='off',
               CARGO_TARGET_DIR=str(source / 'target'))
    build_tools = BUILD / 'build-tools'
    if not (build_tools / 'bin/cargo-zigbuild').exists():
        run('uv', 'venv', build_tools)
        run('uv', 'pip', 'install', '--python', build_tools / 'bin/python', 'cargo-zigbuild==0.23.2')
    env['PATH'] = str(build_tools / 'bin') + os.pathsep + env['PATH']
    cross_env = dict(env, CARGO_TARGET_DIR=str(target), PYO3_CROSS='1',
                     PYO3_CROSS_PYTHON_VERSION='3.10', PYO3_BUILD_EXTENSION_MODULE='1')
    if not args.skip_rust:
        run('rustup', 'target', 'add', GNU, MUSL, cwd=source)
        run('cargo', 'build', '--release', '--locked', '-p', 'baml_cli', '--bin', 'baml-cli', cwd=source, env=env)
        run('cargo', 'zigbuild', '--release', '--locked', '--target', GNU + '.2.34',
            '-p', 'baml_pack_host', '--bin', 'baml-pack-host', cwd=source, env=cross_env)
        run('cargo', 'zigbuild', '--release', '--locked', '--target', GNU + '.2.34',
            '-p', 'bridge_python', '--lib', cwd=source, env=cross_env)

    toolchain = BUILD / 'toolchain'
    toolchain.mkdir(exist_ok=True)
    shutil.copy2(cli, toolchain / 'baml-cli')
    archives = BUILD / 'archives'
    archives.mkdir(exist_ok=True)
    # Cross-target `pack` normally downloads a release. Serve ONLY our local
    # cross-built hosts through its supported release URL override instead.
    for triple in (GNU,):
        archive = archives / f'baml-language-local-{triple}.tar.gz'
        with tarfile.open(archive, 'w:gz') as tar:
            tar.add(target / triple / 'release/baml-pack-host', arcname='baml-pack-host')
        Path(str(archive) + '.sha256').write_text(f'{digest(archive)}  {archive.name}\n')
    handler = functools.partial(http.server.SimpleHTTPRequestHandler, directory=str(archives))
    server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), handler)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    pack_env = dict(env, BAML_PACK_HOST_RELEASE_VERSION='local',
                    BAML_PACK_HOST_RELEASE_BASE_URL=f'http://127.0.0.1:{server.server_port}')
    try:
        for name, triple in [('baml-debian', GNU)]:
            dest = BUILD / name
            dest.mkdir(exist_ok=True)
            for baml_file in sorted((ROOT / name / 'baml_src').glob('*.baml')):
                run(cli, 'fmt', baml_file, '--agent-skill-check', 'off', cwd=ROOT / name, env=env)
            run(cli, 'pack', 'main', '--target', triple, '--output', dest / 'hello',
                '--agent-skill-check', 'off', cwd=ROOT / name, env=pack_env)
    finally:
        server.shutdown()
        server.server_close()

    for name in ('python-baml', 'node-baml'):
        run(cli, 'fmt', 'baml_src/main.baml', '--agent-skill-check', 'off', cwd=ROOT / name, env=env)
        run(cli, 'generate', '--agent-skill-check', 'off', cwd=ROOT / name, env=env)

    # Python wrapper source and the abi3 extension both come from this checkout.
    py_dest = BUILD / 'python-baml/baml_bridge'
    py_dest.parent.mkdir(exist_ok=True)
    copy_tree(source / 'sdks/python/src/baml_bridge', py_dest)
    shutil.copy2(target / GNU / 'release/libbaml_py.so', py_dest / 'baml_py.abi3.so')
    for name in ('python-baseline', 'python-baml'):
        wheels = BUILD / name / 'wheels'
        if wheels.exists():
            shutil.rmtree(wheels)
        wheels.mkdir(parents=True, exist_ok=True)
        # pip's target flags do not change dependency environment markers;
        # use Python 3.10 on the host too (e.g. anyio needs exceptiongroup).
        run('uv', 'run', '--no-project', '--python', '3.10', '--with', 'pip==26.2.1', 'python', '-m', 'pip', 'download',
            '--only-binary=:all:', '--platform', 'manylinux2014_aarch64', '--python-version', '310',
            '--implementation', 'cp', '--abi', 'cp310', '--dest', wheels, '-r', ROOT / name / 'requirements.txt')

    # Compile the TypeScript wrapper on macOS in an isolated copy of the package.
    bridge = BUILD / 'node-bridge'
    copy_tree(source / 'sdks/typescript/bridge_typescript', bridge)
    run('npm', 'install', '--ignore-scripts', '--no-audit', '--no-fund', cwd=bridge)
    proto = source / 'crates/bridge_ctypes/types'
    run('node', 'node_modules/protobufjs-cli/bin/pbjs', '-t', 'static-module', '-w', 'es6',
        '-p', proto, '-o', 'typescript_src/proto/baml_cffi.js',
        proto / 'baml_bridge/cffi/v1/baml_inbound.proto', proto / 'baml_bridge/cffi/v1/baml_outbound.proto', cwd=bridge)
    run('node', 'node_modules/protobufjs-cli/bin/pbts', '-o', 'typescript_src/proto/baml_cffi.d.ts',
        'typescript_src/proto/baml_cffi.js', cwd=bridge)
    # napi-rs emits the native loader and declarations from the Rust source.
    # All outputs go to staging; source SDK directories remain untouched.
    run(bridge / 'node_modules/.bin/napi', 'build', '--manifest-path',
        source / 'sdks/typescript/bridge_typescript/Cargo.toml', '--package-json-path', bridge / 'package.json',
        '--output-dir', bridge / 'dist', '--js', 'native.js', '--dts', 'native.d.ts',
        '--platform', '--esm', '--release', '--cross-compile', '--target', MUSL,
        cwd=source, env=dict(cross_env, RUSTFLAGS='-C target-feature=-crt-static'))
    shutil.copy2(bridge / 'dist/native.d.ts', bridge / 'typescript_src/native.d.ts')
    # This checkout's wrapper has existing HandleKey vs protobuf Long type
    # errors. Transpile its unchanged runtime source; type-check the generated
    # demo SDK below, then verify the actual local native bridge in Docker.
    run('npx', 'tsc', '-p', 'tsconfig.json', '--noCheck', cwd=bridge)
    run('node', 'typescript_src/copy-proto.js', cwd=bridge)
    # Upstream's final packaging step also fixes protobuf's Node ESM import.
    run('node', 'typescript_src/tag-generated-files.js', cwd=bridge)
    for name in ('node-baseline', 'node-baml'):
        app = ROOT / name
        run('npm', 'install' if name == 'node-baml' else 'ci', '--install-links',
            '--ignore-scripts', '--no-audit', '--no-fund', cwd=app)
        if name == 'node-baml':
            run('npx', 'tsc', cwd=app)
        run('npm', 'prune', '--install-links', '--omit=dev', '--ignore-scripts', '--no-audit', '--no-fund', cwd=app)
        dest = BUILD / name
        dest.mkdir(exist_ok=True)
        # Dereference the local npm link so images don't depend on host paths.
        if (dest / 'node_modules').exists():
            shutil.rmtree(dest / 'node_modules')
        shutil.copytree(app / 'node_modules', dest / 'node_modules', symlinks=False,
                        ignore=shutil.ignore_patterns('typescript', '@types', '.bin'))
        if name == 'node-baml':
            # npm may cache a file dependency when its version is unchanged.
            # Always stage this build's wrapper and native addon explicitly.
            installed = dest / 'node_modules/@boundaryml/baml-bridge'
            shutil.rmtree(installed / 'dist')
            shutil.copy2(bridge / 'package.json', installed / 'package.json')
            shutil.copytree(bridge / 'dist', installed / 'dist')

    # Vegeta is a pinned Linux executable, fetched and verified on the host.
    tools = BUILD / 'tools'
    tools.mkdir(exist_ok=True)
    filename = 'vegeta_12.13.0_linux_arm64.tar.gz'
    base = 'https://github.com/tsenart/vegeta/releases/download/v12.13.0/'
    archive = tools / filename
    urllib.request.urlretrieve(base + filename, archive)
    checksums = urllib.request.urlopen(base + 'vegeta_12.13.0_checksums.txt').read().decode()
    expected = next(line.split()[0] for line in checksums.splitlines() if line.split()[-1] == filename)
    if digest(archive) != expected:
        raise RuntimeError('Vegeta checksum mismatch')
    with tarfile.open(archive) as tar:
        (tools / 'vegeta').write_bytes(tar.extractfile('vegeta').read())
    (tools / 'vegeta').chmod(0o755)

    artifacts = [toolchain / 'baml-cli', py_dest / 'baml_py.abi3.so',
                 bridge / 'dist/baml_node.linux-arm64-musl.node', tools / 'vegeta']
    artifacts.append(BUILD / 'baml-debian/hello')
    manifest = {'source': str(source), 'revision': revision_id,
                'note': 'Includes the current checkout contents; revision alone does not identify uncommitted changes.',
                'platform': 'linux/arm64',
                'rustc': subprocess.check_output(['rustc', '--version'], cwd=source, text=True).strip(),
                'profile': 'release', 'gnu_glibc': '2.34',
                'pack_host_sha256': {triple: digest(target / triple / 'release/baml-pack-host') for triple in (GNU,)},
                'sha256': {str(p.relative_to(BUILD)): digest(p) for p in artifacts}}
    (BUILD / 'manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')
    print('Host artifacts ready. Run ./scripts/compose.sh up -d --build')


if __name__ == '__main__':
    main()

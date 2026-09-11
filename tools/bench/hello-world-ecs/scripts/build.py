#!/usr/bin/env python3
"""Build ten architecture-specific workload images plus one x64 load image."""
import argparse
import datetime
import json
from pathlib import Path
import shlex
import subprocess

ROOT = Path(__file__).resolve().parents[1]
MATRIX = json.loads((ROOT / 'matrix.json').read_text())


def builds(repository, tag):
    for variant in MATRIX['variants']:
        for arch, spec in MATRIX['architectures'].items():
            name = f'{variant}-{arch}'
            yield name, spec['docker'], f'apps/{variant}/Dockerfile', f'{repository}:{tag}-{name}'
    yield 'load', 'amd64', 'load-generator/Dockerfile', f'{repository}:{tag}-load'


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--repository', required=True, help='ECR repository URI or local image name')
    parser.add_argument('--tag', required=True, help='Unique run/build tag')
    parser.add_argument('--docker-context', default='colima')
    parser.add_argument('--builder', help='Existing Buildx builder with native or emulated arm64 and amd64 support')
    parser.add_argument('--execute', action='store_true', help='Execute builds; otherwise print commands only')
    parser.add_argument('--push', action='store_true', help='Publish images; without this flag load them locally')
    parser.add_argument('--resume', action='store_true', help='Reuse completed entries from this build manifest; do not edit sources between attempts')
    args = parser.parse_args()
    destination = ROOT / 'artifacts' / args.tag
    # Build tags are also directory names; disallow traversal and shell-like input.
    if not args.tag or any(c not in 'abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_.-' for c in args.tag) or args.tag in ('.', '..'):
        parser.error('Invalid tag')
    manifest = destination / 'images.json'
    images = json.loads(manifest.read_text())['images'] if args.resume and manifest.exists() else {}
    for name, arch, dockerfile, image in builds(args.repository, args.tag):
        if name in images and images[name]['tag'] == image and images[name]['pushed'] == args.push:
            print('Reusing completed image ' + image, flush=True)
            continue
        metadata = destination / (name + '.json')
        cmd = ['docker', '--context', args.docker_context, 'buildx', 'build',
               '--platform', 'linux/' + arch, '--file', dockerfile, '--tag', image,
               '--metadata-file', str(metadata), '--provenance=false', '--push' if args.push else '--load']
        if args.builder:
            cmd += ['--builder', args.builder]
        cmd += ['.']
        print(shlex.join(cmd), flush=True)
        if args.execute:
            destination.mkdir(parents=True, exist_ok=True)
            subprocess.run(cmd, cwd=ROOT, check=True)
            digest = json.loads(metadata.read_text())['containerimage.digest']
            images[name] = {'tag': image, 'image': args.repository + '@' + digest,
                            'docker_architecture': arch, 'pushed': args.push}
            manifest.write_text(json.dumps({'built_at': datetime.datetime.now(datetime.timezone.utc).isoformat(), 'images': images}, indent=2) + '\n')
    if args.execute:
        (destination / 'images.json').write_text(json.dumps({'built_at': datetime.datetime.now(datetime.timezone.utc).isoformat(),
                                                          'images': images}, indent=2) + '\n')
        print(destination / 'images.json')


if __name__ == '__main__':
    main()

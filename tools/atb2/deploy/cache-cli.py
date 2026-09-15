"""Publish builder-produced CLIs to an immutable, version/revision-keyed cache.

Only bootstrap invokes this as root, with executable bytes supplied by the
unprivileged builder. Agents only receive a read-only mount of the cache.
"""
import hashlib
import json
import os
from pathlib import Path
import re
import sys
import tempfile

VERSION = re.compile(r'(?:baml(?:-cli)?\s+)?([0-9]+\.[0-9]+\.[0-9]+(?:-[A-Za-z0-9.-]+)?(?:\+[A-Za-z0-9.-]+)?)\Z')
REVISION = re.compile(r'[0-9a-f]{40}\Z')


def directory(path):
    if path.is_symlink():
        raise ValueError('cache directory is a symlink')
    path.mkdir(exist_ok=True)
    if not path.is_dir() or path.stat().st_uid != os.geteuid():
        raise ValueError('cache directory has unexpected owner')
    path.chmod(0o755)


def publish(root, version, revision, stream):
    match = VERSION.fullmatch(version.strip())
    if not match or not REVISION.fullmatch(revision):
        raise ValueError('invalid CLI version or source revision')
    version = match.group(1)
    directory(root)
    folder = root / version
    directory(folder)
    folder = folder / revision
    directory(folder)
    binary = folder / 'baml-cli'
    fd, temporary = tempfile.mkstemp(dir=folder, prefix='.publish-')
    try:
        digest = hashlib.sha256()
        size = 0
        with os.fdopen(fd, 'wb') as output:
            while chunk := stream.read(1024 * 1024):
                size += len(chunk)
                if size > 2 * 1024**3:
                    raise ValueError('CLI artifact too large')
                digest.update(chunk)
                output.write(chunk)
        if size == 0:
            raise ValueError('empty CLI artifact')
        if binary.is_symlink():
            raise ValueError('cached CLI is a symlink')
        if binary.exists():
            # Never replace an already-published version/revision pair.
            with binary.open('rb') as existing:
                old = hashlib.file_digest(existing, 'sha256').hexdigest()
            if old != digest.hexdigest():
                raise ValueError('CLI version/revision already contains a different artifact')
        else:
            os.chmod(temporary, 0o555)
            os.replace(temporary, binary)
        entries = []
        for path in sorted(root.glob('*/*/baml-cli')):
            if path.is_symlink() or not path.is_file():
                raise ValueError('unsafe cached artifact')
            entries.append({'version': path.parent.parent.name, 'revision': path.parent.name,
                            'path': str(path.relative_to(root))})
        index_fd, index_tmp = tempfile.mkstemp(dir=root, prefix='.index-')
        try:
            with os.fdopen(index_fd, 'w') as out:
                json.dump(entries, out)
            os.chmod(index_tmp, 0o444)
            os.replace(index_tmp, root / 'index.json')
        finally:
            Path(index_tmp).unlink(missing_ok=True)
        return binary
    finally:
        Path(temporary).unlink(missing_ok=True)


if __name__ == '__main__':
    if os.geteuid() != 0:
        raise SystemExit('only bootstrap may publish shared CLIs')
    publish(Path('/data/cli-cache'), sys.argv[1], sys.argv[2], sys.stdin.buffer)

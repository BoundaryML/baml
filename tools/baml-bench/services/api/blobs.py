"""On-volume blob store for transcripts and built baml binaries.

Convex documents have a ~1MB cap and transcripts/binaries are larger, so
the service persists them on its own volume and stores only a pointer
(the storage id) in Convex.
"""

from __future__ import annotations

import hashlib
import os
from pathlib import Path

BLOB_DIR = Path(os.environ.get("BLOB_DIR", "/data/blobs"))


def _path(storage_id: str) -> Path:
    """Resolve a storage id to an absolute path inside the blob directory.

    Args:
        storage_id: Relative blob storage id (e.g. ``tasks/abc.txt``).

    Returns:
        The resolved absolute path within BLOB_DIR.

    Raises:
        ValueError: When the resolved path escapes BLOB_DIR (path traversal).
    """
    base = BLOB_DIR.resolve()
    p = (BLOB_DIR / storage_id).resolve()
    if not p.is_relative_to(base):
        raise ValueError(f"path traversal in storage id: {storage_id}")
    return p


def put_text(kind: str, key: str, text: str) -> str:
    """Write a text blob to the volume and return its storage id.

    Args:
        kind: Blob namespace (e.g. table name) used as the directory.
        key: Unique key within the namespace (e.g. a document id).
        text: Text content to persist.

    Returns:
        The storage id (``{kind}/{key}.txt``) for later retrieval.
    """
    storage_id = f"{kind}/{key}.txt"
    p = _path(storage_id)
    p.parent.mkdir(parents=True, exist_ok=True)
    p.write_text(text)
    return storage_id


def get_text(storage_id: str) -> str:
    """Read and return a stored text blob.

    Args:
        storage_id: Storage id of the text blob.

    Returns:
        The decoded text content of the blob.
    """
    return _path(storage_id).read_text()


def put_binary(kind: str, key: str, data: bytes) -> tuple[str, str, int]:
    """Write a binary blob to the volume and return its identifying metadata.

    Args:
        kind: Blob namespace used as the directory (e.g. ``baml``).
        key: Unique key within the namespace (e.g. a release sha).
        data: Raw bytes to persist.

    Returns:
        A tuple of (storage_id, sha256_hex, size_bytes).
    """
    storage_id = f"{kind}/{key}"
    p = _path(storage_id)
    p.parent.mkdir(parents=True, exist_ok=True)
    p.write_bytes(data)
    digest = hashlib.sha256(data).hexdigest()
    return storage_id, digest, len(data)


def get_binary(storage_id: str) -> bytes:
    """Read and return a stored binary blob.

    Args:
        storage_id: Storage id of the binary blob.

    Returns:
        The raw bytes of the blob.
    """
    return _path(storage_id).read_bytes()


def exists(storage_id: str) -> bool:
    """Report whether a blob exists for the given storage id.

    Args:
        storage_id: Storage id to check.

    Returns:
        True if the blob exists and the id is valid; False otherwise
        (including invalid/path-traversing ids).
    """
    try:
        return _path(storage_id).exists()
    except ValueError:
        return False

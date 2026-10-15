#!/usr/bin/env python3
"""Read-only reproduction of a published artifact row in the #5011 baseline audit.

Downloads bytes and reads archives in memory. Never extracts to disk or executes
package code. No third-party Python packages are required.
"""

import argparse
import gzip
import hashlib
import io
import json
import re
import tarfile
import urllib.request
import zipfile
from pathlib import Path


def download(url):
    request = urllib.request.Request(url, headers={"User-Agent": "BoundaryML-license-audit/1.0"})
    with urllib.request.urlopen(request, timeout=120) as response:
        data = response.read()
        if response.headers.get("Content-Encoding") == "gzip":
            data = gzip.decompress(data)
        return data


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("record", help="Baseline table row ID, e.g. R001")
    args = parser.parse_args()
    baseline = Path(__file__).resolve().parents[1] / "docs/5011-sdk-license-audit.json"
    evidence = json.loads(baseline.read_text())
    record = next((row for row in evidence["rows"] if row["id"] == args.record), None)
    if record is None:
        parser.error(f"Unknown record {args.record}")
    if record.get("metadata_url"):
        print("REGISTRY", record["metadata_url"])
        try:
            print(download(record["metadata_url"]).decode("utf-8"))
        except Exception as error:
            print("Registry metadata unavailable:", error)
    data = download(record["url"])
    digest = hashlib.sha256(data).hexdigest()
    print("ARTIFACT", record["url"], "SHA256", digest)
    if digest != record["sha256"]:
        raise SystemExit("Artifact differs from the audited bytes")
    interesting = re.compile(r"(^|/)([^/]*(?:licen[cs]e|notice)[^/]*|package.json|METADATA|PKG-INFO|Cargo.toml|go.mod|Package.swift|[^/]*\.nuspec|[^/]*\.pom|MANIFEST.MF)$|(^|/)legal/", re.I)
    if zipfile.is_zipfile(io.BytesIO(data)):
        with zipfile.ZipFile(io.BytesIO(data)) as archive:
            for entry in archive.infolist():
                print("MEMBER", entry.filename)
                if not entry.is_dir() and interesting.search(entry.filename):
                    print(archive.read(entry).decode("utf-8", "replace"))
    elif data[:2] == b"\x1f\x8b":
        with tarfile.open(fileobj=io.BytesIO(data), mode="r:gz") as archive:
            for entry in archive:
                print("MEMBER", entry.name)
                if entry.isfile() and interesting.search(entry.name):
                    print(archive.extractfile(entry).read().decode("utf-8", "replace"))
    elif record["url"].endswith((".pom", ".module", ".rb")):
        print(data.decode("utf-8", "replace"))
    else:
        print("Raw binary: no archive-level manifest or bundled license file")


if __name__ == "__main__":
    main()

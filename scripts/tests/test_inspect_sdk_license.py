"""Keep the reproduction tool's digest contract equal to raw curl downloads."""

import contextlib
import gzip
import hashlib
import importlib.machinery
import importlib.util
import io
import json
import unittest
from pathlib import Path
from unittest.mock import patch

SCRIPT = Path(__file__).resolve().parents[2] / "tools/inspect-sdk-license.py"
LOADER = importlib.machinery.SourceFileLoader("inspect_sdk_license", str(SCRIPT))
SPEC = importlib.util.spec_from_loader(LOADER.name, LOADER)
INSPECTOR = importlib.util.module_from_spec(SPEC)
LOADER.exec_module(INSPECTOR)


class EncodedAuditDownloads(unittest.TestCase):
    def response(self, raw):
        response = io.BytesIO(raw)
        response.headers = {"Content-Encoding": "gzip"}
        return response

    def test_download_retains_raw_bytes_and_decodes_registry_metadata(self):
        payload = b'{"license":"Apache-2.0"}'
        raw = gzip.compress(payload)
        with patch.object(
            INSPECTOR.urllib.request, "urlopen", return_value=self.response(raw)
        ):
            self.assertEqual(
                INSPECTOR.download("https://example.invalid/metadata"), payload
            )
        with patch.object(
            INSPECTOR.urllib.request, "urlopen", return_value=self.response(raw)
        ):
            self.assertEqual(
                INSPECTOR.download(
                    "https://example.invalid/metadata", include_raw=True
                ),
                (raw, payload),
            )

    def test_main_hashes_encoded_response_and_inspects_decoded_payload(self):
        payload = b"<project><licenses>Apache-2.0</licenses></project>"
        raw = gzip.compress(payload)
        digest = hashlib.sha256(raw).hexdigest()
        evidence = {
            "rows": [
                {
                    "id": "R000",
                    "url": "https://example.invalid/artifact.pom",
                    "sha256": digest,
                }
            ]
        }
        output = io.StringIO()
        with (
            patch("sys.argv", [str(SCRIPT), "R000"]),
            patch.object(Path, "read_text", return_value=json.dumps(evidence)),
            patch.object(
                INSPECTOR.urllib.request, "urlopen", return_value=self.response(raw)
            ),
            contextlib.redirect_stdout(output),
        ):
            INSPECTOR.main()
        self.assertIn(digest, output.getvalue())
        self.assertIn(payload.decode(), output.getvalue())


if __name__ == "__main__":
    unittest.main()

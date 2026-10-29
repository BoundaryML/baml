"""Exercise the actual loopback server's release-file boundary."""

import http.client
import http.server
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch
from urllib.parse import urlsplit

from fixture import FixtureHTTPServer


class FixtureServerTests(unittest.TestCase):
    def test_loopback_startup_does_not_require_dns(self):
        with patch("socket.getfqdn", side_effect=AssertionError("unexpected DNS lookup")):
            with FixtureHTTPServer(("127.0.0.1", 0), http.server.BaseHTTPRequestHandler) as server:
                self.assertEqual(server.server_name, "localhost")
                self.assertGreater(server.server_port, 0)

    def test_get_stays_within_release_directory(self):
        helper = Path(__file__).with_name("fixture.py")
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "private").write_text("private sentinel")
            (root / "server").mkdir()
            for version in ["0.11.0", "0.12.0", "0.13.0"]:
                for extension in [".zip", ".tar.gz"]:
                    (root / "server" / (version + extension)).write_bytes(b"archive fixture")
            process = subprocess.Popen(
                [sys.executable, str(helper), "serve", directory],
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
            )
            try:
                address = urlsplit(process.stdout.readline().strip())
                connection = http.client.HTTPConnection(address.hostname, address.port, timeout=5)
                for path in ["/../private", "/%2e%2e/private", "/%2e%2e%5cprivate"]:
                    with self.subTest(path=path):
                        connection.request("GET", path)
                        response = connection.getresponse()
                        self.assertEqual(response.status, 404)
                        self.assertNotIn(b"private sentinel", response.read())
                connection.request("GET", "/version/0.11.0.json")
                response = connection.getresponse()
                self.assertEqual(response.status, 200)
                self.assertIn(b'"version": "0.11.0"', response.read())
                edited_text = '[package]\r\nname = "edited"\r\n'
                (root / "server" / "0.11.0.zip.edit").write_text(json.dumps({
                    "path": "project/baml.toml",
                    "text": edited_text,
                }))
                connection.request("GET", "/0.11.0.zip")
                response = connection.getresponse()
                self.assertEqual(response.status, 200)
                self.assertEqual(response.read(), b"archive fixture")
                self.assertEqual((root / "project" / "baml.toml").read_bytes(), edited_text.encode("utf-8"))
                connection.close()
            finally:
                process.terminate()
                process.communicate(timeout=5)


if __name__ == "__main__":
    unittest.main()

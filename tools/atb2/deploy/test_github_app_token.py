"""github-app-token.py: JWT shape, cache reuse, and no secret on stdout when unconfigured."""
import base64
import importlib.util
import json
import os
import subprocess
import tempfile
import time
import unittest
from pathlib import Path

spec = importlib.util.spec_from_file_location("bammy_token", Path(__file__).with_name("github-app-token.py"))
tok = importlib.util.module_from_spec(spec)
spec.loader.exec_module(tok)


def b64json(segment):
    return json.loads(base64.urlsafe_b64decode(segment + "=" * (-len(segment) % 4)))


class GithubAppToken(unittest.TestCase):
    def test_jwt_is_rs256_issued_by_the_client_id_and_short_lived(self):
        key = subprocess.run(["openssl", "genrsa", "2048"], capture_output=True, text=True, check=True).stdout
        jwt = tok.sign_jwt("Iv1.abc", key)
        header, payload, signature = jwt.split(".")
        self.assertEqual(b64json(header), {"alg": "RS256", "typ": "JWT"})
        claims = b64json(payload)
        self.assertEqual(claims["iss"], "Iv1.abc")
        self.assertLessEqual(claims["exp"] - claims["iat"], 600)
        self.assertTrue(len(signature) > 300)
        self.assertNotIn(key, jwt)

    def test_cache_is_reused_only_while_fresh(self):
        with tempfile.TemporaryDirectory() as d:
            path = os.path.join(d, "t.json")
            tok.store(path, "ghs_fresh", time.time() + 3600)
            self.assertEqual(tok.cached(path), "ghs_fresh")
            self.assertEqual(oct(os.stat(path).st_mode & 0o777), "0o600")
            tok.store(path, "ghs_stale", time.time() + 120)
            self.assertIsNone(tok.cached(path))
            self.assertIsNone(tok.cached(os.path.join(d, "missing.json")))

    def test_unconfigured_run_fails_closed_without_output(self):
        env = {k: v for k, v in os.environ.items() if not k.startswith("BAMMY_")}
        env["PATH"] = "/usr/bin:/bin"
        run = subprocess.run(["python3", "-I", str(Path(__file__).with_name("github-app-token.py"))], env=env, capture_output=True, text=True)
        self.assertEqual(run.returncode, 1)
        self.assertEqual(run.stdout, "")
        self.assertIn("could not mint", run.stderr)


if __name__ == "__main__":
    unittest.main()

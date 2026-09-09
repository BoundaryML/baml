"""Offline HTTP smoke: real BAML listener, no agents or external credentials."""
import hashlib
import hmac
import json
import os
from pathlib import Path
import socket
import subprocess
import time
import unittest
import urllib.error
import urllib.request


class SlackHTTPTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cli = os.environ.get("BAML_CLI", str(Path.home() / ".atb2/target/debug/baml-cli"))
        if not Path(cli).is_file():
            raise unittest.SkipTest("set BAML_CLI to canary's baml-cli")
        with socket.socket() as listener:
            listener.bind(("127.0.0.1", 0))
            port = listener.getsockname()[1]
        cls.base = f"http://127.0.0.1:{port}"
        cls.secret = "offline-test-signing-key"
        # Never forward the invoking shell's service keys, Slack token or login.
        env = {key: os.environ[key] for key in ("PATH", "HOME") if key in os.environ}
        env.update(ATB2_UI_RUNNER_SECRET="offline-website-signing-fixture-key-00000", ATB_SLACK_SIGNING_SECRET=cls.secret, BAML_TELEMETRY_DISABLED="1", BAML_AGENT_SKILL_CHECK="off")
        cls.proc = subprocess.Popen([cli, "run", "-e", f"serve_slack(port = {port})"],
            cwd=Path(__file__).resolve().parents[1], env=env,
            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        for _ in range(300):
            if cls.proc.poll() is not None:
                break
            try:
                with urllib.request.urlopen(cls.base + "/health", timeout=0.2) as res:
                    if res.status == 200:
                        return
            except (OSError, urllib.error.URLError):
                time.sleep(0.1)
        cls.proc.terminate()
        cls.proc.wait(timeout=10)
        raise AssertionError("BAML listener did not become healthy")

    @classmethod
    def tearDownClass(cls):
        cls.proc.terminate()
        cls.proc.wait(timeout=10)

    def post(self, payload, timestamp=None, signed=True):
        raw = json.dumps(payload).encode()
        ts = str(int(time.time()) if timestamp is None else timestamp)
        headers = {"Content-Type": "application/json", "X-Slack-Request-Timestamp": ts}
        if signed:
            digest = hmac.new(self.secret.encode(), b"v0:" + ts.encode() + b":" + raw, hashlib.sha256).hexdigest()
            headers["X-Slack-Signature"] = "v0=" + digest
        req = urllib.request.Request(self.base + "/slack/events", data=raw, headers=headers)
        try:
            res = urllib.request.urlopen(req, timeout=3)
        except urllib.error.HTTPError as err:
            res = err
        with res:
            return res.status, json.load(res)

    def test_signed_challenge(self):
        challenge = 'quoted"challenge\\newline\n'
        self.assertEqual(self.post({"type": "url_verification", "challenge": challenge}),
                         (200, {"challenge": challenge}))

    def test_unsigned_and_stale_requests_are_rejected(self):
        self.assertEqual(self.post({}, signed=False)[0], 403)
        self.assertEqual(self.post({}, timestamp=int(time.time()) - 301)[0], 403)

    def test_website_hmac_covers_unicode_body_and_rejects_tampering(self):
        raw=json.dumps({'operation':'unknown','author':'fixture','body':'Unicode ✓','id':'bad'},ensure_ascii=False).encode()
        ts=str(int(time.time()))
        signature='v0='+hmac.new(b'offline-website-signing-fixture-key-00000',b'v0:'+ts.encode()+b':'+raw,hashlib.sha256).hexdigest()
        for payload,expected in [(raw,400),(raw+b' ',403)]:
            req=urllib.request.Request(self.base+'/ui',data=payload,headers={'X-ATB2-Timestamp':ts,'X-ATB2-Signature':signature})
            with self.assertRaises(urllib.error.HTTPError) as error:urllib.request.urlopen(req,timeout=5)
            self.assertEqual(error.exception.code,expected)
            error.exception.close()

    def test_store_failure_is_not_acknowledged(self):
        payload = {"type": "event_callback", "event_id": "Ev1", "team_id": "T1", "event": {
            "type": "app_mention", "user": "U1", "channel": "C1", "ts": "123.4", "text": "<@B1> broken"}}
        self.assertEqual(self.post(payload)[0], 503)


if __name__ == "__main__":
    unittest.main()

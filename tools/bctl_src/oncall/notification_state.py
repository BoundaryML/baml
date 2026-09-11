"""GitHub Contents API journal, separate from the roster and source branch."""

from __future__ import annotations

import base64
import datetime
import json
import subprocess
from urllib.parse import quote

BRANCH = "oncall/notification-state"


class GitHubState:
    def __init__(self, repository: str, friday: datetime.date, *, sandbox_run_id: str | None = None):
        self.root = f"repos/{repository}"
        if sandbox_run_id is not None and not sandbox_run_id.isdecimal():
            raise RuntimeError("sandbox run ID must be a GitHub Actions numeric run ID")
        key = f"sandbox/{sandbox_run_id}" if sandbox_run_id is not None else friday.isoformat()
        self.path = f"{self.root}/contents/notifications/{key}.json"
        self.sha = None

    def _api(self, path, *, method="GET", body=None, missing_ok=False):
        args = ["gh", "api", path, "--method", method]
        if body is not None:
            args += ["--input", "-"]
        result = subprocess.run(
            args, input=json.dumps(body) if body is not None else None,
            text=True, capture_output=True, check=False,
        )
        if result.returncode:
            if missing_ok and "(HTTP 404)" in result.stderr:
                return None
            raise RuntimeError(f"notification state API failed: {result.stderr.strip()}")
        return json.loads(result.stdout)

    def load(self):
        # Only bootstrap on an explicit 404; auth/network errors must not reset state.
        ref = self._api(f"{self.root}/git/ref/heads/{BRANCH}", missing_ok=True)
        if ref is None:
            repository = self._api(self.root)
            base = self._api(f"{self.root}/git/ref/heads/{repository['default_branch']}")
            self._api(
                f"{self.root}/git/refs", method="POST",
                body={"ref": f"refs/heads/{BRANCH}", "sha": base["object"]["sha"]},
            )
        result = self._api(f"{self.path}?ref={quote(BRANCH, safe='')}", missing_ok=True)
        if result is None:
            return None
        self.sha = result["sha"]
        return json.loads(base64.b64decode(result["content"]))

    def save(self, state):
        body = {
            "branch": BRANCH,
            "message": f"oncall: checkpoint {state['friday']} notifications",
            "content": base64.b64encode((json.dumps(state, indent=2) + "\n").encode()).decode(),
        }
        if self.sha is not None:
            body["sha"] = self.sha
        result = self._api(self.path, method="PUT", body=body)
        self.sha = result["content"]["sha"]

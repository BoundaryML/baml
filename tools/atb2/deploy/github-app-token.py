"""Mint a GitHub App installation token for the bammy bot, cached until it nears expiry.

Reads BAMMY_GITHUB_APP_CLIENT_ID and BAMMY_GITHUB_APP_PRIVATE_KEY (a PEM) from the
environment, signs an RS256 JWT with the system openssl (no third-party Python
packages in the runner image), looks the installation up, and prints the
installation access token on stdout. Every push, `gh` call and PR the runner makes
with it appears as the App's bot user, never as a person.

Cache: $ATB2_TOKEN_CACHE (default $HOME/.bammy-github-token.json), mode 0600, reused
while more than five minutes remain. Exit 1 with a fixed message on any failure;
nothing secret is ever printed.
"""
import base64
import json
import os
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.request

API = os.environ.get("GITHUB_API_URL", "https://api.github.com").rstrip("/")
REPO = os.environ.get("ATB2_REPO", "BoundaryML/baml")


def b64url(data):
    return base64.urlsafe_b64encode(data).rstrip(b"=").decode()


def sign_jwt(client_id, pem):
    now = int(time.time())
    header = b64url(json.dumps({"alg": "RS256", "typ": "JWT"}, separators=(",", ":")).encode())
    payload = b64url(json.dumps({"iat": now - 60, "exp": now + 540, "iss": client_id}, separators=(",", ":")).encode())
    signing_input = f"{header}.{payload}".encode()
    fd, path = tempfile.mkstemp(prefix="bammy-", suffix=".pem")
    try:
        with os.fdopen(fd, "w") as f:
            f.write(pem if pem.endswith("\n") else pem + "\n")
        out = subprocess.run(
            ["openssl", "dgst", "-sha256", "-sign", path],
            input=signing_input, capture_output=True, timeout=30,
        )
    finally:
        os.unlink(path)
    if out.returncode != 0 or not out.stdout:
        raise RuntimeError("jwt signing failed")
    return f"{header}.{payload}.{b64url(out.stdout)}"


def api(method, path, token, body=None):
    req = urllib.request.Request(
        API + path, method=method, data=json.dumps(body).encode() if body is not None else None,
        headers={"Authorization": "Bearer " + token, "Accept": "application/vnd.github+json",
                 "X-GitHub-Api-Version": "2022-11-28", "User-Agent": "atb2-bammy"},
    )
    with urllib.request.urlopen(req, timeout=30) as r:
        return json.load(r)


def installation_id(jwt):
    owner, name = REPO.split("/", 1)
    try:
        return api("GET", f"/repos/{owner}/{name}/installation", jwt)["id"]
    except urllib.error.HTTPError as e:
        if e.code != 404:
            raise
    # not installed on the repo directly: take the first installation the App has
    installs = api("GET", "/app/installations", jwt)
    if not installs:
        raise RuntimeError("the App is not installed anywhere")
    return installs[0]["id"]


def cached(path):
    try:
        with open(path) as f:
            data = json.load(f)
        if isinstance(data.get("token"), str) and data.get("expires_at", 0) - time.time() > 300:
            return data["token"]
    except (OSError, ValueError):
        pass
    return None


def store(path, token, expires_at):
    tmp = path + ".tmp"
    fd = os.open(tmp, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)
    with os.fdopen(fd, "w") as f:
        json.dump({"token": token, "expires_at": expires_at}, f)
    os.replace(tmp, path)


def mint():
    client_id = os.environ.get("BAMMY_GITHUB_APP_CLIENT_ID", "").strip()
    pem = os.environ.get("BAMMY_GITHUB_APP_PRIVATE_KEY", "")
    if not client_id or "PRIVATE KEY" not in pem:
        raise RuntimeError("bammy GitHub App credentials are not configured")
    cache = os.environ.get("ATB2_TOKEN_CACHE") or os.path.join(os.environ.get("HOME", "/tmp"), ".bammy-github-token.json")
    token = cached(cache)
    if token:
        return token
    jwt = sign_jwt(client_id, pem)
    data = api("POST", f"/app/installations/{installation_id(jwt)}/access_tokens", jwt, {})
    token = data["token"]
    # GitHub says one hour; keep our own clock in case the reply lacks expires_at
    store(cache, token, time.time() + 3600 - 60)
    return token


def main():
    try:
        print(mint())
    except Exception:  # noqa: BLE001 - the reason may carry response bodies; keep it out of logs
        print("atb2: could not mint a bammy GitHub App token", file=sys.stderr)
        sys.exit(1)


if __name__ == "__main__":
    main()

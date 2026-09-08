"""Fetch secrets as root, then exec setpriv with a fresh runtime environment."""

import json
import os
import resource
from urllib.parse import urlsplit
import stat
import subprocess
import sys


RUNTIME_KEYS = frozenset("""
ATB2_DATASET ATB2_GIT_EMAIL ATB2_GIT_USER ATB2_ISSUES
ATB2_KEEP_RUNS ATB2_MAX_WAIT_S ATB2_MODEL ATB2_POLL_S ATB2_REPO ATB2_REPO_URL
ATB2_REVIEWERS ATB2_SHEPHERDS ATB2_UI_URL ATB2_SLACK_BOT_TOKEN ATB2_SLACK_CHANNEL
ATB2_SLACK_INTAKE_CHANNEL ATB_SLACK_BOT_TOKEN ATB_SLACK_FIX_CHANNEL
ATB2_SLACK_SIGNING_SECRET ATB_SLACK_SIGNING_SECRET
ATB2_POSTHOG_API_KEY ATB2_POSTHOG_PROJECT_ID ATB2_POSTHOG_HOST
ATB_POSTHOG_API_KEY ATB_POSTHOG_PROJECT_ID ATB_POSTHOG_HOST
ATB_GITHUB_TOKEN GH_TOKEN GITHUB_TOKEN FEEDBACK_SUPABASE_KEY FEEDBACK_SUPABASE_URL
BAML_VERSION HOSTNAME
""".split())
FIXED_ENV = {
    "PATH": "/usr/local/cargo/bin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin",
    "HOME": "/data/home", "USER": "atb2", "LOGNAME": "atb2",
    "ATB2_HOME": "/data", "CARGO_HOME": "/data/cargo", "RUSTUP_HOME": "/data/rustup",
    "LANG": "C.UTF-8", "TERM": "dumb", "GIT_TERMINAL_PROMPT": "0",
}


def runtime_environment(source):
    env = {key: source[key] for key in RUNTIME_KEYS if key in source}
    auth = source.get("ATB2_INFISICAL_AUTH", "machine")
    if auth not in ("machine", "user"):
        raise ValueError("invalid Infisical authentication mode")
    if auth == "user":
        # Only explicitly selected demo logins may replace machine authentication.
        # Reject symlinks and writable parents before trusting persisted CLI state.
        for path in ("/data", "/data/infisical-home"):
            info = os.lstat(path)
            forbidden = 0o077 if path.endswith("infisical-home") else 0o022
            if not stat.S_ISDIR(info.st_mode) or info.st_uid != 0 or info.st_mode & forbidden:
                raise ValueError("Infisical login directory must be private to root")
    client_id = source.get("INFISICAL_CLIENT_ID")
    client_secret = source.get("INFISICAL_CLIENT_SECRET")
    if auth == "user" or client_id or client_secret or source.get("INFISICAL_TOKEN"):
        project = source.get("INFISICAL_PROJECT_ID")
        if not project:
            raise ValueError("INFISICAL_PROJECT_ID is required")
        # The exporter runs as root and returns JSON in memory. Never source
        # exported shell code or place credentials in command arguments/files.
        export_env = {
            "PATH": FIXED_ENV["PATH"], "HOME": "/root", "LANG": "C.UTF-8",
            "INFISICAL_DISABLE_UPDATE_CHECK": "true",
        }
        if auth == "user":
            export_env["HOME"] = "/data/infisical-home"
        for key in ("INFISICAL_API_URL", "INFISICAL_DOMAIN"):
            if key in source:
                url = urlsplit(source[key])
                if url.scheme != 'https' or not url.hostname or url.username or url.password or url.fragment:
                    raise ValueError('Infisical endpoint must use HTTPS without embedded credentials')
                export_env[key] = source[key]
        if auth == "user":
            pass  # The root exporter uses its saved CLI session, never inherited tokens.
        elif client_id or client_secret:
            if not client_id or not client_secret:
                raise ValueError("both Infisical Universal Auth credentials are required")
            login_env = {
                **export_env,
                "INFISICAL_UNIVERSAL_AUTH_CLIENT_ID": client_id,
                "INFISICAL_UNIVERSAL_AUTH_CLIENT_SECRET": client_secret,
            }
            login = subprocess.run(
                ["/usr/local/bin/infisical", "login", "--method=universal-auth",
                 "--plain", "--silent", "--telemetry=false"],
                env=login_env, cwd="/", stdin=subprocess.DEVNULL,
                capture_output=True, text=True, timeout=120,
            )
            token = login.stdout.strip()
            if login.returncode or not token or any(c.isspace() or c == "\0" for c in token):
                raise ValueError("Infisical login failed")
            # Only the exporter receives the fresh access token. It never
            # receives the client secret or the host's application credentials.
            export_env["INFISICAL_TOKEN"] = token
        else:
            export_env["INFISICAL_TOKEN"] = source["INFISICAL_TOKEN"]
        result = subprocess.run(
            ["/usr/local/bin/infisical", "export", "--format=json", "--silent",
             "--telemetry=false", "--expand=false", "--projectId=" + project,
             "--env=" + source.get("INFISICAL_ENV", "prod")],
            env=export_env, cwd="/", stdin=subprocess.DEVNULL,
            capture_output=True, text=True, timeout=120,
        )
        if result.returncode:
            # Exporter diagnostics may contain secrets; do not relay them.
            raise ValueError("Infisical export failed")
        rows = json.loads(result.stdout)
        if not isinstance(rows, list):
            raise ValueError("invalid Infisical export")
        for row in rows:
            if not isinstance(row, dict) or not isinstance(row.get("key"), str):
                raise ValueError("invalid Infisical export row")
            key = row["key"]
            if key in RUNTIME_KEYS:
                value = row.get("value")
                if not isinstance(value, str) or "\0" in value:
                    raise ValueError("invalid runtime secret value")
                env[key] = value
    env.update(FIXED_ENV)
    return env


def main():
    if os.geteuid() != 0:
        raise ValueError("runtime launcher requires root")
    resource.setrlimit(resource.RLIMIT_CORE, (0, 0))
    env = runtime_environment(os.environ)
    # Replace the token-bearing process image/environment BEFORE lowering UID.
    # Secrets travel in envp, never in the public command line of env/setpriv.
    os.execve(
        "/usr/bin/setpriv",
        ["setpriv", "--reuid=1000", "--regid=1000", "--clear-groups",
         "--no-new-privs", "--bounding-set=-all",
         "/usr/local/bin/atb2-entrypoint", *sys.argv[1:]],
        env,
    )


if __name__ == "__main__":
    try:
        main()
    except (ValueError, OSError, subprocess.SubprocessError):
        print("atb2: runtime secret loading/launch failed; runtime was not started", file=sys.stderr)
        sys.exit(1)

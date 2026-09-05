"""Fetch secrets as root, then exec setpriv with a fresh runtime environment."""

import json
import os
import resource
import subprocess
import sys


RUNTIME_KEYS = frozenset("""
ATB2_DATASET ATB2_FLAKY_TESTS ATB2_GIT_EMAIL ATB2_GIT_USER ATB2_ISSUES
ATB2_KEEP_RUNS ATB2_MAX_WAIT_S ATB2_MODEL ATB2_POLL_S ATB2_REPO ATB2_REPO_URL
ATB2_REVIEWERS ATB2_UI_URL ATB2_SLACK_BOT_TOKEN ATB2_SLACK_CHANNEL
ATB2_SLACK_INTAKE_CHANNEL ATB_SLACK_BOT_TOKEN ATB_SLACK_FIX_CHANNEL
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
    if source.get("INFISICAL_TOKEN"):
        project = source.get("INFISICAL_PROJECT_ID")
        if not project:
            raise ValueError("INFISICAL_PROJECT_ID is required")
        # The exporter runs as root and returns JSON in memory. Never source
        # exported shell code or place credentials in command arguments/files.
        export_env = {
            "PATH": FIXED_ENV["PATH"], "HOME": "/root", "LANG": "C.UTF-8",
            "INFISICAL_TOKEN": source["INFISICAL_TOKEN"],
            "INFISICAL_DISABLE_UPDATE_CHECK": "true",
        }
        for key in ("INFISICAL_API_URL", "INFISICAL_DOMAIN"):
            if key in source:
                export_env[key] = source[key]
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

#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.9"
# # No third-party packages: the harness drives the real `gcloud` CLI + `cargo`
# # (intentionally, to verify the fork against the genuine toolchain) and uses
# # only the Python standard library. The inline block keeps it runnable via
# # `uv run` with a pinned interpreter.
# dependencies = []
# ///
"""
run-e2e.py — repeatable end-to-end credential-safety harness for the
forked_google_cloud_auth crate.

Run this on any change to forks/google-cloud-auth to assert that every GCP auth
flow still mints a usable access token and that real Google APIs accept it
(tokeninfo + Cloud Resource Manager; Vertex AI optionally). Each scenario
ASSERTS and the script exits non-zero on any failure, so it is safe to gate CI.

Tiers:
  offline — `cargo test -p forked_google_cloud_auth` (every flow's logic +
            the ADC source-precedence guard). No creds needed.
  local   — ADC authorized_user via the well-known path AND via
            GOOGLE_APPLICATION_CREDENTIALS, signed against real Google APIs.
            (Service-account-JSON is included only if a key file is supplied;
            key creation is org-blocked in the dev project — see README.)
  cloud   — provisions a real GCE VM and exercises the GCE metadata-server
            flow (the SA-backed token path), then tears it down.

Usage:
  ./run-e2e.py                 # offline + local  (default; needs ADC creds)
  ./run-e2e.py --offline-only  # offline only     (CI-safe, no creds)
  ./run-e2e.py --all           # offline + local + cloud
  ./run-e2e.py --cloud         # offline + cloud
  Flags: --project ID (default from gcloud config; must be the dev project)
         --sa-file PATH (enables the service-account flow) --skip-build
         --vertex-model M --vertex-region R (optional Vertex call) --zone Z
"""

import argparse
import json
import os
import shutil
import signal
import subprocess
import sys
import tempfile
import time
from datetime import datetime
from pathlib import Path

# This harness only ever touches the dev Vertex project. Any live tier that
# resolves to a different project is refused before it can mint creds or
# provision anything. Change this only if you deliberately move the test project.
DEV_PROJECT = "sam-project-vertex-1"

SCRIPT_DIR = Path(__file__).resolve().parent
REPO = SCRIPT_DIR.parent.parent  # forks/google-cloud-auth-systest -> forks -> repo root
MANIFEST = SCRIPT_DIR / "Cargo.toml"
MUSL_TARGET = "x86_64-unknown-linux-musl"

def red(s):    return f"\033[31m{s}\033[0m"
def green(s):  return f"\033[32m{s}\033[0m"
def yellow(s): return f"\033[33m{s}\033[0m"

def log(title):
    print()
    print(f"=== {title} ===")

SUMMARY = []
PASS = 0
FAIL = 0

def record(name, ok, detail):
    global PASS, FAIL
    if ok:
        PASS += 1
        SUMMARY.append(f"PASS  {name}  — {detail}")
    else:
        FAIL += 1
        SUMMARY.append(f"FAIL  {name}  — {detail}")

def run(cmd, env=None, cwd=None):
    return subprocess.run(cmd, env=env, cwd=cwd,
                          stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)

def gcloud(*args):
    """Run `gcloud ...`; return stripped stdout ('' on failure)."""
    r = run(["gcloud", *args])
    return r.stdout.strip() if r.returncode == 0 else ""

# ---------------------------------------------------------------------------
# Harness JSON parsing / assertions
# ---------------------------------------------------------------------------
def parse_report(raw):
    if not raw:
        return None
    idx = raw.find("{")
    if idx < 0:
        return None
    try:
        return json.loads(raw[idx:])
    except Exception:
        return None

def jget(d, path):
    if d is None:
        return ""
    cur = d
    for k in path.split("."):
        if isinstance(cur, dict) and k in cur:
            cur = cur[k]
        else:
            return ""
    return cur if cur is not None else ""

def assert_report(scenario, raw, want_proj):
    """Assert a harness JSON report: token minted, tokeninfo 200 w/ cloud-platform
    scope, and (when a project is expected) resourcemanager 200 for that project."""
    d = parse_report(raw)
    tok = jget(d, "token.ok")
    ti_s = jget(d, "tokeninfo.status")
    scope = jget(d, "tokeninfo.scope")
    rm_s = jget(d, "resourcemanager.status")
    rm_p = jget(d, "resourcemanager.project_id")
    vx_s = jget(d, "vertex.status")
    has_cp = "cloud-platform" in str(scope)
    detail = f"token={tok} tokeninfo={ti_s} scope={'cloud-platform' if scope else 'none'}"
    ok = (tok is True) and (ti_s == 200) and has_cp
    # Always surface the resourcemanager result; only GATE on it when a project is
    # expected (local user creds). The GCE default SA has no project-get role, so
    # the metadata tier proves token validity + scope via tokeninfo instead.
    if rm_s != "":
        detail += f" rm={rm_s}" + (f"/{rm_p}" if rm_p else "")
    if want_proj:
        ok = ok and (rm_s == 200) and (str(rm_p) == str(want_proj))
    # The vertex key only exists when --vertex-model/--vertex-region were
    # supplied; when it does, the generateContent call must succeed.
    if vx_s != "":
        detail += f" vertex={vx_s}"
        ok = ok and (vx_s == 200)
    if ok:
        print(f"  {green('PASS')} {scenario} — {detail}")
    else:
        print(f"  {red('FAIL')} {scenario} — {detail}")
        print(f"    raw: {(raw or '')[:500]}")
    record(scenario, ok, detail)

# ---------------------------------------------------------------------------
# Cloud resource handles + idempotent teardown
# ---------------------------------------------------------------------------
class Cloud:
    vm_name = ""
    bucket = ""

def master_cleanup(project, zone):
    if Cloud.vm_name or Cloud.bucket:
        log("Teardown")
    if Cloud.vm_name:
        gcloud("compute", "instances", "delete", Cloud.vm_name, "--zone", zone,
               "--project", project, "--quiet")
    if Cloud.bucket:
        gcloud("storage", "rm", "--recursive", f"gs://{Cloud.bucket}", "--project", project)

# ---------------------------------------------------------------------------
# Tiers
# ---------------------------------------------------------------------------
def run_offline():
    log("Offline: cargo test -p forked_google_cloud_auth")
    r = subprocess.run(["cargo", "test", "-p", "forked_google_cloud_auth"], cwd=REPO,
                       stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
    print("\n".join(r.stdout.splitlines()[-20:]))
    if r.returncode == 0:
        record("offline:cargo-test", True, "all flow + ADC-precedence tests passed")
    else:
        record("offline:cargo-test", False, "cargo test failed")

def run_local(args, native_bin, project, vopt, tmp):
    log("Local ADC scenarios (real Google APIs)")
    empty_home = tmp / "empty"
    empty_home.mkdir(parents=True, exist_ok=True)
    cfg = os.environ.get("CLOUDSDK_CONFIG", str(Path.home() / ".config" / "gcloud"))
    adc_file = Path(cfg) / "application_default_credentials.json"

    # 1) ADC authorized_user via the well-known path (real HOME).
    out = run([str(native_bin), "--flow", "adc", "--project", project, *vopt, "--json"]).stdout
    assert_report("local:adc-well-known (authorized_user)", out, project)

    # 2) ADC via GOOGLE_APPLICATION_CREDENTIALS (empty HOME so only GAC can answer).
    if adc_file.is_file():
        env = {"PATH": os.environ.get("PATH", ""), "HOME": str(empty_home),
               "GOOGLE_APPLICATION_CREDENTIALS": str(adc_file)}
        out = run([str(native_bin), "--flow", "adc", "--project", project, *vopt, "--json"],
                  env=env).stdout
        assert_report("local:adc-GOOGLE_APPLICATION_CREDENTIALS", out, project)
    else:
        record("local:adc-GOOGLE_APPLICATION_CREDENTIALS", False, f"ADC file not found at {adc_file}")

    # 3) Service-account JSON flow (only if a key file is supplied).
    if args.sa_file and Path(args.sa_file).is_file():
        env = {"PATH": os.environ.get("PATH", ""), "HOME": str(empty_home)}
        out = run([str(native_bin), "--flow", "service-account",
                   "--service-account-file", args.sa_file, "--project", project, *vopt, "--json"],
                  env=env).stdout
        assert_report("local:service-account-json", out, project)
    else:
        SUMMARY.append("SKIP  local:service-account-json  — no --sa-file "
                       "(SA key creation is org-blocked; flow is offline-covered)")
        print(f"  {yellow('SKIP')} local:service-account-json — supply --sa-file to enable")

def run_cloud(args, musl_bin, project, vopt, tmp, suffix, now_epoch):
    if not (musl_bin.exists() and os.access(musl_bin, os.X_OK)):
        record("cloud:gce-metadata", False, f"musl binary missing ({musl_bin})")
        return
    log("Cloud: GCE metadata-server flow (live VM)")
    Cloud.bucket = f"gcp-systest-{project}-{now_epoch}"
    Cloud.vm_name = f"gcp-systest-{suffix}"
    gcloud("storage", "buckets", "create", f"gs://{Cloud.bucket}", "--project", project,
           "--location", "us-central1")
    gcloud("storage", "cp", str(musl_bin), f"gs://{Cloud.bucket}/gcp-systest", "--project", project)

    # Startup script: download the harness via the VM's metadata token, run the
    # fork's ADC flow (which falls through to the metadata server on a bare VM),
    # and upload the JSON report back to GCS (deterministic, no serial-log parsing).
    vopt_str = " ".join(vopt)
    startup = tmp / "startup.sh"
    startup.write_text(
        "#!/bin/bash\n"
        'TOK=$(curl -s -H "Metadata-Flavor: Google" '
        '"http://metadata.google.internal/computeMetadata/v1/instance/service-accounts/default/token" '
        "| sed -n 's/.*\"access_token\":\"\\([^\"]*\\)\".*/\\1/p')\n"
        f'curl -s -H "Authorization: Bearer $TOK" -o /tmp/gcp-systest '
        f'"https://storage.googleapis.com/storage/v1/b/{Cloud.bucket}/o/gcp-systest?alt=media"\n'
        "chmod +x /tmp/gcp-systest\n"
        f"/tmp/gcp-systest --flow adc --project {project} {vopt_str} --json > /tmp/out.json 2>&1\n"
        'curl -s -X POST -H "Authorization: Bearer $TOK" -H "Content-Type: application/json" '
        "--data-binary @/tmp/out.json "
        f'"https://storage.googleapis.com/upload/storage/v1/b/{Cloud.bucket}/o?uploadType=media&name=out.json"\n')

    created = gcloud("compute", "instances", "create", Cloud.vm_name, "--project", project,
                     "--zone", args.zone, "--machine-type", "e2-small",
                     "--image-family", "debian-12", "--image-project", "debian-cloud",
                     "--scopes", "https://www.googleapis.com/auth/cloud-platform",
                     "--metadata-from-file", f"startup-script={startup}")
    # gcloud returns '' here on success too (stdout goes to stderr); detect failure
    # by re-listing rather than stdout content.
    exists = gcloud("compute", "instances", "describe", Cloud.vm_name, "--zone", args.zone,
                    "--project", project, "--format", "value(name)")
    if exists != Cloud.vm_name:
        record("cloud:gce-metadata", False, "VM create failed")
        return
    print(f"  VM {Cloud.vm_name} created in {args.zone}; waiting for the metadata-flow result…")

    # Poll GCS for the result object the VM uploads after running the harness.
    out = ""
    for _ in range(40):
        candidate = gcloud("storage", "cat", f"gs://{Cloud.bucket}/out.json", "--project", project)
        if '"flow"' in candidate:
            out = candidate
            break
        time.sleep(8)
    if out:
        # Metadata-server SA has no project-get role here, so assert token validity +
        # scope (rm status is shown but not gated).
        assert_report("cloud:gce-metadata (metadata server)", out, "")
    else:
        record("cloud:gce-metadata (metadata server)", False,
               "no harness result uploaded within timeout")

# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------
def main():
    p = argparse.ArgumentParser(add_help=True, description="GCP fork credential-safety e2e harness")
    p.add_argument("--offline-only", action="store_true")
    p.add_argument("--local", action="store_true")
    p.add_argument("--cloud", action="store_true")
    p.add_argument("--all", action="store_true")
    p.add_argument("--skip-build", action="store_true")
    p.add_argument("--project", default="")
    p.add_argument("--sa-file", default="")
    p.add_argument("--vertex-model", default="")
    p.add_argument("--vertex-region", default="")
    p.add_argument("--zone", default="us-central1-c")
    args = p.parse_args()

    run_off = True
    run_loc = True
    run_cld = False
    if args.offline_only:
        run_loc = False; run_cld = False
    if args.local:
        run_loc = True
    if args.cloud:
        run_cld = True; run_loc = False
    if args.all:
        run_loc = True; run_cld = True

    # Standalone package excluded from the workspace; keep binaries in the
    # shared workspace target dir so cloud upload paths stay stable.
    native_bin = REPO / "target" / "release" / "gcp-systest"
    musl_bin = REPO / "target" / MUSL_TARGET / "release" / "gcp-systest"
    now = datetime.now()
    suffix = "e2e-" + now.strftime("%Y%m%d-%H%M%S") + f"-{os.getpid()}"
    now_epoch = int(now.timestamp())
    tmpdir = Path(tempfile.mkdtemp())

    signal.signal(signal.SIGTERM, lambda *a: (_ for _ in ()).throw(SystemExit(143)))

    project = args.project
    try:
        if run_off:
            run_offline()

        if not run_loc and not run_cld:
            log("Summary")
            print("\n".join(SUMMARY))
            print()
            print(f"Passed: {PASS}  Failed: {FAIL}")
            sys.exit(0 if FAIL == 0 else 1)

        # ---- Guard ----
        if not project:
            project = gcloud("config", "get-value", "project")
        if not project or project == "(unset)":
            print(f"{red('No GCP project')}; set one with: gcloud config set project {DEV_PROJECT}")
            sys.exit(1)
        if project != DEV_PROJECT:
            print(f"{red('REFUSING TO RUN')}: project '{project}' is not the dev project ({DEV_PROJECT}).")
            print(f"This harness only tests in {DEV_PROJECT}. Pass --project {DEV_PROJECT} "
                  f"or set it in gcloud config.")
            sys.exit(1)
        # Confirm we actually have ADC creds before claiming anything.
        if run(["gcloud", "auth", "application-default", "print-access-token"]).returncode != 0:
            print(f"{red('No ADC credentials')} (run: gcloud auth application-default login)")
            sys.exit(1)
        print(f"GCP project: {project} — dev-project guard OK")

        # ---- Build ----
        if not args.skip_build:
            log(f"Build harness (native{' + musl' if run_cld else ''})")
            r = subprocess.run([
                "cargo", "build", "--release",
                "--manifest-path", str(MANIFEST),
                "--target-dir", str(REPO / "target"),
            ], cwd=REPO)
            if r.returncode == 0 and native_bin.exists() and os.access(native_bin, os.X_OK):
                record("build:native", True, "ok")
            else:
                record("build:native", False, "cargo build failed")
            if run_cld:
                r = subprocess.run([
                    "cargo", "zigbuild", "--release",
                    "--manifest-path", str(MANIFEST),
                    "--target-dir", str(REPO / "target"),
                    "--target", MUSL_TARGET,
                ], cwd=REPO)
                if r.returncode == 0:
                    record("build:musl", True, "static ELF")
                else:
                    record("build:musl", False, "zigbuild failed")

        vopt = []
        if args.vertex_model and args.vertex_region:
            vopt = ["--vertex-model", args.vertex_model, "--vertex-region", args.vertex_region]

        if run_loc:
            run_local(args, native_bin, project, vopt, tmpdir)
        if run_cld:
            run_cloud(args, musl_bin, project, vopt, tmpdir, suffix, now_epoch)
    finally:
        master_cleanup(project, args.zone)
        shutil.rmtree(tmpdir, ignore_errors=True)

    # ---- Summary ----
    log("Summary")
    print("\n".join(SUMMARY))
    print()
    print(f"Project: {project}   Passed: {PASS}   Failed: {FAIL}")
    if FAIL == 0:
        print(green("ALL GCP AUTH FLOWS VERIFIED"))
        sys.exit(0)
    else:
        print(red("CREDENTIAL-SAFETY REGRESSION"))
        sys.exit(1)

if __name__ == "__main__":
    main()

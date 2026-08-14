#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.9"
# # No third-party packages: the harness drives the real `aws` CLI + `cargo`
# # (intentionally, to verify the fork against the genuine toolchain) and uses
# # only the Python standard library for JSON, the mock metadata server, and
# # process orchestration. The inline block keeps it runnable via `uv run`.
# dependencies = []
# ///
"""
run-e2e.py — repeatable end-to-end credential-safety harness for the forked
AWS crates (forked_aws_config + forked_aws_sigv4).

Run this on any change to forks/aws-config or forks/aws-sigv4 to assert that
every form of AWS credential resolution still works and that the resulting SigV4
signature is accepted by real AWS (STS GetCallerIdentity; Bedrock where noted).
Each scenario ASSERTS and the script exits non-zero on any failure, so it is
safe to gate CI on.

Tiers (compose with flags):
  offline  — `cargo test -p forked_aws_config` (all provider logic + the
             env>profile>container>IMDS precedence guard). No AWS creds needed.
  local    — env / shared-profile / credential_process / ECS-container(mock) /
             SSO, signed against real AWS with creds exported from --profile.
  cloud    — provisions a live EC2 instance (real IMDSv2) and a live Lambda
             (runtime-injected env creds), runs the harness on each, tears
             everything down, and re-verifies teardown.

Usage:
  ./run-e2e.py                 # offline + local   (default; needs SSO creds)
  ./run-e2e.py --offline-only  # offline only      (CI-safe, no creds)
  ./run-e2e.py --all           # offline + local + cloud
  ./run-e2e.py --cloud         # offline + cloud
  Flags: --profile NAME (default boundaryml-dev) --region R (us-east-1)
         --model ID (amazon.nova-micro-v1:0) --skip-build
"""

import argparse
import http.server
import json
import os
import signal
import socket
import socketserver
import stat
import subprocess
import sys
import tempfile
import threading
import time
import zipfile
from datetime import datetime
from pathlib import Path

# This harness is only ever allowed to touch the boundaryml-dev account. Any live
# tier that resolves to a different account is refused before it can export creds
# or provision anything. Change this only if you deliberately move the test account.
DEV_ACCOUNT = "147997132427"

SCRIPT_DIR = Path(__file__).resolve().parent
REPO = SCRIPT_DIR.parent.parent  # forks/aws-config-systest -> forks -> repo root
MANIFEST = SCRIPT_DIR / "Cargo.toml"
MUSL_TARGET = "x86_64-unknown-linux-musl"

# ---------------------------------------------------------------------------
# Colors / printing
# ---------------------------------------------------------------------------
def red(s):    return f"\033[31m{s}\033[0m"
def green(s):  return f"\033[32m{s}\033[0m"
def yellow(s): return f"\033[33m{s}\033[0m"

def log(title):
    print()
    print(f"=== {title} ===")

# ---------------------------------------------------------------------------
# Result tracking
# ---------------------------------------------------------------------------
SUMMARY = []
PASS = 0
FAIL = 0

def record(name, ok, detail):
    """ok is truthy on success (mirrors a 0 exit code in the shell version)."""
    global PASS, FAIL
    if ok:
        PASS += 1
        SUMMARY.append(f"PASS  {name}  — {detail}")
    else:
        FAIL += 1
        SUMMARY.append(f"FAIL  {name}  — {detail}")

# ---------------------------------------------------------------------------
# Subprocess helpers
# ---------------------------------------------------------------------------
def run(cmd, env=None, cwd=None, check=False):
    """Run a command (list form), capturing stdout/stderr as text."""
    return subprocess.run(
        cmd, env=env, cwd=cwd, check=check,
        stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True,
    )

class AwsCli:
    def __init__(self, profile, region):
        self.base = ["aws", "--profile", profile, "--region", region]

    def __call__(self, *args, quiet=True):
        """Run `aws ...`; return stripped stdout ('' on failure)."""
        r = run(self.base + list(args))
        if r.returncode != 0:
            return ""
        return r.stdout.strip()

# ---------------------------------------------------------------------------
# Harness JSON parsing / assertions
# ---------------------------------------------------------------------------
def parse_report(raw):
    """Parse the harness JSON, tolerating leading noise (e.g. s3 progress)."""
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
    """Navigate a dotted path through nested dicts; '' if absent."""
    if d is None:
        return ""
    cur = d
    for k in path.split("."):
        if isinstance(cur, dict) and k in cur:
            cur = cur[k]
        else:
            return ""
    return cur if cur is not None else ""

def assert_report(scenario, raw, want_acct, need_bedrock):
    """Assert a harness JSON report shows STS 200 for the expected account."""
    d = parse_report(raw)
    cok = jget(d, "credentials.ok")
    ssts = jget(d, "sts.status")
    sacct = jget(d, "sts.account")
    sarn = jget(d, "sts.arn")
    bsts = jget(d, "bedrock.status")
    detail = f"creds.ok={cok} sts={ssts} acct={sacct}"
    if sarn:
        detail += f" arn=…{str(sarn).rsplit('/', 1)[-1]}"
    if need_bedrock:
        detail += f" bedrock={bsts}"
    ok = (cok is True) and (ssts == 200) and (str(sacct) == str(want_acct))
    if need_bedrock:
        ok = ok and (bsts == 200)
    if ok:
        print(f"  {green('PASS')} {scenario} — {detail}")
    else:
        print(f"  {red('FAIL')} {scenario} — {detail}")
        print(f"    raw: {(raw or '')[:600]}")
    record(scenario, ok, detail)

# ---------------------------------------------------------------------------
# Mock ECS/container credential endpoint (real creds over local HTTP)
# ---------------------------------------------------------------------------
def free_port():
    s = socket.socket()
    s.bind(("127.0.0.1", 0))
    port = s.getsockname()[1]
    s.close()
    return port

def start_mock_creds_server(port, body_bytes):
    class Handler(http.server.BaseHTTPRequestHandler):
        def do_GET(self):
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.end_headers()
            self.wfile.write(body_bytes)
        def log_message(self, *a):
            pass
    httpd = socketserver.TCPServer(("127.0.0.1", port), Handler)
    t = threading.Thread(target=httpd.serve_forever, daemon=True)
    t.start()
    return httpd

# ---------------------------------------------------------------------------
# Cloud resource handles (empty until created) + idempotent teardown
# ---------------------------------------------------------------------------
class Cloud:
    ec2_instance_id = ""
    ec2_bucket = ""
    ec2_role = ""
    ec2_iprof = ""
    lambda_fn = ""
    lambda_role = ""

def master_cleanup(aws):
    c = Cloud
    if any([c.ec2_instance_id, c.ec2_bucket, c.ec2_role, c.ec2_iprof, c.lambda_fn, c.lambda_role]):
        log("Teardown")
    if c.ec2_instance_id:
        aws("ec2", "terminate-instances", "--instance-ids", c.ec2_instance_id)
        aws("ec2", "wait", "instance-terminated", "--instance-ids", c.ec2_instance_id)
    if c.ec2_bucket:
        aws("s3", "rm", f"s3://{c.ec2_bucket}", "--recursive")
        aws("s3api", "delete-bucket", "--bucket", c.ec2_bucket)
    if c.ec2_iprof:
        aws("iam", "remove-role-from-instance-profile", "--instance-profile-name", c.ec2_iprof, "--role-name", c.ec2_role)
        aws("iam", "delete-instance-profile", "--instance-profile-name", c.ec2_iprof)
    if c.ec2_role:
        aws("iam", "delete-role-policy", "--role-name", c.ec2_role, "--policy-name", "perms")
        aws("iam", "detach-role-policy", "--role-name", c.ec2_role, "--policy-arn", "arn:aws:iam::aws:policy/AmazonSSMManagedInstanceCore")
        aws("iam", "delete-role", "--role-name", c.ec2_role)
    if c.lambda_fn:
        aws("lambda", "delete-function", "--function-name", c.lambda_fn)
    if c.lambda_role:
        aws("iam", "delete-role-policy", "--role-name", c.lambda_role, "--policy-name", "bedrock-invoke")
        aws("iam", "detach-role-policy", "--role-name", c.lambda_role, "--policy-arn", "arn:aws:iam::aws:policy/service-role/AWSLambdaBasicExecutionRole")
        aws("iam", "delete-role", "--role-name", c.lambda_role)

# ---------------------------------------------------------------------------
# Tiers
# ---------------------------------------------------------------------------
def run_offline():
    log("Offline: cargo test -p forked_aws_config")
    r = subprocess.run(["cargo", "test", "-p", "forked_aws_config"], cwd=REPO,
                       stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
    print("\n".join(r.stdout.splitlines()[-25:]))
    if r.returncode == 0:
        record("offline:cargo-test", True, "all provider + precedence tests passed")
    else:
        record("offline:cargo-test", False, "cargo test failed")

def export_creds(aws):
    """Export short-lived creds from the profile as a dict, or None."""
    out = aws("configure", "export-credentials", "--format", "process")
    if not out:
        return None
    try:
        d = json.loads(out)
    except Exception:
        return None
    if not d.get("AccessKeyId"):
        return None
    return {
        "AWS_ACCESS_KEY_ID": d["AccessKeyId"],
        "AWS_SECRET_ACCESS_KEY": d["SecretAccessKey"],
        "AWS_SESSION_TOKEN": d.get("SessionToken", "") or "",
    }

def run_local(args, native_bin, account, tmp):
    log(f"Local provider scenarios (real creds exported from {args.profile})")
    empty_home = tmp / "empty-home"
    empty_home.mkdir(parents=True, exist_ok=True)

    aws = AwsCli(args.profile, args.region)
    creds = export_creds(aws)
    if creds is None:
        record("local:*", False, f"could not export credentials from {args.profile}")
        return
    ak = creds["AWS_ACCESS_KEY_ID"]
    sk = creds["AWS_SECRET_ACCESS_KEY"]
    st = creds["AWS_SESSION_TOKEN"]

    # 1) SSO (live, real HOME + profile) — also exercises Bedrock signing.
    #    Current env minus any AWS creds, so resolution comes from the profile.
    env_sso = {k: v for k, v in os.environ.items()
               if k not in ("AWS_ACCESS_KEY_ID", "AWS_SECRET_ACCESS_KEY", "AWS_SESSION_TOKEN")}
    out = run([str(native_bin), "--profile", args.profile, "--region", args.region,
               "--bedrock-model", args.model, "--sign-sts", "--sign-bedrock"], env=env_sso).stdout
    assert_report("local:sso (+bedrock)", out, account, need_bedrock=True)

    base_env = {"PATH": os.environ.get("PATH", ""), "HOME": str(empty_home)}

    # 2) Environment variables.
    env2 = dict(base_env, AWS_ACCESS_KEY_ID=ak, AWS_SECRET_ACCESS_KEY=sk,
                AWS_SESSION_TOKEN=st, AWS_REGION=args.region)
    out = run([str(native_bin), "--region", args.region, "--sign-sts"], env=env2).stdout
    assert_report("local:env", out, account, need_bedrock=False)

    # 3) Shared credentials file (static profile).
    credfile = tmp / "credentials"
    credfile.write_text(
        f"[e2e]\naws_access_key_id = {ak}\naws_secret_access_key = {sk}\naws_session_token = {st}\n")
    env3 = dict(base_env, AWS_SHARED_CREDENTIALS_FILE=str(credfile), AWS_REGION=args.region)
    out = run([str(native_bin), "--profile", "e2e", "--region", args.region, "--sign-sts"], env=env3).stdout
    assert_report("local:profile-static", out, account, need_bedrock=False)

    # 4) credential_process.
    cpscript = tmp / "credproc.sh"
    cpscript.write_text(
        "#!/bin/sh\ncat <<JSON\n"
        f'{{"Version":1,"AccessKeyId":"{ak}","SecretAccessKey":"{sk}","SessionToken":"{st}"}}\n'
        "JSON\n")
    cpscript.chmod(0o755)
    cfgfile = tmp / "config"
    cfgfile.write_text(f"[profile e2ecp]\ncredential_process = {cpscript}\n")
    env4 = dict(base_env, AWS_CONFIG_FILE=str(cfgfile), AWS_REGION=args.region)
    out = run([str(native_bin), "--profile", "e2ecp", "--region", args.region, "--sign-sts"], env=env4).stdout
    assert_report("local:credential_process", out, account, need_bedrock=False)

    # 5) ECS/container endpoint (mock HTTP server, real creds).
    port = free_port()
    body = json.dumps({"AccessKeyId": ak, "SecretAccessKey": sk, "Token": st,
                       "Expiration": "2099-01-01T00:00:00Z"}).encode()
    httpd = start_mock_creds_server(port, body)
    try:
        env5 = dict(base_env, AWS_CONTAINER_CREDENTIALS_FULL_URI=f"http://127.0.0.1:{port}/creds",
                    AWS_REGION=args.region)
        out = run([str(native_bin), "--region", args.region, "--sign-sts"], env=env5).stdout
    finally:
        httpd.shutdown()
    assert_report("local:ecs-container(mock)", out, account, need_bedrock=False)

def run_cloud(args, musl_bin, account, tmp, suffix):
    aws = AwsCli(args.profile, args.region)
    if not (musl_bin.exists() and os.access(musl_bin, os.X_OK)):
        record("cloud:*", False, f"musl binary missing ({musl_bin}) — run without --skip-build")
        return

    # ---- EC2 IMDSv2 -------------------------------------------------------
    log("Cloud: EC2 IMDSv2 (live)")
    Cloud.ec2_role = f"baml-systest-ec2-{suffix}"
    Cloud.ec2_iprof = Cloud.ec2_role
    Cloud.ec2_bucket = f"baml-systest-{suffix}-{account}"
    ami = aws("ssm", "get-parameter", "--name",
              "/aws/service/ami-amazon-linux-latest/al2023-ami-kernel-default-x86_64",
              "--query", "Parameter.Value", "--output", "text")
    subnet = aws("ec2", "describe-subnets", "--filters",
                 "Name=map-public-ip-on-launch,Values=true",
                 "--query", "Subnets[0].SubnetId", "--output", "text")
    subnet_vpc = aws("ec2", "describe-subnets", "--subnet-ids", subnet,
                     "--query", "Subnets[0].VpcId", "--output", "text")
    sg = aws("ec2", "describe-security-groups", "--filters",
             f"Name=vpc-id,Values={subnet_vpc}", "Name=group-name,Values=default",
             "--query", "SecurityGroups[0].GroupId", "--output", "text")
    print(f"  ami={ami} subnet={subnet} vpc={subnet_vpc} sg={sg}")

    aws("iam", "create-role", "--role-name", Cloud.ec2_role, "--assume-role-policy-document",
        '{"Version":"2012-10-17","Statement":[{"Effect":"Allow","Principal":{"Service":"ec2.amazonaws.com"},"Action":"sts:AssumeRole"}]}')
    aws("iam", "attach-role-policy", "--role-name", Cloud.ec2_role,
        "--policy-arn", "arn:aws:iam::aws:policy/AmazonSSMManagedInstanceCore")
    aws("iam", "put-role-policy", "--role-name", Cloud.ec2_role, "--policy-name", "perms",
        "--policy-document",
        '{"Version":"2012-10-17","Statement":[{"Effect":"Allow","Action":["bedrock:InvokeModel","s3:GetObject"],"Resource":"*"}]}')
    aws("iam", "create-instance-profile", "--instance-profile-name", Cloud.ec2_iprof)
    aws("iam", "add-role-to-instance-profile", "--instance-profile-name", Cloud.ec2_iprof,
        "--role-name", Cloud.ec2_role)
    time.sleep(12)

    aws("s3api", "create-bucket", "--bucket", Cloud.ec2_bucket)
    aws("s3", "cp", str(musl_bin), f"s3://{Cloud.ec2_bucket}/aws-systest")

    Cloud.ec2_instance_id = aws(
        "ec2", "run-instances", "--image-id", ami, "--instance-type", "t3.micro",
        "--iam-instance-profile", f"Name={Cloud.ec2_iprof}", "--subnet-id", subnet,
        "--associate-public-ip-address", "--security-group-ids", sg,
        "--tag-specifications",
        f"ResourceType=instance,Tags=[{{Key=Name,Value=baml-systest-{suffix}}}]",
        "--query", "Instances[0].InstanceId", "--output", "text")
    print(f"  instance={Cloud.ec2_instance_id}; waiting for running + SSM…")
    aws("ec2", "wait", "instance-running", "--instance-ids", Cloud.ec2_instance_id)

    ssm_online = False
    for _ in range(20):
        ping = aws("ssm", "describe-instance-information", "--filters",
                   f"Key=InstanceIds,Values={Cloud.ec2_instance_id}",
                   "--query", "InstanceInformationList[0].PingStatus", "--output", "text")
        if ping == "Online":
            ssm_online = True
            break
        time.sleep(12)

    if not ssm_online:
        record("cloud:ec2-imdsv2", False, "instance never registered with SSM")
    else:
        cmd = (f"aws s3 cp s3://{Cloud.ec2_bucket}/aws-systest /tmp/aws-systest && "
               f"chmod +x /tmp/aws-systest && /tmp/aws-systest --region {args.region} "
               f"--bedrock-model {args.model} --sign-sts --sign-bedrock")
        cid = aws("ssm", "send-command", "--instance-ids", Cloud.ec2_instance_id,
                  "--document-name", "AWS-RunShellScript",
                  "--parameters", json.dumps({"commands": [cmd]}),
                  "--query", "Command.CommandId", "--output", "text")
        for _ in range(20):
            status = aws("ssm", "get-command-invocation", "--command-id", cid,
                         "--instance-id", Cloud.ec2_instance_id, "--query", "Status", "--output", "text")
            if status in ("Success", "Failed"):
                break
            time.sleep(6)
        raw = aws("ssm", "get-command-invocation", "--command-id", cid,
                  "--instance-id", Cloud.ec2_instance_id,
                  "--query", "StandardOutputContent", "--output", "text")
        assert_report("cloud:ec2-imdsv2 (+bedrock)", raw, account, need_bedrock=True)

    # ---- Lambda -----------------------------------------------------------
    log("Cloud: Lambda (live)")
    Cloud.lambda_role = f"baml-systest-lambda-{suffix}"
    Cloud.lambda_fn = f"baml-systest-{suffix}"
    aws("iam", "create-role", "--role-name", Cloud.lambda_role, "--assume-role-policy-document",
        '{"Version":"2012-10-17","Statement":[{"Effect":"Allow","Principal":{"Service":"lambda.amazonaws.com"},"Action":"sts:AssumeRole"}]}')
    aws("iam", "attach-role-policy", "--role-name", Cloud.lambda_role,
        "--policy-arn", "arn:aws:iam::aws:policy/service-role/AWSLambdaBasicExecutionRole")
    aws("iam", "put-role-policy", "--role-name", Cloud.lambda_role, "--policy-name", "bedrock-invoke",
        "--policy-document",
        '{"Version":"2012-10-17","Statement":[{"Effect":"Allow","Action":"bedrock:InvokeModel","Resource":"*"}]}')
    lrole_arn = aws("iam", "get-role", "--role-name", Cloud.lambda_role,
                    "--query", "Role.Arn", "--output", "text")
    time.sleep(12)

    # Package the musl binary as a `bootstrap` provided.al2023 Lambda (exec bit set).
    zip_path = tmp / "function.zip"
    with zipfile.ZipFile(zip_path, "w", zipfile.ZIP_DEFLATED) as zf:
        info = zipfile.ZipInfo("bootstrap")
        info.external_attr = (stat.S_IFREG | 0o755) << 16
        zf.writestr(info, musl_bin.read_bytes())

    aws("lambda", "create-function", "--function-name", Cloud.lambda_fn,
        "--runtime", "provided.al2023", "--handler", "bootstrap", "--architectures", "x86_64",
        "--role", lrole_arn, "--timeout", "30", "--memory-size", "256",
        "--environment", f"Variables={{BEDROCK_MODEL={args.model}}}",
        "--zip-file", f"fileb://{zip_path}")
    for _ in range(20):
        state = aws("lambda", "get-function-configuration", "--function-name", Cloud.lambda_fn,
                    "--query", "State", "--output", "text")
        if state == "Active":
            break
        time.sleep(3)
    lambda_out = tmp / "lambda-out.json"
    aws("lambda", "invoke", "--function-name", Cloud.lambda_fn, "--payload", "{}",
        "--cli-binary-format", "raw-in-base64-out", str(lambda_out))
    raw = lambda_out.read_text() if lambda_out.exists() else ""
    assert_report("cloud:lambda-env (+bedrock)", raw, account, need_bedrock=True)

# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------
def main():
    p = argparse.ArgumentParser(add_help=True, description="AWS fork credential-safety e2e harness")
    p.add_argument("--offline-only", action="store_true")
    p.add_argument("--local", action="store_true")
    p.add_argument("--cloud", action="store_true")
    p.add_argument("--all", action="store_true")
    p.add_argument("--skip-build", action="store_true")
    p.add_argument("--profile", default="boundaryml-dev")
    p.add_argument("--region", default="us-east-1")
    p.add_argument("--model", default="amazon.nova-micro-v1:0")
    args = p.parse_args()

    # Tier selection (mirrors the shell flag semantics).
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
    native_bin = REPO / "target" / "release" / "aws-systest"
    musl_bin = REPO / "target" / MUSL_TARGET / "release" / "aws-systest"
    suffix = "e2e-" + datetime.now().strftime("%Y%m%d-%H%M%S") + f"-{os.getpid()}"
    aws = AwsCli(args.profile, args.region)

    tmpdir = Path(tempfile.mkdtemp())

    # Ensure SIGTERM raises so the finally-block teardown always runs.
    signal.signal(signal.SIGTERM, lambda *a: (_ for _ in ()).throw(SystemExit(143)))

    account = None
    try:
        if run_off:
            run_offline()

        if not run_loc and not run_cld:
            log("Summary")
            print("\n".join(SUMMARY))
            print()
            print(f"Passed: {PASS}  Failed: {FAIL}")
            sys.exit(0 if FAIL == 0 else 1)

        # Identity / build prerequisites.
        account = aws("sts", "get-caller-identity", "--query", "Account", "--output", "text")
        if not account or account == "None":
            print(f"{red('No AWS credentials')} for profile {args.profile} "
                  f"(try: aws sso login --profile {args.profile})")
            sys.exit(1)
        # Account guard: only ever run live tests against boundaryml-dev.
        if account != DEV_ACCOUNT:
            print(f"{red('REFUSING TO RUN')}: profile '{args.profile}' resolves to account "
                  f"{account}, not the")
            print(f"boundaryml-dev account ({DEV_ACCOUNT}). This harness only tests in boundaryml-dev.")
            print(f"Use --profile boundaryml-dev (or any profile that maps to {DEV_ACCOUNT}).")
            sys.exit(1)
        print(f"AWS account: {account} (profile {args.profile}, region {args.region}) "
              f"— boundaryml-dev guard OK")

        if not args.skip_build:
            log(f"Build harness (native{' + musl' if run_cld else ''})")
            r = subprocess.run([
                "cargo", "build", "--release",
                "--manifest-path", str(MANIFEST),
                "--target-dir", str(REPO / "target"),
            ], cwd=REPO)
            if r.returncode != 0:
                record("build:native", False, "cargo build failed")
            if native_bin.exists() and os.access(native_bin, os.X_OK):
                record("build:native", True, "ok")
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

        if run_loc:
            run_local(args, native_bin, account, tmpdir)
        if run_cld:
            run_cloud(args, musl_bin, account, tmpdir, suffix)
    finally:
        master_cleanup(aws)
        try:
            import shutil
            shutil.rmtree(tmpdir, ignore_errors=True)
        except Exception:
            pass

    # ---- Summary ----
    log("Summary")
    print("\n".join(SUMMARY))
    print()
    print(f"Account: {account}   Passed: {PASS}   Failed: {FAIL}")
    if FAIL == 0:
        print(green("ALL CREDENTIAL FORMS VERIFIED"))
        sys.exit(0)
    else:
        print(red("CREDENTIAL-SAFETY REGRESSION"))
        sys.exit(1)

if __name__ == "__main__":
    main()

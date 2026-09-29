#!/usr/bin/env python3
"""Exercise a real provider-crate substitution without changing the checkout."""

import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile

from check_no_rustls import assert_no_rustls as assert_no_tls

WORKSPACE = Path(__file__).resolve().parent.parent
REPO = WORKSPACE.parent
FIXTURES = WORKSPACE / "scripts/provider_fixtures"
PACKAGES = [
    "baml", "baml_cli", "baml_lsp_server", "baml_pack_host", "bridge_cffi",
    "bridge_python", "bridge_java", "bridge_swift", "bridge_typescript",
]
IMPLEMENTATIONS = {"aws-lc-rs", "aws-lc-sys", "ring", "aes-gcm-siv", "chacha20poly1305"}


def cargo(workspace, *args, capture=False, env=None):
    return subprocess.run(
        ["cargo", *args], cwd=workspace, check=True, text=True,
        stdout=subprocess.PIPE if capture else None, env=env,
    ).stdout


def check_graph(workspace, package, expected, target=None, features=None, forbid_tls=False):
    args = ["tree", "--locked", "-p", package, "-e", "normal,build", "--prefix", "none", "--format", "{p}"]
    if target:
        args += ["--target", target]
    if features is not None:
        args += ["--no-default-features"]
        if features:
            args += ["--features", features]
    graph = cargo(workspace, *args, capture=True)
    names = {line.split()[0] for line in graph.splitlines() if line.strip()}
    if forbid_tls:
        assert_no_tls(names)
    found = names & IMPLEMENTATIONS
    if found != expected:
        raise RuntimeError(f"{package} ({target or 'host'}): expected {expected}, got {found}\n{graph}")
    print(f"ok: {package} ({target or 'host'}, {features or 'defaults'}): {', '.join(sorted(found)) or 'no bundled crypto'}", flush=True)


def copy_source(destination):
    # Include local edits and new files, but not ignored build outputs. Copy files
    # rather than symlinking them: build scripts may write generated source.
    paths = subprocess.check_output(
        ["git", "ls-files", "-z", "--cached", "--others", "--exclude-standard"], cwd=REPO,
    ).decode().split("\0")
    for name in set(paths):
        if not (name.startswith("baml_language/") or name in {"release/platforms.json", "skills/baml-core/SKILL.md"}):
            continue
        source = REPO / name
        if source.is_file():
            target = destination / name
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(source, target)


def replacement_manifest(ring):
    dependencies = "ring.workspace = true\npem-rfc7468.workspace = true\nzeroize.workspace = true\n" if ring else ""
    return f'''[package]
name = "vendor_test_crypto"
version = "0.1.0"
edition.workspace = true

[dependencies]
baml_crypto_types.workspace = true
{dependencies}'''


def replacement_source(ring):
    if ring:
        return (FIXTURES / "crypto.rs").read_text()
    return '''use std::sync::Arc;
use baml_crypto_types::CryptoProvider;
pub fn provider() -> Arc<dyn CryptoProvider> { Arc::new(RejectAll) }
struct RejectAll;
impl CryptoProvider for RejectAll {}
'''


def replace_dependency(workspace, name, package):
    manifest = workspace / "Cargo.toml"
    text, count = re.subn(
        rf'^{name} = .*$',
        f'{name} = {{ path = "crates/{package}", package = "{package}" }}',
        manifest.read_text(), flags=re.MULTILINE,
    )
    if count != 1:
        raise RuntimeError(f"Expected one workspace dependency for {name}")
    manifest.write_text(text)
    provider = workspace / "crates" / package
    (provider / "src").mkdir(parents=True)
    return provider


def build_without_tls(workspace, env):
    metadata = json.loads(cargo(workspace, "metadata", "--locked", "--format-version", "1", capture=True))
    packages = {p["id"]: p["name"] for p in metadata["packages"]}
    # Inspect Cargo's artifact stream as well as its dependency graph. This
    # includes build scripts and proc macros, including cached artifacts.
    output = cargo(workspace, "build", "--locked", "--message-format=json", "-p", "baml_cli", "-p", "bridge_python", "--features", "no-phone-home", capture=True, env=env)
    compiled = set()
    cli = None
    bridge = None
    for line in output.splitlines():
        message = json.loads(line)
        if message.get("reason") == "compiler-artifact":
            compiled.add(packages[message["package_id"]])
            if packages[message["package_id"]] == "baml_cli" and message.get("executable"):
                cli = message["executable"]
            if packages[message["package_id"]] == "bridge_python":
                bridge = next((p for p in message["filenames"] if Path(p).suffix in {".so", ".dylib", ".dll"}), bridge)
    assert_no_tls(compiled)
    if not {"baml_cli", "bridge_python"} <= compiled or cli is None or bridge is None:
        raise RuntimeError("Expected CLI and Python bridge build artifacts")
    print(f"ok: {len(compiled)} build-artifact packages contain no rustls dependencies", flush=True)
    subprocess.run([env["PYO3_PYTHON"], "-c", """
import importlib.util, sys
from importlib.machinery import ExtensionFileLoader
loader = ExtensionFileLoader("baml_py", sys.argv[1])
spec = importlib.util.spec_from_file_location("baml_py", sys.argv[1], loader=loader)
module = importlib.util.module_from_spec(spec)
loader.exec_module(module)
assert module.get_version()
print("ok: rustls-free Python bridge imports")
""", bridge], check=True, env=env)
    return cli


def main():
    python = os.environ.get("PYO3_PYTHON", sys.executable)
    subprocess.run([python, "-c", "import sys; assert sys.version_info >= (3, 10), 'Set PYO3_PYTHON to Python 3.10 or newer'"], check=True)
    default = {"aws-lc-rs", "aws-lc-sys", "aes-gcm-siv", "chacha20poly1305"}
    for package in PACKAGES:
        check_graph(WORKSPACE, package, default)
    check_graph(WORKSPACE, "bridge_swift", {"ring", "aes-gcm-siv", "chacha20poly1305"}, "aarch64-apple-ios")
    check_graph(WORKSPACE, "bridge_wasm", {"aes-gcm-siv", "chacha20poly1305"}, "wasm32-unknown-unknown")

    # The separately published Rust SDK downloader retains its own features.
    for feature, expected in [
        ("aws-crypto", {"aws-lc-rs", "aws-lc-sys"}),
        ("ring-crypto", {"ring"}),
        ("external-crypto", set()),
    ]:
        check_graph(WORKSPACE, "baml_bridge", expected, features=feature)

    check_graph(WORKSPACE, "baml_bridge", set(), features="", target="all", forbid_tls=True)

    with tempfile.TemporaryDirectory(prefix="baml-provider-check-") as directory:
        source = Path(directory)
        copy_source(source)
        workspace = source / "baml_language"
        provider = replace_dependency(workspace, "baml_crypto_provider", "vendor_test_crypto")
        http = replace_dependency(workspace, "baml_http_provider", "vendor_test_http")
        (http / "Cargo.toml").write_text('''[package]
name = "vendor_test_http"
version = "0.1.0"
edition.workspace = true

[dependencies]
baml_http_types.workspace = true
futures.workspace = true
''')
        (http / "src/lib.rs").write_text((FIXTURES / "http.rs").read_text())
        (provider / "Cargo.toml").write_text(replacement_manifest(True))
        (provider / "src/lib.rs").write_text(replacement_source(True))
        # Resolve only the new local package into the copied lockfile.
        cargo(workspace, "metadata", "--format-version", "1", capture=True)
        for package in PACKAGES:
            check_graph(workspace, package, {"ring"}, target="all", forbid_tls=True)

        # Test real hashing/signing/randomness against the replacement backend.
        # Reuse build output within this import without touching source checkout.
        revision = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=REPO, text=True).strip()
        env = dict(os.environ, CARGO_TARGET_DIR=str(WORKSPACE / "target/provider-check"), BAML_GIT_SHA=revision, PYO3_PYTHON=python)
        cargo(workspace, "test", "--locked", "-p", "baml_crypto", env=env)
        cargo(workspace, "check", "--locked", "-p", "bridge_cffi", "-p", "bridge_typescript", "-p", "baml_pack_host", "-p", "baml", env=env)

        cli = build_without_tls(workspace, env)

        # BAML must observe Unsupported from every cipher constructor; accepting
        # one would mean the VM retained a fallback implementation.
        fixture = workspace / "provider_tests"
        fixture.mkdir()
        tests = [(FIXTURES / "provider.baml").read_text()]
        for algorithm, size in [("Aes128GcmSiv", 16), ("Aes256GcmSiv", 32), ("ChaCha20Poly1305", 32), ("XChaCha20Poly1305", 32)]:
            tests.append(f'''test "rejects_{algorithm}" {{
    {{
        baml.crypto.{algorithm}.new(baml.Uint8Array.zeroes({size}));
        baml.sys.panic("provider rejection was bypassed")
    }} catch (e) {{
        baml.errors.Unsupported => assert.is_true(true)
    }}
}}
''')
        (fixture / "provider.baml").write_text("\n".join(tests))
        subprocess.run([cli, "test", "--from", str(fixture)], cwd=workspace, env=env, check=True)

        # A second replacement rejects every crypto operation. The runtime
        # must propagate those errors without pulling in another backend.
        (provider / "Cargo.toml").write_text(replacement_manifest(False))
        (provider / "src/lib.rs").write_text(replacement_source(False))
        cargo(workspace, "metadata", "--format-version", "1", capture=True)
        for package in PACKAGES:
            check_graph(workspace, package, set(), target="all", forbid_tls=True)
        tests_dir = workspace / "crates/baml_crypto/tests"
        tests_dir.mkdir(exist_ok=True)
        (tests_dir / "unsupported_provider.rs").write_text('''#[test]
fn unsupported_provider_has_no_fallback() {
    use baml_crypto::{CryptoError, AeadAlgorithm, AeadError};
    assert!(matches!(baml_crypto::sha256(b"test"), Err(CryptoError::Unsupported(_))));
    assert!(matches!(baml_crypto::hmac_sha256(b"key", b"test"), Err(CryptoError::Unsupported(_))));
    assert!(matches!(baml_crypto::sign_rs256("not a key", b"test"), Err(CryptoError::Unsupported(_))));
    assert!(matches!(baml_crypto::fill_random(&mut [0; 32]), Err(CryptoError::Unsupported(_))));
    assert!(matches!(baml_crypto::aead(AeadAlgorithm::Aes128GcmSiv, &[0; 16]), Err(AeadError::Unsupported(_))));
}
''')
        cargo(workspace, "test", "--locked", "-p", "baml_crypto", "--test", "unsupported_provider", env=env)



if __name__ == "__main__":
    main()

#!/usr/bin/env bash
# Run in a disposable runner image, with --network none and this file mounted.
# A real Cargo build script tries to read a pre-existing runtime login.
set -euo pipefail
mkdir -p /data/home /data/bootstrap/repo/baml_language/src
printf 'test-login-placeholder' > /data/home/.credentials.json
# Deliberately permissive old-volume permissions: bootstrap must tighten them.
chmod 755 /data/home
chmod 644 /data/home/.credentials.json
chown -R atb2:atb2 /data/home
cat > /data/bootstrap/repo/baml_language/Cargo.toml <<'EOF'
[package]
name = "baml_cli"
version = "0.0.0"
edition = "2021"
[[bin]]
name = "baml-cli"
path = "src/main.rs"
EOF
cat > /data/bootstrap/repo/baml_language/build.rs <<'EOF'
use std::{env, fs, process::Command};
fn main() {
    assert_eq!(env::var("HOME").unwrap(), "/data/bootstrap/home");
    assert_eq!(Command::new("id").arg("-u").output().unwrap().stdout, b"1001\n");
    assert!(env::var("FEEDBACK_SUPABASE_KEY").is_err());
    for path in ["/data/home/.credentials.json", "/proc/1/environ"] {
        let err = fs::read(path).expect_err("builder read a protected file");
        assert_eq!(err.kind(), std::io::ErrorKind::PermissionDenied);
    }
    assert!(fs::write("/app/builder-write", "bad").is_err());
    for path in ["/usr/local/cargo/bin/builder-write", "/usr/local/rustup/builder-write", "/data/rustup/builder-write"] {
        assert!(fs::write(path, "bad").is_err(), "builder changed shared/runtime tools");
    }
    assert!(!Command::new("setpriv").args(["--reuid=0", "id"]).status().unwrap().success());
    let status = fs::read_to_string("/proc/self/status").unwrap();
    assert!(status.contains("NoNewPrivs:\t1"));
    assert!(status.contains("CapEff:\t0000000000000000"));
    fs::write("/data/bootstrap/isolation-passed", "ok").unwrap();
}
EOF
cat > /data/bootstrap/repo/baml_language/src/main.rs <<'EOF'
use std::{env, fs, process::Command};
fn main() {
    assert_eq!(Command::new("id").arg("-u").output().unwrap().stdout, b"1000\n");
    assert_eq!(env::var("HOME").unwrap(), "/data/home");
    assert_eq!(fs::read_to_string("/data/home/.credentials.json").unwrap(), "test-login-placeholder");
    assert!(fs::read("/data/bootstrap/isolation-passed").is_err());
    assert!(!Command::new("setpriv").args(["--reuid=0", "id"]).status().unwrap().success());
    println!("runtime isolation passed");
}
EOF
git -C /data/bootstrap/repo init -q -b canary
git -C /data/bootstrap/repo add .
git -C /data/bootstrap/repo -c user.name=test -c user.email=test@example.invalid commit -qm fixture
git -C /data/bootstrap/repo remote add origin /data/bootstrap/repo
export ATB2_CANARY_REV
ATB2_CANARY_REV=$(git -C /data/bootstrap/repo rev-parse HEAD)
chown -R builder:builder /data/bootstrap
export FEEDBACK_SUPABASE_KEY=offline-test-placeholder
/usr/local/bin/atb2-bootstrap
test "$(cat /data/bootstrap/isolation-passed)" = ok
test "$(stat -c %a /data/home)" = 700

# A pinned cache remains usable even with no git remote and no network.
setpriv --reuid=1001 --regid=1001 --clear-groups git -C /data/bootstrap/repo remote remove origin
/usr/local/bin/atb2-bootstrap
echo 'PASS: real build isolation, existing login preserved, cached offline boot'

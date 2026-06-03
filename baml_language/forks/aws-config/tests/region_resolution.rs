#![allow(clippy::doc_markdown)]

//! Region-resolution parity tests for the slim `aws_config` fork.
//!
//! These mirror the behavioral cases from upstream `aws-config`:
//!   - `src/environment/region.rs` (`no_region`, `prioritize_aws_region`,
//!     `fallback_to_default_region`)
//!   - `src/profile/region.rs` (`load_region`, `load_region_env_profile_override`,
//!     `load_region_explicit_override`, `load_region_nonexistent_profile`)
//!   - the `test-data/profile-provider/region_override` fixture (env beats profile)
//!
//! Upstream cases that rely on machinery we did not port (`source_profile`
//! region chaining, the Smithy region-provider chain in `src/meta/region.rs`)
//! are intentionally skipped — see the bottom of this file.

mod common;

use aws_config::*;
use common::MockIo;

/// Upstream `environment::region::test::no_region`: with no env and no profile
/// files, the resolver yields `None`.
#[tokio::test]
async fn no_region_returns_none() {
    let io = MockIo::new();
    assert_eq!(resolve_region(&io, None).await, None);
}

/// Upstream `environment::region::test::prioritize_aws_region`: `AWS_REGION`
/// wins over `AWS_DEFAULT_REGION`.
#[tokio::test]
async fn aws_region_beats_default_region() {
    let io = MockIo::new()
        .env("AWS_REGION", "us-east-1")
        .env("AWS_DEFAULT_REGION", "us-east-2");
    assert_eq!(
        resolve_region(&io, None).await,
        Some("us-east-1".to_string())
    );
}

/// Upstream `environment::region::test::fallback_to_default_region`: when
/// `AWS_REGION` is unset, fall back to `AWS_DEFAULT_REGION`.
#[tokio::test]
async fn falls_back_to_default_region() {
    let io = MockIo::new().env("AWS_DEFAULT_REGION", "us-east-2");
    assert_eq!(
        resolve_region(&io, None).await,
        Some("us-east-2".to_string())
    );
}

/// Upstream's env region provider treats a set-but-empty `AWS_REGION` as the
/// selected region, so it still beats `AWS_DEFAULT_REGION`.
#[tokio::test]
async fn empty_aws_region_still_beats_default_region() {
    let io = MockIo::new()
        .env("AWS_REGION", "")
        .env("AWS_DEFAULT_REGION", "us-east-2");
    assert_eq!(resolve_region(&io, None).await, Some(String::new()));
}

/// An empty `AWS_DEFAULT_REGION` is also considered configured when
/// `AWS_REGION` is unset, matching upstream's environment region provider.
#[tokio::test]
async fn empty_default_region_beats_profile_region() {
    let io = MockIo::new()
        .env("AWS_DEFAULT_REGION", "")
        .env("HOME", "/home")
        .file("/home/.aws/config", "[default]\nregion = us-west-2\n");
    assert_eq!(resolve_region(&io, None).await, Some(String::new()));
}

/// Upstream `profile::region::test::load_region` (`region_override` fixture):
/// with no env override the default profile's `region` from the config file is
/// used.
#[tokio::test]
async fn region_from_default_profile_config_file() {
    let io = MockIo::new().env("HOME", "/home").file(
        "/home/.aws/config",
        "[default]\nregion = us-east-1\n\n[profile base]\nregion = eu-west-1\n",
    );
    assert_eq!(
        resolve_region(&io, None).await,
        Some("us-east-1".to_string())
    );
}

/// Upstream `profile::region::test::load_region_env_profile_override`: the
/// active profile is selected via `AWS_PROFILE`, and that profile's region is
/// returned.
#[tokio::test]
async fn region_from_profile_selected_by_aws_profile_env() {
    let io = MockIo::new()
        .env("HOME", "/home")
        .env("AWS_PROFILE", "base")
        .file(
            "/home/.aws/config",
            "[default]\nregion = us-east-1\n\n[profile base]\nregion = eu-west-1\n",
        );
    assert_eq!(
        resolve_region(&io, None).await,
        Some("eu-west-1".to_string())
    );
}

/// Upstream `profile::region::test::load_region_explicit_override`: an explicit
/// `profile_override` argument selects the profile, taking precedence over
/// `AWS_PROFILE`.
#[tokio::test]
async fn region_from_profile_selected_by_explicit_override() {
    let io = MockIo::new()
        .env("HOME", "/home")
        .env("AWS_PROFILE", "default")
        .file(
            "/home/.aws/config",
            "[default]\nregion = us-east-1\n\n[profile base]\nregion = eu-west-1\n",
        );
    assert_eq!(
        resolve_region(&io, Some("base")).await,
        Some("eu-west-1".to_string())
    );
}

/// Upstream `profile::region::test::load_region_nonexistent_profile`:
/// selecting a profile that does not exist yields `None`.
#[tokio::test]
async fn nonexistent_profile_returns_none() {
    let io = MockIo::new()
        .env("HOME", "/home")
        .env("AWS_PROFILE", "doesnotexist")
        .file(
            "/home/.aws/config",
            "[default]\nregion = us-east-1\n\n[profile base]\nregion = eu-west-1\n",
        );
    assert_eq!(resolve_region(&io, None).await, None);
}

/// Upstream `region_override` fixture intent: an `AWS_REGION` env var overrides
/// the region set in the profile config file.
#[tokio::test]
async fn env_region_overrides_profile_region() {
    let io = MockIo::new()
        .env("AWS_REGION", "us-east-2")
        .env("HOME", "/home")
        .file("/home/.aws/config", "[default]\nregion = us-east-1\n");
    assert_eq!(
        resolve_region(&io, None).await,
        Some("us-east-2".to_string())
    );
}

/// Empty `profile_override` is treated as unset, so `AWS_PROFILE` still selects
/// the profile. Guards the `filter(|s| !s.is_empty())` on the override.
#[tokio::test]
async fn empty_profile_override_falls_back_to_aws_profile() {
    let io = MockIo::new()
        .env("HOME", "/home")
        .env("AWS_PROFILE", "base")
        .file(
            "/home/.aws/config",
            "[default]\nregion = us-east-1\n\n[profile base]\nregion = eu-west-1\n",
        );
    assert_eq!(
        resolve_region(&io, Some("")).await,
        Some("eu-west-1".to_string())
    );
}

/// Credentials-file region overrides the config-file region for the same
/// profile (credentials file is higher precedence, merged per-key).
#[tokio::test]
async fn credentials_file_region_overrides_config_file() {
    let io = MockIo::new()
        .env("HOME", "/home")
        .file("/home/.aws/config", "[default]\nregion = us-east-1\n")
        .file("/home/.aws/credentials", "[default]\nregion = ap-south-1\n");
    assert_eq!(
        resolve_region(&io, None).await,
        Some("ap-south-1".to_string())
    );
}

/// Mirrors upstream `profile::region::test::load_region_from_source_profile`:
/// when the selected profile has no `region`, follow `source_profile` links.
#[tokio::test]
async fn region_from_source_profile() {
    let io = MockIo::new().env("HOME", "/home").file(
        "/home/.aws/config",
        "[profile credentials]\n\
         region = us-east-1\n\
         \n\
         [profile needs-source]\n\
         source_profile = credentials\n\
         role_arn = arn:aws:iam::123456789012:role/test\n",
    );
    assert_eq!(
        resolve_region(&io, Some("needs-source")).await,
        Some("us-east-1".to_string())
    );
}

/// Source-profile cycles resolve to `None`, matching upstream's cycle guard.
#[tokio::test]
async fn source_profile_cycle_returns_none() {
    let io = MockIo::new().env("HOME", "/home").file(
        "/home/.aws/config",
        "[profile a]\nsource_profile = b\n\n[profile b]\nsource_profile = a\n",
    );
    assert_eq!(resolve_region(&io, Some("a")).await, None);
}

// ---------------------------------------------------------------------------
// Intentionally NOT ported:
//   * `meta::region::test::{provider_chain, empty_chain}` — these exercise the
//     Smithy `RegionProviderChain` combinator, which the slim resolver replaces
//     with a fixed precedence inside `resolve_region`.
// ---------------------------------------------------------------------------

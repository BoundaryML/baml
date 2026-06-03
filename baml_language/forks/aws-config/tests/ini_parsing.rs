#![allow(clippy::doc_markdown)]

//! Integration coverage for the public `IniFile` parser, mirroring the
//! behavioral cases in the upstream AWS profile-file parser
//! (`aws-runtime/src/env_config/parse.rs` and the shared JSON suite
//! `aws-runtime/test-data/profile-parser-tests.json`).
//!
//! Our slim parser operates at a *raw* layer: it does not lowercase keys,
//! normalize the `profile ` / `sso-session ` prefixes, or support
//! sub-property continuations. Those upstream cases are intentionally not
//! ported. The cases below are the ones whose behavior our parser shares.

mod common;

use aws_config::*;
#[allow(unused_imports)]
use common::MockIo;

/// Mirrors "Profiles can contain properties." and
/// "Profiles can contain multiple properties." from the upstream JSON suite:
/// a `[section]` header followed by `key = value` lines populates that
/// section, and multiple sections are kept distinct.
#[tokio::test]
async fn parses_sections_and_keys() {
    let ini = IniFile::parse(
        "\
[default]
aws_access_key_id = AKIA
aws_secret_access_key = SECRET

[profile work]
region = us-west-2
",
    );

    let default = ini.section("default").unwrap();
    assert_eq!(default.get("aws_access_key_id").unwrap(), "AKIA");
    assert_eq!(default.get("aws_secret_access_key").unwrap(), "SECRET");

    let work = ini.section("profile work").unwrap();
    assert_eq!(work.get("region").unwrap(), "us-west-2");

    // Section names are raw at this layer: there is no bare "work" section.
    assert!(ini.section("work").is_none());
}

/// Mirrors upstream "Property keys and values are trimmed." Surrounding
/// spaces and tabs around both the key and the value are stripped.
#[tokio::test]
async fn trims_key_and_value_whitespace() {
    let ini = IniFile::parse("[s]\n\tname \t=  \tvalue \t\n");
    assert_eq!(ini.section("s").unwrap().get("name").unwrap(), "value");
}

/// Mirrors upstream "Property values can be empty." (`name =`): a key with an
/// empty value is still recorded.
#[tokio::test]
async fn empty_value_is_recorded() {
    let ini = IniFile::parse("[s]\nname =\n");
    let s = ini.section("s").unwrap();
    assert_eq!(s.get("name").unwrap(), "");
}

/// Mirrors upstream "Equals signs are supported in property values."
/// (`name = val=ue`): only the first `=` splits key from value.
#[tokio::test]
async fn equals_in_value_is_preserved() {
    let ini = IniFile::parse("[s]\nname = val=ue\n");
    assert_eq!(ini.section("s").unwrap().get("name").unwrap(), "val=ue");
}

/// Mirrors upstream "Pound sign comments are ignored." and
/// "Semicolon sign comments are ignored." A whole-line `#`/`;` comment is
/// dropped, and a trailing ` # note` / ` ; note` (preceded by whitespace) is
/// stripped from the value.
#[tokio::test]
async fn strips_line_and_trailing_comments() {
    let ini = IniFile::parse(
        "\
# a comment
; another comment
[default]
region = us-east-1 ; trailing comment
key = value # note
",
    );
    let d = ini.section("default").unwrap();
    assert_eq!(d.get("region").unwrap(), "us-east-1");
    assert_eq!(d.get("key").unwrap(), "value");
}

/// Mirrors the upstream `prepare_line_strips_comments` unit test and
/// "Comments adjacent to values are included in the value." A `#` that is NOT
/// preceded by whitespace does not start a comment, so `a#b` is kept whole.
#[tokio::test]
async fn keeps_hash_without_preceding_space() {
    let ini = IniFile::parse("[s]\nurl = http://x/a#b\nname2 = value#Adjacent\n");
    let s = ini.section("s").unwrap();
    assert_eq!(s.get("url").unwrap(), "http://x/a#b");
    assert_eq!(s.get("name2").unwrap(), "value#Adjacent");
}

/// Mirrors the upstream `prepare_line_strips_comments` case
/// `"name = value # Comment with # sign"`: once a whitespace-preceded comment
/// starts, the rest of the line (including any later `#`) is dropped.
#[tokio::test]
async fn first_whitespace_comment_drops_rest_of_line() {
    let ini = IniFile::parse("[s]\nname = value # Comment with # sign\n");
    assert_eq!(ini.section("s").unwrap().get("name").unwrap(), "value");
}

/// Mirrors upstream "Blank lines are ignored." Empty and whitespace-only
/// lines are skipped without affecting the surrounding sections.
#[tokio::test]
async fn blank_lines_are_ignored() {
    let ini = IniFile::parse("\t \n[s]\n\t\n \nname = value\n\t \n");
    let s = ini.section("s").unwrap();
    assert_eq!(s.get("name").unwrap(), "value");
    assert_eq!(s.len(), 1);
}

/// Mirrors upstream "Duplicate properties in a profile use the last one
/// defined." (`name = value` then `name = value2`).
#[tokio::test]
async fn duplicate_keys_last_wins() {
    let ini = IniFile::parse("[s]\nname = value\nname = value2\n");
    assert_eq!(ini.section("s").unwrap().get("name").unwrap(), "value2");
}

/// Mirrors upstream "Multiple profiles can have properties." Two distinct
/// sections each retain their own keys.
#[tokio::test]
async fn multiple_sections_kept_distinct() {
    let ini = IniFile::parse(
        "\
[foo]
name = value
[bar]
name2 = value2
",
    );
    assert_eq!(ini.section("foo").unwrap().get("name").unwrap(), "value");
    assert!(ini.section("foo").unwrap().get("name2").is_none());
    assert_eq!(ini.section("bar").unwrap().get("name2").unwrap(), "value2");
    assert!(ini.section("bar").unwrap().get("name").is_none());
}

/// Mirrors upstream "Profile names should be trimmed." and "profile name with
/// extra whitespace": whitespace inside the brackets around the (raw) section
/// name is trimmed. Section names are raw here, so the `profile ` prefix is
/// part of the name.
#[tokio::test]
async fn section_name_is_trimmed() {
    let ini = IniFile::parse("[   profile foo    ]\nname = value\n");
    assert_eq!(
        ini.section("profile foo").unwrap().get("name").unwrap(),
        "value"
    );
    // The untrimmed form is not how it is stored.
    assert!(ini.section("   profile foo    ").is_none());
}

/// Mirrors upstream "Properties must be defined in a profile." Our slim parser
/// is lenient (it does not error): a `key = value` line that appears before
/// any `[section]` header is simply skipped.
#[tokio::test]
async fn property_before_any_section_is_skipped() {
    let ini = IniFile::parse("name = value\n[s]\nk = v\n");
    assert!(ini.section("s").unwrap().get("k").is_some());
    // The orphan property produced no section.
    assert_eq!(ini.iter().count(), 1);
}

/// Mirrors upstream "Property definitions must contain an equals sign."
/// (`key : value`). Our lenient parser skips a line with no `=` rather than
/// erroring, leaving the section otherwise intact.
#[tokio::test]
async fn malformed_line_without_equals_is_skipped() {
    let ini = IniFile::parse("[s]\nkey : value\ngood = yes\n");
    let s = ini.section("s").unwrap();
    assert!(s.get("key").is_none());
    assert_eq!(s.get("good").unwrap(), "yes");
    assert_eq!(s.len(), 1);
}

/// Mirrors upstream "Property key cannot be empty." (`= value`). A line whose
/// key trims to empty is not recorded.
#[tokio::test]
async fn empty_key_is_not_recorded() {
    let ini = IniFile::parse("[s]\n= value\nk = v\n");
    let s = ini.section("s").unwrap();
    assert_eq!(s.len(), 1);
    assert_eq!(s.get("k").unwrap(), "v");
}

/// Mirrors upstream "Windows style line endings are supported." `\r\n`
/// terminated lines parse the same as `\n`.
#[tokio::test]
async fn windows_line_endings_supported() {
    let ini = IniFile::parse("[s]\r\nname = value\r\n");
    assert_eq!(ini.section("s").unwrap().get("name").unwrap(), "value");
}

/// Mirrors the upstream notion that an `sso-session` header without a space
/// before the name is just a profile name ("sso-session without the space
/// before the name is a profile"). At our raw layer the section name is taken
/// verbatim, so `[sso-session x]` and `[sso-sessionfoo]` are distinct raw
/// names.
#[tokio::test]
async fn raw_section_names_for_sso_session_and_default() {
    let ini = IniFile::parse(
        "\
[default]
region = us-east-1
[sso-session x]
sso_region = us-west-2
[sso-sessionfoo]
name = value
",
    );
    assert_eq!(
        ini.section("default").unwrap().get("region").unwrap(),
        "us-east-1"
    );
    assert_eq!(
        ini.section("sso-session x")
            .unwrap()
            .get("sso_region")
            .unwrap(),
        "us-west-2"
    );
    assert_eq!(
        ini.section("sso-sessionfoo").unwrap().get("name").unwrap(),
        "value"
    );
    // No accidental merge between the spaced and unspaced sso-session names.
    assert!(ini.section("sso-session x").unwrap().get("name").is_none());
}

/// Mirrors upstream "Duplicate profiles in the same file merge properties."
/// Re-opening a section keeps existing keys and adds new ones.
#[tokio::test]
async fn reopened_section_merges_properties() {
    let ini = IniFile::parse(
        "\
[foo]
name = value
[foo]
name2 = value2
",
    );
    let foo = ini.section("foo").unwrap();
    assert_eq!(foo.get("name").unwrap(), "value");
    assert_eq!(foo.get("name2").unwrap(), "value2");
    assert_eq!(foo.len(), 2);
}

/// Sanity check that the parser composes with the resolver's region lookup
/// over the public surface: a config-file `region` under the active profile is
/// returned. Mirrors the precedence behavior backing
/// `aws-runtime`'s profile region source. Uses `MockIo` only (fully offline).
#[tokio::test]
async fn region_resolves_from_parsed_config_profile() {
    let io = MockIo::new()
        .env("HOME", "/home/u")
        .file("/home/u/.aws/config", "[default]\nregion = eu-central-1\n");
    let region = resolve_region(&io, None).await;
    assert_eq!(region.as_deref(), Some("eu-central-1"));
}

/// Mirrors the AWS_REGION-over-profile precedence: an env override wins over a
/// parsed profile `region`. Confirms the parser output is lower precedence
/// than the environment. Fully offline via `MockIo`.
#[tokio::test]
async fn env_region_overrides_parsed_profile_region() {
    let io = MockIo::new()
        .env("HOME", "/home/u")
        .env("AWS_REGION", "us-east-2")
        .file("/home/u/.aws/config", "[default]\nregion = eu-central-1\n");
    let region = resolve_region(&io, None).await;
    assert_eq!(region.as_deref(), Some("us-east-2"));
}

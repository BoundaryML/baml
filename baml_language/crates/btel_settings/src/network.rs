//! What a traced HTTP request records. A header or query value outside these
//! allowlists is stored as a hash, never as itself: requests carry API keys.

use std::{fmt::Write as _, ops::Range};

use sha2::{Digest, Sha256};

/// **Observability semantics.** A body over this size is recorded as truncated
/// instead of its content. Identical bodies share one CAS blob.
pub const BODY_CAPTURE_MAX_BYTES: usize = 8 * 1024 * 1024;

/// **Privacy boundary.** Request headers recorded as they are, by lowercase name.
pub const REQUEST_HEADERS: &[&str] = &[
    "content-type",
    "content-length",
    "accept",
    "accept-encoding",
    "user-agent",
    "anthropic-version",
    "anthropic-beta",
    "openai-beta",
    "idempotency-key",
];

/// **Privacy boundary.** Response headers recorded as they are, by lowercase name.
pub const RESPONSE_HEADERS: &[&str] = &[
    "content-type",
    "content-length",
    "content-encoding",
    "date",
    "retry-after",
    "retry-after-ms",
    "request-id",
    "x-request-id",
    "x-amzn-requestid",
    "cf-ray",
    "openai-processing-ms",
];

/// **Privacy boundary.** Response headers recorded as they are, by lowercase prefix.
pub const RESPONSE_HEADER_PREFIXES: &[&str] = &["x-ratelimit-", "anthropic-ratelimit-"];

/// **Privacy boundary.** URL query parameters whose values are recorded as they are.
pub const QUERY_PARAMETERS: &[&str] = &[];

/// A hashed value reads `sha256:` and the first hex digits of its SHA-256.
pub const HASH_PREFIX: &str = "sha256:";
pub const HASH_HEX_DIGITS: usize = 16;

/// Who reads scrubbed text, which decides both what is scrubbed and how.
///
/// - **Telemetry** is a privacy boundary: a query or header value is kept only
///   when its name is allowlisted ([`QUERY_PARAMETERS`], [`REQUEST_HEADERS`]),
///   and is hashed otherwise, so equal values can still be recognized across
///   its records.
/// - **Log** text (errors, captured requests) keeps what debugging needs: a
///   value is scrubbed only when its name is a credential's
///   ([`is_credential_name`]), and is written as [`REDACTED`], since a plain
///   hash of a guessable value can be reversed by guessing and links equal
///   values wherever the logs travel.
///
/// Both always scrub URL userinfo and fragments.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scrub {
    Telemetry,
    Log,
}

/// What [`Scrub::Log`] writes in place of a value.
pub const REDACTED: &str = "REDACTED";

impl Scrub {
    /// `value` written as this audience records a scrubbed value.
    pub fn mask(self, value: &str) -> String {
        match self {
            Scrub::Telemetry => hash(value),
            Scrub::Log => REDACTED.to_owned(),
        }
    }

    /// Whether this audience scrubs the value of query parameter `name`.
    fn scrubs_query_parameter(self, name: &str) -> bool {
        match self {
            Scrub::Telemetry => !QUERY_PARAMETERS.contains(&name),
            Scrub::Log => is_credential_name(name),
        }
    }

    /// Whether this audience scrubs the value of request header `name`.
    fn scrubs_request_header(self, name: &str) -> bool {
        match self {
            Scrub::Telemetry => !REQUEST_HEADERS.contains(&name.to_ascii_lowercase().as_str()),
            Scrub::Log => is_credential_name(name),
        }
    }
}

/// The words a credential's name has (`x-api-key`, `X-Amz-Security-Token`,
/// `client_secret`, `Helicone-Auth`, `AWSAccessKeyId`, `session_id`, …),
/// matched case-insensitively against each word of a name (see
/// [`name_words`]).
const CREDENTIAL_WORDS: &[&str] = &[
    "auth",
    "authentication",
    "authorization",
    "key",
    "keyid",
    "apikey",
    "token",
    "secret",
    "signature",
    "sig",
    "password",
    "passwd",
    "passphrase",
    "pwd",
    "credential",
    "credentials",
    "cookie",
    "session",
    "sessionid",
    "assertion",
    "jwt",
    "bearer",
    "otp",
];

/// The endings a credential's name has when its words are run together
/// (`apikey`, `subscriptionkey`, `accesstoken`, …), matched
/// case-insensitively.
const CREDENTIAL_NAME_ENDINGS: &[&str] = &[
    "key",
    "keyid",
    "token",
    "secret",
    "signature",
    "sig",
    "password",
    "passwd",
    "credential",
    "credentials",
    "authorization",
    "cookie",
];

/// Credential names that are one only as a whole: an OAuth `code=`, where
/// `error_code` and `status_code` are not.
const CREDENTIAL_NAMES: &[&str] = &["code"];

/// Whether `name` (a query parameter, header, or `name=value` in text) names
/// a credential: one of its words is a credential's, it ends as one does, or
/// it is one only as a whole (an OAuth `code`). A denylist: a credential under
/// a name it does not know is not caught, which is why telemetry uses an
/// allowlist instead.
pub fn is_credential_name(name: &str) -> bool {
    CREDENTIAL_NAMES
        .iter()
        .any(|known| name.eq_ignore_ascii_case(known))
        || is_credential_label(name)
}

/// Whether `name` reads as a credential's wherever it appears: one of its
/// words is a credential's, or it ends as one does. A `name: value` label is
/// matched by this alone, so prose such as `error code: 401` is kept.
fn is_credential_label(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    CREDENTIAL_NAME_ENDINGS
        .iter()
        .any(|ending| lower.ends_with(ending))
        || name_words(name).any(|word| {
            CREDENTIAL_WORDS
                .iter()
                .any(|known| word.eq_ignore_ascii_case(known))
        })
}

/// The words of `name`: its runs of ASCII letters and digits, each split again
/// where a lowercase letter meets an uppercase one (`AWSAccessKeyId` is
/// `AWSAccess`, `Key`, `Id`).
fn name_words(name: &str) -> impl Iterator<Item = &str> {
    name.split(|c: char| !c.is_ascii_alphanumeric())
        .flat_map(|run| {
            let mut rest = run;
            std::iter::from_fn(move || {
                // `run` is ASCII, so every byte index is a char boundary.
                let bytes = rest.as_bytes();
                let end = (1..bytes.len())
                    .find(|&i| bytes[i - 1].is_ascii_lowercase() && bytes[i].is_ascii_uppercase())
                    .unwrap_or(bytes.len());
                let (word, tail) = rest.split_at(end);
                rest = tail;
                (!word.is_empty()).then_some(word)
            })
        })
}

/// Query names stay; each nonempty value `scrub` scrubs is masked, and every
/// other value is scrubbed as text ([`sanitize_text`]), since it can hold a
/// URL or `name=value` of its own (`?redirect=https://h/?key=…`,
/// `?a=b;key=…`). Credentials before the host and a fragment are masked whole.
pub fn sanitize_url(url: &str, scrub: Scrub) -> String {
    sanitize_url_at(url, scrub, 0)
}

/// How many URLs deep, each in the query of the one around it, a value is
/// scrubbed as text. A value nested deeper is masked whole: the scrubber
/// recurses once per level, and its input can be untrusted (a response body).
const MAX_URL_NESTING: usize = 8;

/// [`sanitize_url`] for a URL `depth` URLs deep in another's query.
fn sanitize_url_at(url: &str, scrub: Scrub, depth: usize) -> String {
    let (url, fragment) = match url.split_once('#') {
        Some((url, fragment)) => (url, Some(fragment)),
        None => (url, None),
    };
    let (base, query) = match url.split_once('?') {
        Some((base, query)) => (base, Some(query)),
        None => (url, None),
    };
    let mut sanitized = String::with_capacity(url.len());
    let authority = base.find("://").map_or(0, |scheme| scheme + 3);
    let host = base[authority..]
        .find('/')
        .map_or(base.len(), |path| authority + path);
    match base[authority..host].rfind('@') {
        Some(at) => {
            sanitized.push_str(&base[..authority]);
            sanitized.push_str(&scrub.mask(&base[authority..authority + at]));
            sanitized.push_str(&base[authority + at..]);
        }
        None => sanitized.push_str(base),
    }
    if let Some(query) = query {
        sanitized.push('?');
        for (i, pair) in query.split('&').enumerate() {
            if i > 0 {
                sanitized.push('&');
            }
            match pair.split_once('=') {
                Some((name, value)) if !value.is_empty() && scrub.scrubs_query_parameter(name) => {
                    sanitized.push_str(name);
                    sanitized.push('=');
                    sanitized.push_str(&scrub.mask(value));
                }
                Some((name, value)) => {
                    sanitized.push_str(name);
                    sanitized.push('=');
                    if depth < MAX_URL_NESTING {
                        sanitized.push_str(&sanitize_text_at(value, scrub, depth + 1));
                    } else {
                        sanitized.push_str(&scrub.mask(value));
                    }
                }
                None => sanitized.push_str(pair),
            }
        }
    }
    if let Some(fragment) = fragment {
        sanitized.push('#');
        if !fragment.is_empty() {
            sanitized.push_str(&scrub.mask(fragment));
        }
    }
    sanitized
}

/// A request header's value as `scrub` records it: masked when `scrub`
/// scrubs the name, else as itself.
pub fn sanitize_request_header(name: &str, value: &str, scrub: Scrub) -> String {
    if scrub.scrubs_request_header(name) {
        scrub.mask(value)
    } else {
        value.to_owned()
    }
}

/// `text` with the credentials it may quote masked, for an error: every URL in
/// it is sanitized by [`sanitize_url`], and outside a URL the value of every
/// credential-named assignment is masked (`api_key=…`, `api_key="…"`,
/// `"api_key": "…"`, `Authorization: Bearer …`).
///
/// A URL runs from its scheme to the first whitespace, quote, or closing
/// bracket, which is how prose and `reqwest`'s `for url (…)` delimit one.
pub fn sanitize_text(text: &str, scrub: Scrub) -> String {
    sanitize_text_at(text, scrub, 0)
}

/// [`sanitize_text`] for a query value `depth` URLs deep.
fn sanitize_text_at(text: &str, scrub: Scrub, depth: usize) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(separator) = rest.find("://") {
        let start = rest[..separator]
            .trim_end_matches(|c: char| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
            .len();
        if start == separator {
            // No scheme before `://`: not a URL; keep it and look further on.
            out.push_str(&sanitize_assignments(&rest[..separator + 3], scrub));
            rest = &rest[separator + 3..];
            continue;
        }
        let end = rest[separator..]
            .find(|c: char| is_value_end(c) && c != '&' && c != ',' && c != ';')
            .map_or(rest.len(), |at| separator + at);
        out.push_str(&sanitize_assignments(&rest[..start], scrub));
        out.push_str(&sanitize_url_at(&rest[start..end], scrub, depth));
        rest = &rest[end..];
    }
    out.push_str(&sanitize_assignments(rest, scrub));
    out
}

/// `text` (with no URL in it) with the value of every credential-named
/// assignment masked: `name=value` when [`is_credential_name`] matches, and
/// `name: value` (a header line, YAML, or JSON's `"name": value`) when its
/// name reads as a credential's wherever it appears. A value may be quoted, in
/// which case what the quotes hold is masked, or open with an HTTP auth scheme
/// (`Bearer`, `Basic`, …), which is kept. Any other value is scanned on, so
/// one it holds (`a=key=…`) is found too.
///
/// Not caught: a secret without such a name (a bare `Bearer` token in
/// prose), and a name spaced from its separator (`key = …`).
fn sanitize_assignments(text: &str, scrub: Scrub) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find(['=', ':']) {
        let name = assignment_name(&rest[..at]);
        let credential = if rest.as_bytes()[at] == b'=' {
            is_credential_name(name)
        } else {
            is_credential_label(name)
        };
        out.push_str(&rest[..=at]);
        rest = &rest[at + 1..];
        // Only a credential's value is read to its end, so a run of other
        // assignments (`a=a=a=…`) is read once.
        if credential {
            let value = credential_value(rest);
            out.push_str(&rest[..value.start]);
            if !value.is_empty() {
                out.push_str(&scrub.mask(&rest[value.clone()]));
            }
            rest = &rest[value.end..];
        }
    }
    out.push_str(rest);
    out
}

/// The name an assignment's separator follows: the run of name characters
/// before it, inside quotes when it is quoted (`"api_key":`).
fn assignment_name(before: &str) -> &str {
    let unquoted = before.strip_suffix(['"', '\'']).unwrap_or(before);
    let start = unquoted
        .trim_end_matches(|c: char| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
        .len();
    &unquoted[start..]
}

/// The HTTP auth schemes a credential's value can open with, kept in front
/// of the masked part (`Authorization: Bearer …`).
const AUTH_SCHEMES: &[&str] = &["basic", "bearer", "digest", "negotiate", "token"];

/// Where the secret part of the credential value at the start of `rest` lies
/// (empty when there is none); it ends where the value does.
///
/// Spaces may follow the separator. A quoted value's secret is what the
/// quotes hold, through the closing quote's escapes; without a closing quote
/// it runs to the end of the line, so an unclosed quote hides more, not less.
/// Otherwise an auth scheme and its spaces are skipped, and the secret runs to
/// the first [`is_value_end`].
fn credential_value(rest: &str) -> Range<usize> {
    // Every slice below is a suffix of `rest`, so `rest.len() - suffix.len()`
    // is where it starts.
    let value = rest.trim_start_matches([' ', '\t']);
    let mut chars = value.chars();
    let closing = match chars.next() {
        Some(quote @ ('"' | '\'' | '`')) => Some(quote),
        Some('“') => Some('”'),
        Some('‘') => Some('’'),
        _ => None,
    };
    if let Some(closing) = closing {
        let inner = chars.as_str();
        let mut escaped = false;
        let end = inner
            .char_indices()
            .find(|&(_, c)| {
                let closes = c == closing && !escaped;
                escaped = c == '\\' && !escaped;
                closes || c == '\n'
            })
            .map_or(inner.len(), |(at, _)| at);
        let start = rest.len() - inner.len();
        return start..start + end;
    }
    let after_scheme = value
        .find(|c: char| !c.is_ascii_alphabetic())
        .filter(|&end| {
            AUTH_SCHEMES
                .iter()
                .any(|scheme| value[..end].eq_ignore_ascii_case(scheme))
                && value[end..].starts_with([' ', '\t'])
        })
        .map(|end| value[end..].trim_start_matches([' ', '\t']));
    let secret = after_scheme.unwrap_or(value);
    let start = rest.len() - secret.len();
    let end = secret.find(is_value_end).unwrap_or(secret.len());
    start..start + end
}

/// Whether `c` ends a URL or a `name=value` value inside prose.
fn is_value_end(c: char) -> bool {
    c.is_whitespace()
        || matches!(
            c,
            '"' | '\'' | '<' | '>' | ')' | ']' | '}' | '`' | '&' | ',' | ';' | '”' | '’' | '»'
        )
}

/// `sha256:` and the first hex digits of the value's plain SHA-256.
pub fn hash(value: &str) -> String {
    let digest = Sha256::digest(value.as_bytes());
    let mut hashed = String::with_capacity(HASH_PREFIX.len() + HASH_HEX_DIGITS);
    hashed.push_str(HASH_PREFIX);
    for byte in &digest[..HASH_HEX_DIGITS / 2] {
        write!(hashed, "{byte:02x}").expect("writing to a String cannot fail");
    }
    hashed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hashes_are_the_first_hex_digits_of_plain_sha256() {
        // SHA-256("abc") = ba7816bf8f01cfea414140de5dae2223...
        assert_eq!(hash("abc"), "sha256:ba7816bf8f01cfea");
        assert_eq!(hash("abc").len(), "sha256:".len() + 16);
    }

    #[test]
    fn url_query_values_and_credentials_are_hashed_and_names_kept() {
        assert_eq!(
            sanitize_url("https://api.anthropic.com/v1/messages", Scrub::Telemetry),
            "https://api.anthropic.com/v1/messages"
        );
        assert_eq!(
            sanitize_url(
                "https://generativelanguage.googleapis.com/v1beta/models/gemini:streamGenerateContent?alt=sse&key=AIza-secret",
                Scrub::Telemetry
            ),
            format!(
                "https://generativelanguage.googleapis.com/v1beta/models/gemini:streamGenerateContent?alt={}&key={}",
                hash("sse"),
                hash("AIza-secret")
            )
        );
        assert_eq!(
            sanitize_url(
                "http://user:pass@localhost:8080/a@b?flag&empty=#token",
                Scrub::Telemetry
            ),
            format!(
                "http://{}@localhost:8080/a@b?flag&empty=#{}",
                hash("user:pass"),
                hash("token")
            )
        );
    }

    #[test]
    fn credentials_in_text_are_hashed_in_place() {
        let text = "error sending request for url (https://x/v1/m?alt=sse&KEY=SEKRIT): refused";
        assert_eq!(
            sanitize_text(text, Scrub::Telemetry),
            format!(
                "error sending request for url (https://x/v1/m?alt={}&KEY={}): refused",
                hash("sse"),
                hash("SEKRIT")
            )
        );
        assert_eq!(
            sanitize_text(
                "see \"https://u:p@h/a?sig=s\" and http://h/b",
                Scrub::Telemetry
            ),
            format!(
                "see \"https://{}@h/a?sig={}\" and http://h/b",
                hash("u:p"),
                hash("s")
            )
        );
        // Outside a URL, only credential-named values are hashed.
        assert_eq!(
            sanitize_text(
                "?api_key=abc&Access_Token=def&x=1 max_tokens=5 X-Amz-Signature=q",
                Scrub::Telemetry
            ),
            format!(
                "?api_key={}&Access_Token={}&x=1 max_tokens=5 X-Amz-Signature={}",
                hash("abc"),
                hash("def"),
                hash("q")
            )
        );
        assert_eq!(
            sanitize_text("no url :// here", Scrub::Telemetry),
            "no url :// here"
        );
        assert_eq!(sanitize_text("key=", Scrub::Telemetry), "key=");
    }

    /// The same values are scrubbed whichever way they are written.
    #[test]
    fn redact_masks_what_hash_masks() {
        let text = "url (https://u:p@x/v1?key=SEKRIT#frag) and api_key=abc";
        assert_eq!(
            sanitize_text(text, Scrub::Log),
            "url (https://REDACTED@x/v1?key=REDACTED#REDACTED) and api_key=REDACTED"
        );
        assert_eq!(
            sanitize_request_header("Authorization", "Bearer a", Scrub::Log),
            REDACTED
        );
        assert_eq!(
            sanitize_request_header("Content-Type", "application/json", Scrub::Log),
            "application/json"
        );
    }

    /// Log text keeps what debugging needs: only credential-named values go,
    /// while telemetry keeps only allowlisted ones.
    #[test]
    fn log_scrubs_credential_names_and_keeps_the_rest() {
        let url = "https://x/v1/deployments/d?api-version=2024-02-01&alt=sse&key=SEKRIT&X-Amz-Signature=s";
        assert_eq!(
            sanitize_url(url, Scrub::Log),
            "https://x/v1/deployments/d?api-version=2024-02-01&alt=sse&key=REDACTED&X-Amz-Signature=REDACTED"
        );
        assert!(!sanitize_url(url, Scrub::Telemetry).contains("api-version=2024"));
        for (header, kept) in [
            ("x-goog-user-project", true),
            ("openai-organization", true),
            ("anthropic-version", true),
            ("x-api-key", false),
            ("ocp-apim-subscription-key", false),
            ("Set-Cookie", false),
            ("Proxy-Authorization", false),
            ("x-amz-security-token", false),
        ] {
            let sanitized = sanitize_request_header(header, "v", Scrub::Log);
            assert_eq!(sanitized == "v", kept, "{header}");
        }
    }

    #[test]
    fn credential_names_are_matched_by_ending_or_whole_name() {
        for name in [
            "API_KEY",
            "AWSAccessKeyId",
            "access_token",
            "client_secret",
            "sig",
            "code",
            "Auth",
        ] {
            assert!(is_credential_name(name), "{name}");
        }
        for name in [
            "alt",
            "api-version",
            "max_tokens",
            "author",
            "keyboard",
            "q",
            "error_code",
            "oauth_version",
        ] {
            assert!(!is_credential_name(name), "{name}");
        }
    }

    #[test]
    fn credential_names_are_matched_by_word() {
        for name in [
            "x-auth",
            "Helicone-Auth",
            "authentication",
            "x-access-key-id",
            "x-api-key-2",
            "access_key_id",
            "session_id",
            "client_assertion",
            "AccessKeyId",
            "x-jwt-assertion",
        ] {
            assert!(is_credential_name(name), "{name}");
        }
    }

    /// Text is scanned by character, so a multi-byte character next to a
    /// name or a scheme is kept as it is.
    #[test]
    fn non_ascii_text_is_scrubbed_in_place() {
        for (text, scrubbed) in [
            ("x²=4", "x²=4"),
            ("价格=10", "价格=10"),
            ("“key=S”", "“key=REDACTED”"),
            ("café=1 and api_key=S", "café=1 and api_key=REDACTED"),
            ("see “https://h/a?key=S”", "see “https://h/a?key=REDACTED”"),
            ("→https://h/p?key=S", "→https://h/p?key=REDACTED"),
            ("ü://x", "ü://x"),
        ] {
            assert_eq!(sanitize_text(text, Scrub::Log), scrubbed, "{text}");
        }
    }

    /// Quoted values, `name: value` labels and auth schemes: what a quote
    /// holds is masked, quotes and the scheme are kept.
    #[test]
    fn credential_assignments_in_every_form_are_masked() {
        for (text, scrubbed) in [
            (r#"api_key="sk-live" next"#, r#"api_key="REDACTED" next"#),
            ("OPENAI_API_KEY='sk-live'", "OPENAI_API_KEY='REDACTED'"),
            (
                r#"{"api_key": "sk-live", "n": 1}"#,
                r#"{"api_key": "REDACTED", "n": 1}"#,
            ),
            (r#"{"token":"a\"b" }"#, r#"{"token":"REDACTED" }"#),
            (
                "x-api-key: sk-live\nnext: 1",
                "x-api-key: REDACTED\nnext: 1",
            ),
            (
                "Authorization: Bearer sk-live",
                "Authorization: Bearer REDACTED",
            ),
            (
                r#""authorization": "Bearer sk-live""#,
                r#""authorization": "REDACTED""#,
            ),
            ("secret: “sk-live”", "secret: “REDACTED”"),
            (
                "api_key=\"sk-live and more\nnext",
                "api_key=\"REDACTED\nnext",
            ),
        ] {
            assert_eq!(sanitize_text(text, Scrub::Log), scrubbed, "{text}");
        }
    }

    /// A label is a credential's only by its words or ending: whole-name
    /// matches such as `code` are for `code=`, so prose keeps its codes.
    #[test]
    fn labels_that_are_not_credentials_are_kept() {
        for text in [
            "error code: 401",
            "status: failed at 12:30",
            "HTTP send failed: connection refused",
            "max_tokens: 5, keyboard: us",
            "code=",
        ] {
            assert_eq!(sanitize_text(text, Scrub::Log), text);
        }
        assert_eq!(sanitize_text("code=abc", Scrub::Log), "code=REDACTED");
    }

    /// URLs nested past the limit are masked whole rather than recursed into,
    /// and a run of assignments is scanned once: untrusted text bounds neither
    /// the stack nor the time.
    #[test]
    fn pathological_text_is_bounded() {
        let nested = "https://h/?a=".repeat(100_000) + "x";
        let scrubbed = sanitize_text(&nested, Scrub::Log);
        let kept = "https://h/?a=".repeat(MAX_URL_NESTING + 1);
        assert_eq!(scrubbed, format!("{kept}{REDACTED}"));
        let assignments = "a=".repeat(200_000) + "key=S";
        assert_eq!(
            sanitize_text(&assignments, Scrub::Log),
            "a=".repeat(200_000) + "key=REDACTED"
        );
    }

    /// A value can hold a credential of its own, past a separator the query
    /// does not split on or inside a URL it carries.
    #[test]
    fn credentials_inside_other_values_are_scrubbed() {
        for (text, scrubbed) in [
            ("https://h/p?a=b,key=S", "https://h/p?a=b,key=REDACTED"),
            ("https://h/p?x=1;key=S", "https://h/p?x=1;key=REDACTED"),
            (
                "https://h/p?alt=sse\\u0026key=S",
                "https://h/p?alt=sse\\u0026key=REDACTED",
            ),
            ("a=b=key=S", "a=b=key=REDACTED"),
            (
                "for url (https://proxy/x?target=https://api/v1?key=S)",
                "for url (https://proxy/x?target=https://api/v1?key=REDACTED)",
            ),
        ] {
            assert_eq!(sanitize_text(text, Scrub::Log), scrubbed, "{text}");
        }
    }

    #[test]
    fn allowlists_are_lowercase_header_names() {
        for name in REQUEST_HEADERS
            .iter()
            .chain(RESPONSE_HEADERS)
            .chain(RESPONSE_HEADER_PREFIXES)
        {
            assert_eq!(*name, name.to_ascii_lowercase());
        }
    }
}

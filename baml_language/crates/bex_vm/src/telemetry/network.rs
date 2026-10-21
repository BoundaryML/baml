//! A traced HTTP request, sanitized on the VM thread before it is captured.
//! Requests carry API keys: header values outside the allowlists, URL query
//! values and URL credentials are recorded as hashes, never as themselves.
use std::fmt::Write as _;

use btel_records::NetworkEventName;
use btel_settings::network as settings;
use sha2::{Digest, Sha256};
use sys_types::network::NetworkEventKind;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Direction {
    Request,
    Response,
}

/// Names lowercased, one entry per name: values a name repeats are joined
/// with `, `. Each value is kept when its name is allowlisted, else hashed.
pub(super) fn sanitize_headers(
    headers: &[(String, String)],
    direction: Direction,
) -> Vec<(String, String)> {
    let mut sanitized: Vec<(String, String)> = Vec::with_capacity(headers.len());
    for (name, value) in headers {
        let name = name.to_ascii_lowercase();
        let value = if allowed(&name, direction) {
            value.clone()
        } else {
            hash(value)
        };
        match sanitized.iter_mut().find(|(seen, _)| *seen == name) {
            Some((_, joined)) => {
                joined.push_str(", ");
                joined.push_str(&value);
            }
            None => sanitized.push((name, value)),
        }
    }
    sanitized
}

fn allowed(name: &str, direction: Direction) -> bool {
    match direction {
        Direction::Request => settings::REQUEST_HEADERS.contains(&name),
        Direction::Response => {
            settings::RESPONSE_HEADERS.contains(&name)
                || settings::RESPONSE_HEADER_PREFIXES
                    .iter()
                    .any(|prefix| name.starts_with(prefix))
        }
    }
}

/// Query names stay; each nonempty value is hashed unless its name is
/// allowlisted. Credentials before the host and a fragment are hashed whole.
pub(super) fn sanitize_url(url: &str) -> String {
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
            sanitized.push_str(&hash(&base[authority..authority + at]));
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
                Some((name, value))
                    if !value.is_empty() && !settings::QUERY_PARAMETERS.contains(&name) =>
                {
                    sanitized.push_str(name);
                    sanitized.push('=');
                    sanitized.push_str(&hash(value));
                }
                _ => sanitized.push_str(pair),
            }
        }
    }
    if let Some(fragment) = fragment {
        sanitized.push('#');
        if !fragment.is_empty() {
            sanitized.push_str(&hash(fragment));
        }
    }
    sanitized
}

/// What a captured error must not quote of a request's raw `url`: the URL
/// and its query string, each with its sanitized form.
pub(super) fn url_rewrites(url: &str) -> Vec<(String, String)> {
    let mut rewrites = Vec::new();
    let sanitized = sanitize_url(url);
    if sanitized != url {
        rewrites.push((url.to_owned(), sanitized));
    }
    let query = url
        .split('#')
        .next()
        .and_then(|url| url.split_once('?'))
        .map(|(_, query)| query)
        .filter(|query| !query.is_empty());
    if let Some(query) = query {
        let sanitized = sanitize_url(&format!("?{query}"));
        let sanitized = &sanitized[1..];
        if sanitized != query {
            rewrites.push((query.to_owned(), sanitized.to_owned()));
        }
    }
    rewrites
}

/// `sha256:` and the first hex digits of the value's plain SHA-256.
pub(super) fn hash(value: &str) -> String {
    let digest = Sha256::digest(value.as_bytes());
    let mut hashed = String::with_capacity(settings::HASH_PREFIX.len() + settings::HASH_HEX_DIGITS);
    hashed.push_str(settings::HASH_PREFIX);
    for byte in &digest[..settings::HASH_HEX_DIGITS / 2] {
        write!(hashed, "{byte:02x}").expect("writing to a String cannot fail");
    }
    hashed
}

pub(super) fn event_name(kind: &NetworkEventKind) -> NetworkEventName {
    match kind {
        NetworkEventKind::Connection { .. } => NetworkEventName::Connection,
        NetworkEventKind::Body(_) | NetworkEventKind::SseEvent { .. } => NetworkEventName::Data,
        NetworkEventKind::StreamEnd => NetworkEventName::End,
        NetworkEventKind::Await => NetworkEventName::Await,
        NetworkEventKind::Close => NetworkEventName::Close,
        NetworkEventKind::Drop => NetworkEventName::Drop,
    }
}

/// A network snapshot's value, built from sanitized Rust values.
pub(super) enum NetworkPayload<'a> {
    /// `{request: {method, url, headers, body}}`, without `body` when bodies
    /// are not recorded.
    Request {
        method: &'a str,
        url: &'a str,
        headers: &'a [(String, String)],
        body: Option<&'a [u8]>,
    },
    /// `{status, headers}`.
    Connection {
        status: u16,
        headers: &'a [(String, String)],
    },
    /// A whole body: a string when it is UTF-8, otherwise bytes.
    Body(&'a [u8]),
    /// `{event, data, id}`, with null for an absent event or id.
    SseEvent {
        event: Option<&'a str>,
        data: &'a str,
        id: Option<&'a str>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headers(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
            .collect()
    }

    #[test]
    fn hashes_are_the_first_hex_digits_of_plain_sha256() {
        // SHA-256("abc") = ba7816bf8f01cfea414140de5dae2223...
        assert_eq!(hash("abc"), "sha256:ba7816bf8f01cfea");
        assert_eq!(hash("abc").len(), "sha256:".len() + 16);
    }

    #[test]
    fn header_names_are_lowercased_and_only_allowlisted_values_kept() {
        let sanitized = sanitize_headers(
            &headers(&[
                ("Content-Type", "application/json"),
                ("Authorization", "Bearer sk-secret"),
                ("x-api-key", "sk-ant-secret"),
                ("Anthropic-Version", "2023-06-01"),
            ]),
            Direction::Request,
        );
        assert_eq!(
            sanitized,
            headers(&[
                ("content-type", "application/json"),
                ("authorization", &hash("Bearer sk-secret")),
                ("x-api-key", &hash("sk-ant-secret")),
                ("anthropic-version", "2023-06-01"),
            ])
        );
    }

    #[test]
    fn response_headers_use_their_own_allowlist_and_prefixes() {
        let raw = headers(&[
            ("X-RateLimit-Remaining-Requests", "99"),
            ("anthropic-ratelimit-tokens-limit", "4000"),
            ("Request-Id", "req_1"),
            ("Set-Cookie", "session=a"),
            ("set-cookie", "theme=b"),
            ("anthropic-version", "2023-06-01"),
        ]);
        assert_eq!(
            sanitize_headers(&raw, Direction::Response),
            headers(&[
                ("x-ratelimit-remaining-requests", "99"),
                ("anthropic-ratelimit-tokens-limit", "4000"),
                ("request-id", "req_1"),
                (
                    "set-cookie",
                    &format!("{}, {}", hash("session=a"), hash("theme=b"))
                ),
                // Allowlisted for requests only.
                ("anthropic-version", &hash("2023-06-01")),
            ])
        );
        // Response prefixes do not apply to requests.
        assert_eq!(
            sanitize_headers(&raw[..1], Direction::Request),
            headers(&[("x-ratelimit-remaining-requests", &hash("99"))])
        );
    }

    #[test]
    fn url_query_values_and_credentials_are_hashed_and_names_kept() {
        assert_eq!(
            sanitize_url("https://api.anthropic.com/v1/messages"),
            "https://api.anthropic.com/v1/messages"
        );
        assert_eq!(
            sanitize_url(
                "https://generativelanguage.googleapis.com/v1beta/models/gemini:streamGenerateContent?alt=sse&key=AIza-secret"
            ),
            format!(
                "https://generativelanguage.googleapis.com/v1beta/models/gemini:streamGenerateContent?alt={}&key={}",
                hash("sse"),
                hash("AIza-secret")
            )
        );
        assert_eq!(
            sanitize_url("http://user:pass@localhost:8080/a@b?flag&empty=#token"),
            format!(
                "http://{}@localhost:8080/a@b?flag&empty=#{}",
                hash("user:pass"),
                hash("token")
            )
        );
    }

    #[test]
    fn url_rewrites_cover_the_url_and_its_query_alone() {
        let url = "https://example.com/v1?key=secret&alt=json#frag";
        assert_eq!(
            url_rewrites(url),
            [
                (url.to_owned(), sanitize_url(url)),
                (
                    "key=secret&alt=json".to_owned(),
                    format!("key={}&alt={}", hash("secret"), hash("json"))
                ),
            ]
        );
        assert!(url_rewrites("https://example.com/v1").is_empty());
    }

    #[test]
    fn every_event_kind_has_its_recorded_name() {
        let names = [
            NetworkEventKind::Connection {
                status: 200,
                headers: Vec::new(),
            },
            NetworkEventKind::Body(Vec::new().into()),
            NetworkEventKind::SseEvent {
                event: None,
                data: String::new(),
                id: None,
            },
            NetworkEventKind::StreamEnd,
            NetworkEventKind::Await,
            NetworkEventKind::Close,
            NetworkEventKind::Drop,
        ]
        .iter()
        .map(|kind| event_name(kind).as_str().to_owned())
        .collect::<Vec<_>>();
        assert_eq!(
            names,
            [
                "connection",
                "data",
                "data",
                "end",
                "await",
                "close",
                "drop"
            ]
        );
    }
}

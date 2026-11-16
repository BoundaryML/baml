//! A traced HTTP request, sanitized on the VM thread before it is captured.
//! Requests carry API keys: header values outside the allowlists, URL query
//! values and URL credentials are recorded as hashes, never as themselves.
use btel_records::NetworkEventName;
pub(super) use btel_settings::network::hash;
use btel_settings::{network as settings, network::Scrub};
use sys_types::network::NetworkEventKind;

/// A traced request's URL, sanitized: telemetry hashes, so equal values can be
/// recognized across its records.
pub(super) fn sanitize_url(url: &str) -> String {
    settings::sanitize_url(url, Scrub::Telemetry)
}

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

/// What a captured error must not quote of a request's raw `url`: the URL
/// and its query string, each with its sanitized form. Each is rewritten both
/// as sent and as scrubbed for logs ([`Scrub::Log`]), which is how the native
/// transport's errors quote it: telemetry records the hashed form either way.
pub(super) fn url_rewrites(url: &str) -> Vec<(String, String)> {
    let sanitized = sanitize_url(url);
    let logged = settings::sanitize_url(url, Scrub::Log);
    let sanitized_query = query(url).map(|raw| {
        let sanitized = sanitize_url(&format!("?{raw}"));
        sanitized[1..].to_owned()
    });
    let mut rewrites: Vec<(String, String)> = Vec::new();
    for form in [url, logged.as_str()] {
        if form != sanitized && !rewrites.iter().any(|(from, _)| from == form) {
            rewrites.push((form.to_owned(), sanitized.clone()));
        }
        if let (Some(form_query), Some(sanitized_query)) = (query(form), &sanitized_query)
            && form_query != sanitized_query
            && !rewrites.iter().any(|(from, _)| from == form_query)
        {
            rewrites.push((form_query.to_owned(), sanitized_query.clone()));
        }
    }
    rewrites
}

/// `url`'s query string, without the `?` or a fragment; `None` when empty.
fn query(url: &str) -> Option<&str> {
    url.split('#')
        .next()
        .and_then(|url| url.split_once('?'))
        .map(|(_, query)| query)
        .filter(|query| !query.is_empty())
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
    fn url_rewrites_cover_the_url_and_its_query_alone() {
        let url = "https://example.com/v1?key=secret&alt=json#frag";
        let query = format!("key={}&alt={}", hash("secret"), hash("json"));
        assert_eq!(
            url_rewrites(url),
            [
                (url.to_owned(), sanitize_url(url)),
                ("key=secret&alt=json".to_owned(), query.clone()),
                // As the native transport quotes it, scrubbed for logs.
                (
                    "https://example.com/v1?key=REDACTED&alt=json#REDACTED".to_owned(),
                    sanitize_url(url)
                ),
                ("key=REDACTED&alt=json".to_owned(), query),
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

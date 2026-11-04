//! Host-identifier casing and per-scope name allocation, shared by the
//! generators that honor `naming_convention = "language"`.
//!
//! BAML names stay the wire identity: a generator that respells a name keeps
//! the BAML spelling for the runtime (a field alias, a keyword argument
//! mapping, a string literal in the codec) and only the host-facing binding
//! changes.

use std::{
    borrow::Cow,
    collections::{HashMap, HashSet},
};

use crate::NamingConvention;

/// The casing a host identifier takes under
/// [`NamingConvention::Language`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Case {
    /// `next_cursor`: functions, methods, modules, fields, parameters.
    Snake,
    /// `PageResult`: types, and enum variants where the host requires it.
    UpperCamel,
}

impl Case {
    /// Whether `name` already follows this case, by the rules rustc's
    /// `non_snake_case` and `non_camel_case_types` lints apply. Acronyms are
    /// accepted as written (`LLMClient`, `POSITIVE`), so idiomatic BAML never
    /// changes spelling. Names with characters outside identifiers (`@`)
    /// never qualify.
    pub fn admits(self, name: &str) -> bool {
        if !name.chars().all(|ch| ch == '_' || ch.is_alphanumeric()) {
            return false;
        }
        let core = name.trim_matches('_');
        if core.contains("__") {
            return false;
        }
        match self {
            Self::Snake => !core.chars().any(char::is_uppercase),
            Self::UpperCamel => {
                let chars: Vec<char> = core.chars().collect();
                chars.first().is_none_or(|first| !first.is_lowercase())
                    && !chars.windows(2).any(|pair| {
                        let [left, right] = [pair[0], pair[1]];
                        has_case(left) && right == '_' || left == '_' && has_case(right)
                    })
            }
        }
    }

    /// `name` respelled in this case. A name this case
    /// [admits](Self::admits) is returned unchanged. Otherwise the name is
    /// split into words (at non-alphanumeric characters, lower-to-upper and
    /// digit-to-upper transitions, and the end of an acronym: `HTTPServer` is
    /// `HTTP` + `Server`) and the words are joined in this case. A known word
    /// with a capital inside (`OAuth`, `GraphQL`, `iOS`, `macOS`, `GitHub`,
    /// `JavaScript`, `TypeScript`, `OpenAI`, `YouTube`) stays one word and
    /// keeps its canonical spelling in [`Case::UpperCamel`]. Leading
    /// underscores are kept; a name with no words is returned unchanged.
    pub fn apply(self, name: &str) -> Cow<'_, str> {
        if self.admits(name) {
            return Cow::Borrowed(name);
        }
        let body = name.trim_start_matches('_');
        let prefix = &name[..name.len() - body.len()];
        let words = words(body);
        if words.is_empty() {
            return Cow::Borrowed(name);
        }
        let mut out = String::with_capacity(name.len() + words.len());
        out.push_str(prefix);
        for (index, word) in words.iter().enumerate() {
            match self {
                Self::Snake => {
                    if index > 0 {
                        out.push('_');
                    }
                    out.extend(word.chars().flat_map(char::to_lowercase));
                }
                Self::UpperCamel => {
                    let canonical = known_word(word);
                    let mut chars = canonical.unwrap_or(word).chars();
                    if let Some(first) = chars.next() {
                        out.extend(first.to_uppercase());
                        match canonical {
                            Some(_) => out.extend(chars),
                            None => out.extend(chars.flat_map(char::to_lowercase)),
                        }
                    }
                }
            }
        }
        Cow::Owned(out)
    }
}

impl NamingConvention {
    /// The host spelling of `name` for a binding that takes `case` under
    /// [`NamingConvention::Language`]; `name` itself under
    /// [`NamingConvention::PreserveCase`].
    pub fn spell(self, name: &str, case: Case) -> Cow<'_, str> {
        match self {
            Self::PreserveCase => Cow::Borrowed(name),
            Self::Language => case.apply(name),
        }
    }
}

/// One name requesting a spelling in a scope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate<K> {
    /// The caller's handle for the allocated spelling.
    pub key: K,
    /// The spelling the name would take with no other name in the scope.
    pub spelling: String,
    /// Whether `spelling` is exactly what the user wrote. Authored spellings
    /// are placed first, so a `next_cursor` keeps its name and a `nextCursor`
    /// that only projects onto it is the one renamed.
    pub authored: bool,
}

/// Give every candidate in one scope a distinct spelling, none of them in
/// `reserved`. A spelling already taken gets trailing underscores until it is
/// free. Authored spellings are placed before projected ones; otherwise the
/// input order decides, so callers pass candidates in a deterministic order.
pub fn allocate_distinct<'r, K: Eq + std::hash::Hash>(
    candidates: Vec<Candidate<K>>,
    reserved: impl IntoIterator<Item = &'r str>,
) -> HashMap<K, String> {
    let mut used: HashSet<String> = reserved.into_iter().map(str::to_string).collect();
    let (authored, projected): (Vec<_>, Vec<_>) = candidates
        .into_iter()
        .partition(|candidate| candidate.authored);
    authored
        .into_iter()
        .chain(projected)
        .map(|candidate| {
            let spelling = allocate_one(&candidate.spelling, &mut used);
            (candidate.key, spelling)
        })
        .collect()
}

/// `candidate`, or `candidate` followed by as many underscores as it takes to
/// leave `used`; the result is added to `used`.
pub fn allocate_one(candidate: &str, used: &mut HashSet<String>) -> String {
    let mut spelling = candidate.to_string();
    while used.contains(&spelling) {
        spelling.push('_');
    }
    used.insert(spelling.clone());
    spelling
}

fn has_case(ch: char) -> bool {
    ch.is_lowercase() || ch.is_uppercase()
}

/// Words with a capital inside that are never split, in their canonical
/// spelling. Each is recognized case-insensitively wherever it stands as a
/// whole word of an identifier (see [`words`]): `getGcpOAuthProvider` is
/// `get` + `Gcp` + `OAuth` + `Provider`, not `get` + `Gcp` + `O` + `Auth` +
/// `Provider`.
const KNOWN_WORDS: &[&str] = &[
    "OAuth",
    "GraphQL",
    "iOS",
    "macOS",
    "GitHub",
    "JavaScript",
    "TypeScript",
    "OpenAI",
    "YouTube",
];

/// The canonical spelling of `word` if it is a [known word](KNOWN_WORDS).
fn known_word(word: &str) -> Option<&'static str> {
    KNOWN_WORDS
        .iter()
        .copied()
        .find(|known| known.eq_ignore_ascii_case(word))
}

/// The length in chars of the longest [known word](KNOWN_WORDS) that `chars`
/// starts with and that ends where a word can end: at the end of `chars`,
/// before a digit, or before an uppercase letter. When the match is written
/// in capitals only, that uppercase letter must start a capitalized word
/// (`OAUTHToken`), so an all-caps run that merely begins with the letters
/// (`IOSTAT`, `OPENAIR`) stays whole. `exact` asks for the canonical spelling
/// instead of a case-insensitive match.
fn known_word_at(chars: &[char], exact: bool) -> Option<usize> {
    KNOWN_WORDS
        .iter()
        .filter_map(|known| {
            let len = known.chars().count();
            let candidate = chars.get(..len)?;
            let matches = candidate.iter().zip(known.chars()).all(|(&ch, k)| {
                if exact {
                    ch == k
                } else {
                    ch.eq_ignore_ascii_case(&k)
                }
            });
            let all_caps = !candidate.iter().any(|ch| ch.is_lowercase());
            let ends_word = match chars.get(len) {
                None => true,
                Some(next) if next.is_lowercase() => false,
                Some(next) if all_caps && next.is_uppercase() => {
                    chars.get(len + 1).is_some_and(|after| after.is_lowercase())
                }
                Some(_) => true,
            };
            (matches && ends_word).then_some(len)
        })
        .max()
}

/// The words of `name`: maximal alphanumeric runs, each split before an
/// uppercase letter that follows a lowercase letter or digit (`nextCursor`,
/// `v2Api`) and before the last letter of an acronym followed by lowercase
/// (`HTTPServer`).
///
/// A [known word](KNOWN_WORDS) is never split. It is recognized where a word
/// starts and when the next character does not continue it in lowercase, so
/// `getGitHubRepos` is `get` + `GitHub` + `Repos` while `IOStream` stays
/// `IO` + `Stream`. Inside an acronym it is recognized only in its canonical
/// spelling (`HTTPOAuth` is `HTTP` + `OAuth`; `BIOS` stays whole). Digits
/// right after it stay attached, as they do after any word (`OAuth2Token` is
/// `OAuth2` + `Token`).
fn words(name: &str) -> Vec<&str> {
    let mut words = Vec::new();
    for run in name
        .split(|ch: char| !ch.is_alphanumeric())
        .filter(|run| !run.is_empty())
    {
        let offsets: Vec<usize> = run.char_indices().map(|(offset, _)| offset).collect();
        let chars: Vec<char> = run.chars().collect();
        let mut start = 0;
        // Where the known word last read ends; no boundary falls inside it.
        let mut known_end = known_word_at(&chars, false).unwrap_or(0);
        for index in 1..chars.len() {
            if index < known_end {
                continue;
            }
            let [previous, ch] = [chars[index - 1], chars[index]];
            let next = chars.get(index + 1).copied();
            let mut boundary = ch.is_uppercase()
                && (previous.is_lowercase()
                    || previous.is_numeric()
                    || previous.is_uppercase() && next.is_some_and(char::is_lowercase)
                    || index == known_end);
            let known = if boundary {
                known_word_at(&chars[index..], false)
            } else if ch.is_uppercase() && previous.is_uppercase() {
                known_word_at(&chars[index..], true)
            } else {
                None
            };
            if let Some(len) = known {
                boundary = true;
                known_end = index + len;
            }
            if boundary {
                words.push(&run[start..offsets[index]]);
                start = offsets[index];
            }
        }
        words.push(&run[start..]);
    }
    words
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snake_case_respells_only_names_that_need_it() {
        for (raw, expected) in [
            ("next_cursor", "next_cursor"),
            ("nextCursor", "next_cursor"),
            ("NextCursor", "next_cursor"),
            ("deploymentGroups", "deployment_groups"),
            ("HTTPServer", "http_server"),
            ("getURL", "get_url"),
            ("v2Api", "v2_api"),
            ("utf8", "utf8"),
            ("_privateField", "_private_field"),
            ("__dunder", "__dunder"),
            ("a__b", "a_b"),
            ("extract@stream", "extract_stream"),
            ("Extract@stream", "extract_stream"),
            ("ClassifySentiment", "classify_sentiment"),
            ("_", "_"),
        ] {
            assert_eq!(Case::Snake.apply(raw), expected, "{raw}");
        }
    }

    #[test]
    fn upper_camel_case_keeps_acronyms_it_admits() {
        for (raw, expected) in [
            ("PageResult", "PageResult"),
            ("page_result", "PageResult"),
            ("pageResult", "PageResult"),
            ("LLMClient", "LLMClient"),
            ("POSITIVE", "POSITIVE"),
            ("NOT_FOUND", "NotFound"),
            ("HTTP_request", "HttpRequest"),
            ("_Hidden", "_Hidden"),
            ("foo_2", "Foo2"),
        ] {
            assert_eq!(Case::UpperCamel.apply(raw), expected, "{raw}");
        }
    }

    #[test]
    fn known_words_stay_whole_in_snake_case() {
        for (raw, expected) in [
            // Each known word at the start, middle and end of a name.
            ("OAuthToken", "oauth_token"),
            ("getGcpOAuthProvider", "get_gcp_oauth_provider"),
            ("refreshOAuth", "refresh_oauth"),
            ("GraphQLSchema", "graphql_schema"),
            ("runGraphQLQuery", "run_graphql_query"),
            ("viaGraphQL", "via_graphql"),
            ("iOSVersion", "ios_version"),
            ("build_iOS_app", "build_ios_app"),
            ("targetIOS", "target_ios"),
            ("macOSVersion", "macos_version"),
            ("getMacOSVersion", "get_macos_version"),
            ("onMacOS", "on_macos"),
            ("GitHubRepo", "github_repo"),
            ("getGitHubRepos", "get_github_repos"),
            ("fromGitHub", "from_github"),
            ("JavaScriptFile", "javascript_file"),
            ("toJavaScriptValue", "to_javascript_value"),
            ("asJavaScript", "as_javascript"),
            ("TypeScriptConfig", "typescript_config"),
            ("emitTypeScriptTypes", "emit_typescript_types"),
            ("toTypeScript", "to_typescript"),
            ("OpenAIClient", "openai_client"),
            ("newOpenAIClient", "new_openai_client"),
            ("useOpenAI", "use_openai"),
            ("YouTubeVideo", "youtube_video"),
            ("listYouTubeVideos", "list_youtube_videos"),
            ("fromYouTube", "from_youtube"),
            // Next to acronyms, on either side.
            ("OAuthHTTPClient", "oauth_http_client"),
            ("HTTPOAuthClient", "http_oauth_client"),
            ("GCPOAuthProvider", "gcp_oauth_provider"),
            ("getOpenAIAPIKey", "get_openai_api_key"),
            ("GraphQLAPI", "graphql_api"),
            ("iOSSDKVersion", "ios_sdk_version"),
            // Any casing of the word, and digits after it.
            ("OAUTHToken", "oauth_token"),
            ("GithubClient", "github_client"),
            ("Oauth_token", "oauth_token"),
            ("OAuth2Token", "oauth2_token"),
            // The letters without word boundaries split as before.
            ("IOStream", "io_stream"),
            ("BIOSVersion", "bios_version"),
            ("KioskMode", "kiosk_mode"),
            ("OAuthorize", "o_authorize"),
            ("IOSTAT", "iostat"),
            ("OPENAIR", "openair"),
            ("readIOSTATValue", "read_iostat_value"),
            ("GITHUBAPI", "githubapi"),
            ("myoauthToken", "myoauth_token"),
            ("GitHubber", "git_hubber"),
        ] {
            assert_eq!(Case::Snake.apply(raw), expected, "{raw}");
        }
    }

    #[test]
    fn known_words_keep_their_spelling_in_upper_camel_case() {
        for (raw, expected) in [
            ("oauth_token", "OAuthToken"),
            ("gcp_oauth_provider", "GcpOAuthProvider"),
            ("graphql_schema", "GraphQLSchema"),
            ("ios_version", "IOSVersion"),
            ("macos_version", "MacOSVersion"),
            ("github_repo", "GitHubRepo"),
            ("javascript_file", "JavaScriptFile"),
            ("typescript_config", "TypeScriptConfig"),
            ("openai_client", "OpenAIClient"),
            ("youtube_video", "YouTubeVideo"),
            ("OAUTH_TOKEN", "OAuthToken"),
            ("oAuthToken", "OAuthToken"),
            // Names the case already admits keep their spelling.
            ("OauthToken", "OauthToken"),
            ("GithubRepo", "GithubRepo"),
            ("OAuthToken", "OAuthToken"),
        ] {
            assert_eq!(Case::UpperCamel.apply(raw), expected, "{raw}");
        }
    }

    #[test]
    fn known_words_do_not_change_admitted_snake_case() {
        for raw in ["oauth_token", "o_auth_token", "github", "git_hub"] {
            assert_eq!(Case::Snake.apply(raw), raw, "{raw}");
        }
    }

    #[test]
    fn preserve_case_never_respells() {
        assert_eq!(
            NamingConvention::PreserveCase.spell("nextCursor", Case::Snake),
            "nextCursor"
        );
        assert_eq!(
            NamingConvention::Language.spell("nextCursor", Case::Snake),
            "next_cursor"
        );
    }

    #[test]
    fn authored_spelling_wins_a_projected_collision() {
        let allocated = allocate_distinct(
            vec![
                Candidate {
                    key: "nextCursor",
                    spelling: "next_cursor".to_string(),
                    authored: false,
                },
                Candidate {
                    key: "next_cursor",
                    spelling: "next_cursor".to_string(),
                    authored: true,
                },
                Candidate {
                    key: "baml",
                    spelling: "baml".to_string(),
                    authored: true,
                },
            ],
            ["baml"],
        );
        assert_eq!(allocated["next_cursor"], "next_cursor");
        assert_eq!(allocated["nextCursor"], "next_cursor_");
        assert_eq!(allocated["baml"], "baml_");
    }
}

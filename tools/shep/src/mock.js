export const reviewers = {
  hellovai: { initials: 'HV', color: '#dca64e', role: 'Super admin' },
  aaronvg: { initials: 'AV', color: '#86a4dd', role: 'Code owner' },
  '2kai2kai2': { initials: 'KK', color: '#ab99cc', role: 'Code owner' },
};

export const ownership = `# CodeTurtle · ownership from the target branch

/baml_language/crates/baml_builtins2/baml_std/ @hellovai @aaronvg @2kai2kai2

/baml_language/ENV_VARS.md @hellovai @aaronvg # codeturtle: all
/baml_language/crates/baml_env/ @hellovai @aaronvg # codeturtle: all

/.github/codeturtle/ @hellovai @aaronvg # codeturtle: all
* @hellovai @aaronvg # codeturtle: all, when=checks.environment.adds_environment_read
`;

export const files = [
  {
    id: 'env', name: 'lib.rs', directory: 'baml_language/crates/baml_env/src', language: 'rust',
    owners: ['hellovai', 'aaronvg'], all: true, rule: '/baml_language/crates/baml_env/', ruleLine: 6, additions: 14, deletions: 2,
    original: `//! Shared rules for BAML environment variables.

use std::env;

pub fn raw_var(name: &str) -> Option<String> {
    env::var(name).ok()
}

pub fn request_timeout() -> u64 {
    30_000
}

#[cfg(test)]
mod tests {
    #[test]
    fn timeout_has_a_default() {
        assert_eq!(super::request_timeout(), 30_000);
    }
}
`,
    modified: `//! Shared rules for BAML environment variables.

use std::env;

pub fn raw_var(name: &str) -> Option<String> {
    env::var(name).ok()
}

pub fn request_timeout() -> Result<u64, String> {
    match raw_var("BAML_REQUEST_TIMEOUT") {
        Some(value) => value.parse::<u64>()
            .map_err(|_| "BAML_REQUEST_TIMEOUT must be a number".into()),
        None => Ok(30_000),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn timeout_has_a_default() {
        assert_eq!(super::request_timeout().unwrap(), 30_000);
    }

    #[test]
    fn invalid_timeout_is_an_error() {
        assert!("not-a-number".parse::<u64>().is_err());
    }
}
`,
  },
  {
    id: 'docs', name: 'ENV_VARS.md', directory: 'baml_language', language: 'markdown',
    owners: ['hellovai', 'aaronvg'], all: true, rule: '/baml_language/ENV_VARS.md', ruleLine: 5, additions: 3, deletions: 0,
    original: `# Environment variables

## Naming

Use BAML_* for variables that users set.
Prefer a CLI flag where possible.

## Variables

| Variable | What it does |
|---|---|
| BAML_LOG | Runtime log level. |
| BAML_TELEMETRY | Runtime tracing level. |
`,
    modified: `# Environment variables

## Naming

Use BAML_* for variables that users set.
Prefer a CLI flag where possible.

## Variables

| Variable | What it does |
|---|---|
| BAML_LOG | Runtime log level. |
| BAML_TELEMETRY | Runtime tracing level. |
| BAML_REQUEST_TIMEOUT | Request timeout in milliseconds. Default: 30000. |

An invalid timeout is an error, never a silent fallback.
`,
  },
  {
    id: 'http', name: 'http.baml', directory: 'baml_language/crates/baml_builtins2/baml_std/baml/ns_http', language: 'baml',
    owners: ['hellovai', 'aaronvg', '2kai2kai2'], all: false, rule: '/baml_language/crates/baml_builtins2/baml_std/', ruleLine: 3, additions: 2, deletions: 1,
    original: `class Request {
  url string
  method string @default("GET")
  timeout int @default(30000)
}

class Response {
  status int
  body string
}
`,
    modified: `class Request {
  url string
  method string @default("GET")
  timeout int?
  retries int @default(2)
}

class Response {
  status int
  body string
}
`,
  },
  {
    id: 'changelog', name: 'CHANGELOG.md', directory: 'docs', language: 'markdown',
    owners: [], all: false, rule: null, additions: 2, deletions: 0,
    original: '# Changelog\n\n## Unreleased\n',
    modified: '# Changelog\n\n## Unreleased\n\nRequests support a configurable timeout.\n',
  },
];

export function initialState() {
  return {
    user: 'hellovai', selected: 'env', showAll: false, tab: 'review', split: false, commit: 1,
    decisions: [], pendingDecisions: [], drafts: [], comments: [], overrides: [], revision: { env: 1, docs: 1, http: 1, changelog: 1 },
    activity: [{ text: 'CodeTurtle loaded ownership rules from .github/codeturtle/OWNERS.', at: '11:03 AM' }],
  };
}

export function fileStatus(file, state) {
  const latest = [...new Map(state.decisions.filter(v => v.file === file.id).map(v => [v.user, v])).values()];
  const current = latest.filter(v => v.revision === state.revision[file.id]);
  const activeBlocks = latest.filter(v => v.kind === 'changes' && !state.overrides.some(o => o.block === v.id));
  const approvals = new Set(current.filter(v => v.kind === 'approve').map(v => v.user));
  const approved = file.owners.length && (file.all ? file.owners.every(user => approvals.has(user)) : approvals.size > 0);
  return { state: activeBlocks.length ? 'blocked' : approved ? 'approved' : file.owners.length ? 'pending' : 'unowned', activeBlocks, approvals };
}

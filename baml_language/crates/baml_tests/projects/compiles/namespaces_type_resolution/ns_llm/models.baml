// ═══════════════════════════════════════════════════════════════════
// LLM NAMESPACE — defines Config, Error, Status, ConfigAlias
//
// These have the SAME names as root types. Bare references here
// must resolve to the LLM versions, NOT root versions.
// ═══════════════════════════════════════════════════════════════════

class Config {
    model string
    temperature float
}

class Error {
    reason string
    retryable bool
}

enum Status {
    Ready
    Generating
    Done
}

enum Model {
    GPT4
    Claude
}

class Response {
    text string
    tokens int
}

type ConfigAlias = Config

type ResponseAlias = Response

// ── Bare name usage (must resolve to llm.* NOT root.*) ──────────

// Bare Config param: must be llm.Config (has .model), not root.Config (has .key)
function LlmIdentity(cfg: Config) -> Config {
    cfg
}

// Bare constructor: must build llm.Config
function LlmConstruct(model: string) -> Config {
    Config { model: model, temperature: 0.7 }
}

// Bare enum return: must be llm.Status.Ready, not root.Status.Ok
function LlmEnumReturn() -> Status {
    Status.Ready
}

// Bare enum match: must match against llm.Status variants
function LlmEnumMatch(s: Status) -> string {
    match (s) {
        Status.Ready => "ready",
        Status.Generating => "generating",
        Status.Done => "done"
    }
}

// Bare alias: must be llm.ConfigAlias = llm.Config
function LlmAliasParam(cfg: ConfigAlias) -> string {
    cfg.model
}

// Bare class in throw
function LlmThrowClass(x: int) -> string {
    match (x) {
        0 => throw Error { reason: "llm failed", retryable: true },
        _ => "ok"
    }
}

// Bare enum in throw
function LlmThrowEnum(x: int) -> string {
    match (x) {
        0 => throw Status.Done,
        _ => "ok"
    }
}

// Bare constructor with field access
function LlmResponseConstruct(text: string) -> Response {
    Response { text: text, tokens: 42 }
}

// ── Cross-namespace: root.X (must get root types, not llm) ──────

// root.Config has .key (not .model) — proves it's root.Config
function LlmUsesRootConfig(cfg: root.Config) -> string {
    cfg.key
}

// root.Error has .code (not .reason) — proves it's root.Error
function LlmUsesRootError(e: root.Error) -> string {
    e.message
}

// root.Status has Ok/Failed/Pending, not Ready/Generating/Done
function LlmUsesRootStatus() -> root.Status {
    root.Status.Ok
}

// root.Status match — must match root variants
function LlmMatchesRootStatus(s: root.Status) -> string {
    match (s) {
        root.Status.Ok => "ok",
        root.Status.Failed => "failed",
        root.Status.Pending => "pending"
    }
}

// root.ConfigAlias must be root.ConfigAlias = root.Config
function LlmUsesRootAlias(cfg: root.ConfigAlias) -> string {
    cfg.key
}

// Throw root.Error from llm namespace
function LlmThrowsRootError(x: int) -> string {
    match (x) {
        0 => throw root.Error { code: 404, message: "not found" },
        _ => "ok"
    }
}

// Throw root enum variant
function LlmThrowsRootEnum(x: int) -> string {
    match (x) {
        0 => throw root.Status.Failed,
        _ => "ok"
    }
}

// ── Cross-namespace: sibling namespace (root.auth.X) ────────────

// Reference ns_auth.Config (has .provider) from ns_llm
function LlmUsesAuthConfig(cfg: root.auth.Config) -> string {
    cfg.provider
}

// Reference ns_auth.Error (has .realm) from ns_llm
function LlmUsesAuthError(e: root.auth.Error) -> string {
    e.realm
}

// Reference ns_auth.Role enum from ns_llm
function LlmUsesAuthRole() -> root.auth.Role {
    root.auth.Role.Admin
}

// ── Cross-namespace: child namespace (root.llm.openai.X) ────────

// Reference nested openai.Client from parent llm namespace
function LlmUsesNestedClient(c: root.llm.openai.Client) -> string {
    c.api_key
}

// ── Mixed: llm types + qualified root/auth types ────────────────

// All three Configs in one signature — must be three different types
function LlmMixedConfigs(
    llm_cfg: Config,
    root_cfg: root.Config,
    auth_cfg: root.auth.Config
) -> string {
    llm_cfg.model
}

// ── Stream companion types ──────────────────────────────────────
// Classes with cross-namespace field types.
// The $stream companions must resolve all field types, including
// types from root and sibling namespaces.

// llm class with fields from root and sibling namespaces
class LlmWithCrossNsFields {
    own_config Config
    own_response Response
    root_config root.Config
    root_error root.Error
    auth_config root.auth.Config
    nested_client root.llm.openai.Client
    own_status Status
    own_alias ConfigAlias
    root_alias root.ConfigAlias
}

// llm class referencing root type alias (this was the alias stream bug)
class LlmWithRootAlias {
    root_alias root.ConfigAlias
    own_alias ResponseAlias
}

// ═══════════════════════════════════════════════════════════════════
// LLM.OPENAI NAMESPACE (nested) — defines Client, Error, Model
//
// Error and Model have the SAME names as root, llm, and auth types.
// Bare references here must resolve to the openai versions.
// ═══════════════════════════════════════════════════════════════════

class Client {
    api_key string
    org_id string
}

class Error {
    status_code int
    body string
}

enum Model {
    GPT4o
    GPT4oMini
    O3
}

// ── Bare name usage (must resolve to llm.openai.*) ──────────────

function OpenAIIdentity(c: Client) -> Client {
    c
}

function OpenAIConstruct(key: string) -> Client {
    Client { api_key: key, org_id: "org-123" }
}

function OpenAIEnumReturn() -> Model {
    Model.GPT4o
}

function OpenAIEnumMatch(m: Model) -> string {
    match (m) {
        Model.GPT4o => "gpt4o",
        Model.GPT4oMini => "gpt4o-mini",
        Model.O3 => "o3"
    }
}

function OpenAIThrowClass(x: int) -> string {
    match (x) {
        0 => throw Error { status_code: 429, body: "rate limited" },
        _ => "ok"
    }
}

function OpenAIThrowEnum(x: int) -> string {
    match (x) {
        0 => throw Model.O3,
        _ => "ok"
    }
}

// ── Cross-namespace: root.fixtures.namespaces_type_resolution.X ─────────────────────────────────────

function OpenAIUsesRootConfig(cfg: root.fixtures.namespaces_type_resolution.Config) -> string {
    cfg.key
}

function OpenAIUsesRootStatus() -> root.fixtures.namespaces_type_resolution.Status {
    root.fixtures.namespaces_type_resolution.Status.Ok
}

function OpenAIMatchesRootStatus(s: root.fixtures.namespaces_type_resolution.Status) -> string {
    match (s) {
        root.fixtures.namespaces_type_resolution.Status.Ok => "ok",
        root.fixtures.namespaces_type_resolution.Status.Failed => "failed",
        root.fixtures.namespaces_type_resolution.Status.Pending => "pending"
    }
}

// ── Cross-namespace: parent (root.fixtures.namespaces_type_resolution.llm.X) ────────────────────────

// Parent llm namespace — NOT the same as bare names here
function OpenAIUsesLlmConfig(cfg: root.fixtures.namespaces_type_resolution.llm.Config) -> string {
    cfg.model
}

function OpenAIUsesLlmResponse(r: root.fixtures.namespaces_type_resolution.llm.Response) -> string {
    r.text
}

function OpenAIUsesLlmStatus() -> root.fixtures.namespaces_type_resolution.llm.Status {
    root.fixtures.namespaces_type_resolution.llm.Status.Generating
}

function OpenAIMatchesLlmStatus(s: root.fixtures.namespaces_type_resolution.llm.Status) -> string {
    match (s) {
        root.fixtures.namespaces_type_resolution.llm.Status.Ready => "ready",
        root.fixtures.namespaces_type_resolution.llm.Status.Generating => "generating",
        root.fixtures.namespaces_type_resolution.llm.Status.Done => "done"
    }
}

// ── Cross-namespace: sibling of parent (root.fixtures.namespaces_type_resolution.auth.X) ────────────

function OpenAIUsesAuthConfig(cfg: root.fixtures.namespaces_type_resolution.auth.Config) -> string {
    cfg.provider
}

function OpenAIUsesAuthRole() -> root.fixtures.namespaces_type_resolution.auth.Role {
    root.fixtures.namespaces_type_resolution.auth.Role.User
}

function OpenAIMatchesAuthRole(r: root.fixtures.namespaces_type_resolution.auth.Role) -> string {
    match (r) {
        root.fixtures.namespaces_type_resolution.auth.Role.Admin => "admin",
        root.fixtures.namespaces_type_resolution.auth.Role.User => "user",
        root.fixtures.namespaces_type_resolution.auth.Role.Guest => "guest"
    }
}

// ── Mixed: all four namespace Errors ────────────────────────────

// openai.Error (.status_code), root.fixtures.namespaces_type_resolution.Error (.code),
// llm.Error (.reason), auth.Error (.realm)
function OpenAIMixedErrors(
    openai_err: Error,
    root_err: root.fixtures.namespaces_type_resolution.Error,
    llm_err: root.fixtures.namespaces_type_resolution.llm.Error,
    auth_err: root.fixtures.namespaces_type_resolution.auth.Error
) -> string {
    openai_err.body
}

// openai.Model, llm.Model — both named Model but different variants
function OpenAIMixedModels(
    openai_model: Model,
    llm_model: root.fixtures.namespaces_type_resolution.llm.Model
) -> string {
    match (openai_model) {
        Model.GPT4o => "gpt4o",
        Model.GPT4oMini => "mini",
        Model.O3 => "o3"
    }
}

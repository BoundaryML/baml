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

// ── Cross-namespace: root.X ─────────────────────────────────────

function OpenAIUsesRootConfig(cfg: root.Config) -> string {
    cfg.key
}

function OpenAIUsesRootStatus() -> root.Status {
    root.Status.Ok
}

function OpenAIMatchesRootStatus(s: root.Status) -> string {
    match (s) {
        root.Status.Ok => "ok",
        root.Status.Failed => "failed",
        root.Status.Pending => "pending"
    }
}

// ── Cross-namespace: parent (root.llm.X) ────────────────────────

// Parent llm namespace — NOT the same as bare names here
function OpenAIUsesLlmConfig(cfg: root.llm.Config) -> string {
    cfg.model
}

function OpenAIUsesLlmResponse(r: root.llm.Response) -> string {
    r.text
}

function OpenAIUsesLlmStatus() -> root.llm.Status {
    root.llm.Status.Generating
}

function OpenAIMatchesLlmStatus(s: root.llm.Status) -> string {
    match (s) {
        root.llm.Status.Ready => "ready",
        root.llm.Status.Generating => "generating",
        root.llm.Status.Done => "done"
    }
}

// ── Cross-namespace: sibling of parent (root.auth.X) ────────────

function OpenAIUsesAuthConfig(cfg: root.auth.Config) -> string {
    cfg.provider
}

function OpenAIUsesAuthRole() -> root.auth.Role {
    root.auth.Role.User
}

function OpenAIMatchesAuthRole(r: root.auth.Role) -> string {
    match (r) {
        root.auth.Role.Admin => "admin",
        root.auth.Role.User => "user",
        root.auth.Role.Guest => "guest"
    }
}

// ── Mixed: all four namespace Errors ────────────────────────────

// openai.Error (.status_code), root.Error (.code),
// llm.Error (.reason), auth.Error (.realm)
function OpenAIMixedErrors(
    openai_err: Error,
    root_err: root.Error,
    llm_err: root.llm.Error,
    auth_err: root.auth.Error
) -> string {
    openai_err.body
}

// openai.Model, llm.Model — both named Model but different variants
function OpenAIMixedModels(
    openai_model: Model,
    llm_model: root.llm.Model
) -> string {
    match (openai_model) {
        Model.GPT4o => "gpt4o",
        Model.GPT4oMini => "mini",
        Model.O3 => "o3"
    }
}

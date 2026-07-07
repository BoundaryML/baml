// Harness providers (scenarios 37-42): a coding-agent harness (Claude Code, Pi, Codex) is a
// marker Provider that implements `Realtime` directly over a subprocess — no HttpProvider, no
// fake `call` (OQ1). Permissions/sandbox/tools are bound config fields (38/39); the live
// control/event channel is the `run` parameter. Subprocess transport is host surface pending
// (P8), so `run` throws. (Defined in stdlib because a user-package class cannot yet satisfy a
// stdlib interface's `requires` — see _plan/deviations.md.)

class ClaudeCode {
    model: string?,
    sandbox: string,
    permission_mode: string,
    allowed_tools: string[],

    implements root.ai.Provider {}

    implements root.ai.Realtime {
        function run(
            self,
            prompt: string,
            io: root.ai.Channel,
        ) -> root.ai.Transcript throws root.errors.RealtimeError {
            throw root.errors.Unsupported { message: "harness subprocess transport pending (plan P8)" }
        }
    }
}

class PiAgent {
    endpoint: string,

    implements root.ai.Provider {}

    implements root.ai.Realtime {
        function run(
            self,
            prompt: string,
            io: root.ai.Channel,
        ) -> root.ai.Transcript throws root.errors.RealtimeError {
            throw root.errors.Unsupported { message: "harness transport pending" }
        }
    }
}

// Runs an interactive coding agent with OpenAI Responses. It can inspect and
// edit the current working directory, run shell commands, queue follow-up
// tasks, recall durable Markdown memories, and curate new memories after each
// completed turn. Fresh turns retain the exact OpenAI conversation; ESC asks
// Agent for a resumable checkpoint after the current model/tool transaction.
//
// baml run ai_scenarios.memory_agent --from crates/baml_tests/baml_src_temp2
function memory_agent() -> string {
    _memory_agent_repl(MemoryAgentBackend.OpenAI)
}

// Runs the same interactive Task/Agent/tool/memory application, but re-renders
// both the coding Task and curator Task for Google AI Gemini with
// Task.with_provider(...). Fresh turns retain Gemini thought signatures and
// other provider-owned continuation state.
//
// baml run ai_scenarios.memory_agent_google --from crates/baml_tests/baml_src_temp2
function memory_agent_google() -> string {
    _memory_agent_repl(MemoryAgentBackend.Google)
}

// Runs one OpenAI coding turn without opening the REPL. Set
// BAML_MEMORY_AGENT_TASK to choose the task; the completed turn is still
// offered to the independent memory curator.
//
// BAML_MEMORY_AGENT_TASK="summarize this repo" \
//   baml run ai_scenarios.memory_agent_once --from crates/baml_tests/baml_src_temp2
function memory_agent_once() -> string {
    let task = baml.env.get("BAML_MEMORY_AGENT_TASK")
        ?? "Summarize this repository and report the most important entry points.";
    _memory_agent_ask(task, MemoryAgentBackend.OpenAI)
}

// Runs the same zero-argument one-shot example after overriding both model
// Tasks to Google AI Gemini.
//
// BAML_MEMORY_AGENT_TASK="summarize this repo" \
//   baml run ai_scenarios.memory_agent_google_once --from crates/baml_tests/baml_src_temp2
function memory_agent_google_once() -> string {
    let task = baml.env.get("BAML_MEMORY_AGENT_TASK")
        ?? "Summarize this repository and report the most important entry points.";
    _memory_agent_ask(task, MemoryAgentBackend.Google)
}

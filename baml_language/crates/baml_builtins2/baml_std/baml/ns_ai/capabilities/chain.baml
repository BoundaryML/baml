// Server-stored response chains (scenario 20): the provider keeps the thread server-side
// and each call extends it by handle — e.g. OpenAI's /v1/responses `previous_response_id`
// (see providers/openai_responses.baml for the live implementation).

/// Handle to a server-stored chain. `id` is the provider's thread/response id;
/// `owner` records who created it (ownership travels with the handle).
class ChainHandle {
    id: string,
    owner: string,
}

/// Extend a server-stored thread: prompt in, typed value out, state stays server-side.
interface Chain requires root.ai.Provider {
    function extend<T>(self, prompt: string, handle: ChainHandle) -> T throws root.errors.UnknownError
}

// Validate the complete append batch before any provider mutates its exact
// continuation. Fresh continuation accepts:
//   - USER messages whose parts are text or media (every wire API accepts
//     media in continuation requests);
//   - SYSTEM-role text messages — the portable steering channel
//     (`ai.ChatMessage.steer`): adapters map them to their wire's honest
//     equivalent (OpenAI developer item; Anthropic/Gemini tagged user turn).
// Tool results must travel through AgentProvider.submit so call IDs stay
// correlated; assistant history remains provider-owned.
function _conversation_append_user_texts(
    messages: ai.Messages,
    provider_name: string,
) -> string[] throws ai.InvalidRequest {
    let texts: string[] = [];
    for (let message in messages.items()) {
        if (message.role() == ai.MessageRole.System) {
            //# Steering: text-only by design
            for (let part in message.parts()) {
                if (part.kind() != ai.MessagePartKind.Text) {
                    throw ai.InvalidRequest {
                        provider: provider_name,
                        status_code: null,
                        detail: "steering messages accept text parts only",
                    };
                }
            }
        } else if (message.role() != ai.MessageRole.User) {
            throw ai.InvalidRequest {
                provider: provider_name,
                status_code: null,
                detail: "exact conversation continuation accepts fresh user messages and steering notes; assistant and tool history remain provider-owned",
            };
        } else {
            for (let part in message.parts()) {
                if (part.kind() == ai.MessagePartKind.ProviderData) {
                    throw ai.InvalidRequest {
                        provider: provider_name,
                        status_code: null,
                        detail: "exact conversation continuation accepts portable text and media parts only",
                    };
                }
            }
        }
        let text = "";
        for (let part in message.parts()) {
            text += part.text() ?? "";
        }
        texts.push(text);
    }
    texts
}

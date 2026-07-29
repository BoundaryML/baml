// Validate the complete append batch before any provider mutates its exact
// continuation. Fresh continuation currently accepts portable user text. Tool
// results must travel through AgentProvider.submit so call IDs stay correlated;
// system and assistant history remain provider-owned.
function _conversation_append_user_texts(
    messages: root.ai.Messages,
    provider_name: string,
) -> string[] throws root.ai.InvalidRequest {
    let texts: string[] = [];
    for (let message in messages.items()) {
        if (message.role() != root.ai.MessageRole.User) {
            throw root.ai.InvalidRequest {
                provider: provider_name,
                status_code: null,
                detail: "exact conversation continuation only accepts fresh user messages",
            };
        }
        let text = "";
        for (let part in message.parts()) {
            if (part.kind() != root.ai.MessagePartKind.Text) {
                throw root.ai.InvalidRequest {
                    provider: provider_name,
                    status_code: null,
                    detail: "exact conversation continuation currently accepts portable text parts only",
                };
            }
            text += part.text() ?? "";
        }
        texts.push(text);
    }
    texts
}

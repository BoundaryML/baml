import { handleChatRequest } from '../server/chat-handler';

export default {
  async fetch(request: Request) {
    return handleChatRequest(request);
  },
};

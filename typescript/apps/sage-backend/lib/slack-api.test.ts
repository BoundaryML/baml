import { SlackFeedbackLogger } from '@/lib/slack-api';
import type { SendFeedbackRequest } from '@baml/sage-interface';
import { describe, it } from 'vitest';

describe('SlackFeedbackLogger', () => {
  const exampleFeedback: SendFeedbackRequest = {
    session_id: 'test-session-123',
    feedback_type: 'thumbs_down',
    comment: 'answer was not useful',
    messages: [
      {
        role: 'user',
        text: 'How do I use BAML with TypeScript?',
      },
      {
        role: 'assistant',
        message_id: 'msg-2025-08-05T19:24:53.414Z',
        text: 'You can use BAML with TypeScript by installing the package.',
        ranked_docs: [
          {
            title: 'TypeScript Guide',
            url: 'https://docs.boundaryml.com/typescript',
            relevance: 'very-relevant',
          },
        ],
      },
    ],
  };

  describe('sendFeedback', () => {
    it('should send feedback to Slack successfully', async () => {
      const slackLogger = new SlackFeedbackLogger();
      await slackLogger.sendFeedback(exampleFeedback);
    });
  });
});

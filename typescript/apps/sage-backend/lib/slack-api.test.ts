import { describe, it, expect, beforeEach } from 'vitest';
import { SlackFeedbackLogger } from './slack-api';
import type { SendFeedbackRequest } from '@baml/sage-interface';

describe('SlackFeedbackLogger', () => {
  const logger = new SlackFeedbackLogger();

  const mockFeedback: SendFeedbackRequest = {
    session_id: 'test-session-123',
    feedback_type: 'thumbs_down',
    comment: 'answer was not useful',
    messages: [
      {
        role: 'user',
        text: 'How do I use BAML with TypeScript?'
      },
      {
        role: 'assistant',
        text: 'You can use BAML with TypeScript by installing the package.',
        ranked_docs: [
          {
            title: 'TypeScript Guide',
            url: 'https://docs.boundaryml.com/typescript',
            relevance: 'very-relevant'
          }
        ]
      }
    ]
  };

  describe('helper methods', () => {

    it('should send feedback to Slack successfully', async () => {
      const mockSlackResponse = {
        ok: true,
        channel: 'C1234567890',
        ts: '1234567890.123456',
        message: {
          text: 'Feedback received',
          blocks: expect.any(Array)
        }
      };

      const result = await logger.sendFeedback(mockFeedback);

      expect(result).toEqual(mockSlackResponse);
    });
  });
});
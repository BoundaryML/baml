import { describe, it, expect, beforeEach } from 'vitest';
import { SlackFeedbackLogger, sendFeedbackToSlack } from './slack-feedback';
import type { SendFeedbackRequest } from '@baml/sage-interface';

describe('SlackFeedbackLogger', () => {
  let logger: SlackFeedbackLogger;

  const mockFeedback: SendFeedbackRequest = {
    session_id: 'test-session-123',
    feedback_type: 'thumbs_up',
    comment: 'Great answer!',
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

  beforeEach(() => {
    // Set up test environment variables - these won't actually be used for real API calls
    process.env.SLACK_BOUNDARY_BOT_TOKEN = 'xoxb-test-token-for-testing';
    process.env.SLACK_FEEDBACK_CHANNEL = '#test-feedback';
    
    logger = new SlackFeedbackLogger();
  });

  describe('constructor', () => {
    it('should create SlackFeedbackLogger with environment variables', () => {
      expect(logger).toBeInstanceOf(SlackFeedbackLogger);
    });

    it('should throw error when token is missing', () => {
      delete process.env.SLACK_BOUNDARY_BOT_TOKEN;
      
      expect(() => new SlackFeedbackLogger()).toThrow(
        'SLACK_BOUNDARY_BOT_TOKEN environment variable or config.token is required'
      );
      
      // Restore for other tests
      process.env.SLACK_BOUNDARY_BOT_TOKEN = 'xoxb-test-token-for-testing';
    });

    it('should use custom config when provided', () => {
      const customLogger = new SlackFeedbackLogger({
        token: 'custom-token',
        channel: '#custom-channel'
      });
      
      expect(customLogger).toBeInstanceOf(SlackFeedbackLogger);
    });
  });

  describe('helper methods', () => {
    it('should extract last Q&A correctly', () => {
      // Access private method through prototype for testing
      const extractLastQnA = (logger as any).extractLastQnA.bind(logger);
      
      const result = extractLastQnA(mockFeedback.messages);
      
      expect(result).toEqual({
        question: 'How do I use BAML with TypeScript?',
        answer: 'You can use BAML with TypeScript by installing the package.'
      });
    });

    it('should handle empty messages array', () => {
      const extractLastQnA = (logger as any).extractLastQnA.bind(logger);
      
      const result = extractLastQnA([]);
      
      expect(result).toEqual({
        question: '',
        answer: ''
      });
    });

    it('should format conversation correctly', () => {
      const formatConversation = (logger as any).formatConversation.bind(logger);
      
      const result = formatConversation(mockFeedback.messages);
      
      expect(result).toContain('🧑 *user:* How do I use BAML with TypeScript?');
      expect(result).toContain('🤖 *assistant:* You can use BAML with TypeScript by installing the package.');
    });

    it('should handle messages with no text in assistant response', () => {
      const formatConversation = (logger as any).formatConversation.bind(logger);
      
      const messagesWithNullText = [
        { role: 'user', text: 'Test question' },
        { role: 'assistant', text: null, ranked_docs: [] }
      ] as any;
      
      const result = formatConversation(messagesWithNullText);
      
      expect(result).toContain('🧑 *user:* Test question');
      expect(result).toContain('🤖 *assistant:* ');
    });
  });
});
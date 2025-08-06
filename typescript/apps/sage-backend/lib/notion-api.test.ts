/**
 * Integration test for Notion API using the NotionLogger class
 *
 * This test does NOT mock the Notion API - it makes real API calls
 *
 * Usage:
 * 1. Make sure you have the environment variables set:
 *    NOTION_BOUNDARY_BOT_TOKEN=secret_your_integration_token
 *    NOTION_ASK_BAAAML_DATABASE_ID=your_database_id
 *
 * 2. Run the test:
 *    pnpm tsx tools/notion-api-test.ts
 *
 * 3. Or run with vitest:
 *    pnpm test notion-api-test
 */

import type { AssistantMessage, SendFeedbackRequest, UserMessage } from '@baml/sage-interface';
import { afterAll, beforeAll, describe, expect, it } from 'vitest';
import { type NotionLogEntry, NotionLogger } from './notion-api';

// Test data
const testSessionId = `test_session_${Date.now()}`;

const testUserMessage: UserMessage = {
  role: 'user',
  text: 'notion-api.test.ts: How do I use BAML with TypeScript for testing?',
  language_preference: 'python',
};

const testAssistantMessage: AssistantMessage = {
  role: 'assistant',
  message_id: 'msg-2025-08-05T19:24:53.414Z',
  text: 'You can use BAML with TypeScript by installing the package and following the quickstart guide. This is a test message.',
  ranked_docs: [
    {
      title: 'BAML TypeScript Quickstart',
      url: 'https://docs.baml.ai/typescript/quickstart',
      relevance: 'very-relevant',
    },
    {
      title: 'BAML Testing Guide',
      url: 'https://docs.baml.ai/testing',
      relevance: 'relevant',
    },
  ],
  suggested_messages: ['Can you show me an example?', 'How do I handle errors?'],
};

const testEntry: NotionLogEntry = {
  session_id: testSessionId,
  user_message: testUserMessage,
  assistant_message: testAssistantMessage,
};

describe('NotionLogger Integration Tests', () => {
  let logger: NotionLogger;
  let createdPageId: string;

  beforeAll(() => {
    logger = new NotionLogger();
    console.log('🧪 Starting Notion API integration tests...');
    console.log(`Test session ID: ${testSessionId}`);
  });

  afterAll(async () => {
    // Clean up: Try to delete the test entries
    // Note: Notion API doesn't support deleting pages, so they'll remain in the database
    console.log(
      "🧹 Test completed. Note: Test entries remain in Notion database as API doesn't support deletion.",
    );
    console.log(`Search for session_id: ${testSessionId} to find test entries.`);
  });

  describe('appendEntry', () => {
    it('should successfully append a new log entry to Notion database', async () => {
      // Act
      const pageId = await logger.appendEntry(testEntry);

      // Assert
      expect(pageId).toBeDefined();
      expect(typeof pageId).toBe('string');
      expect(pageId.length).toBeGreaterThan(0);

      // Store the page ID for use in other tests
      createdPageId = pageId;

      console.log(`✅ Created Notion page with ID: ${pageId}`);
    }, 30000); // 30 second timeout for API call

    it('should handle entry with all optional fields', async () => {
      const entryWithFeedback: NotionLogEntry = {
        ...testEntry,
        session_id: `${testSessionId}_with_feedback`,
        assistant_message: {
          ...testAssistantMessage,
          message_id: `msg-2025-08-05T19:24:53.415Z`,
        },
        feedback_type: 'thumbs_up',
        feedback_comment: 'This response was very helpful!',
      };

      // Act
      const pageId = await logger.appendEntry(entryWithFeedback);

      // Assert
      expect(pageId).toBeDefined();
      expect(typeof pageId).toBe('string');

      console.log(`✅ Created Notion page with feedback: ${pageId}`);
    }, 30000);
  });

  describe('updateFeedback', () => {
    it('should successfully update feedback for an existing entry', async () => {
      // First ensure we have a page to update
      if (!createdPageId) {
        const pageId = await logger.appendEntry(testEntry);
        createdPageId = pageId;
      }

      // Create feedback request
      const feedbackRequest: SendFeedbackRequest = {
        session_id: testSessionId,
        messages: [testUserMessage, testAssistantMessage],
        feedback_type: 'thumbs_up',
        comment: 'Great response! Very helpful.',
      };

      // Act
      const result = await logger.updateFeedback(feedbackRequest);

      // Assert
      expect(result).toBeDefined();
      expect(result.pageId).toBeDefined();
      expect(typeof result.pageId).toBe('string');

      console.log(`✅ Updated feedback for page: ${result.pageId}`);
    }, 30000);

    it('should handle feedback with thumbs_down and long comment', async () => {
      const feedbackRequest: SendFeedbackRequest = {
        session_id: testSessionId,
        messages: [testUserMessage, testAssistantMessage],
        feedback_type: 'thumbs_down',
        comment:
          'This response was not accurate. It failed to address the specific TypeScript integration issues I mentioned. The documentation links were generic and not helpful for my use case. I need more specific examples.',
      };

      // Act
      const result = await logger.updateFeedback(feedbackRequest);

      // Assert
      expect(result).toBeDefined();
      expect(result.pageId).toBeDefined();

      console.log(`✅ Updated feedback with thumbs_down: ${result.pageId}`);
    }, 30000);

    it('should handle feedback without comment', async () => {
      const feedbackRequest: SendFeedbackRequest = {
        session_id: testSessionId,
        messages: [testUserMessage, testAssistantMessage],
        feedback_type: 'thumbs_up',
        // No comment field
      };

      // Act
      const result = await logger.updateFeedback(feedbackRequest);

      // Assert
      expect(result).toBeDefined();
      expect(result.pageId).toBeDefined();

      console.log(`✅ Updated feedback without comment: ${result.pageId}`);
    }, 30000);

    it('should return null pageId when entry is not found', async () => {
      const nonExistentFeedbackRequest: SendFeedbackRequest = {
        session_id: 'non_existent_session_12345',
        messages: [
          testUserMessage,
          {
            ...testAssistantMessage,
            message_id: 'non_existent_message_id_12345',
          },
        ],
        feedback_type: 'thumbs_up',
        comment: 'This should not find any entry',
      };

      // Act
      const result = await logger.updateFeedback(nonExistentFeedbackRequest);

      // Assert
      expect(result).toBeDefined();
      expect(result.pageId).toBeNull();

      console.log(`✅ Correctly returned null for non-existent entry`);
    }, 30000);

    it('should handle multiple assistant messages (should fail)', async () => {
      const multipleAssistantRequest: SendFeedbackRequest = {
        session_id: testSessionId,
        messages: [
          testUserMessage,
          testAssistantMessage,
          {
            ...testAssistantMessage,
            message_id: 'second_assistant_message',
          },
        ],
        feedback_type: 'thumbs_up',
      };

      // Act & Assert
      await expect(logger.updateFeedback(multipleAssistantRequest)).rejects.toThrow(
        'More than one assistant message in feedback request',
      );

      console.log(`✅ Correctly rejected multiple assistant messages`);
    }, 30000);
  });

  describe('toUrl', () => {
    it('should generate correct Notion URL format', () => {
      const testPageId = '246bb2d2-6216-8152-8dfa-cb306212b8e0';

      // Act
      const url = logger.toUrl({ pageId: testPageId });

      // Assert
      expect(url).toBeDefined();
      expect(url).toMatch(/^https:\/\/www\.notion\.so\/gloochat\//);
      expect(url).toContain('?v=');
      expect(url).toContain('&p=');
      expect(url).toContain('&pm=s');
      expect(url).not.toContain('-'); // Should not contain hyphens in IDs

      console.log(`✅ Generated URL: ${url}`);
    });

    it('should handle pageId with and without hyphens', () => {
      const pageIdWithHyphens = '246bb2d2-6216-8152-8dfa-cb306212b8e0';
      const pageIdWithoutHyphens = '246bb2d2621681528dfacb306212b8e0';

      // Act
      const url1 = logger.toUrl({ pageId: pageIdWithHyphens });
      const url2 = logger.toUrl({ pageId: pageIdWithoutHyphens });

      // Assert - Both should produce the same clean URL
      expect(url1).toBe(url2);
      expect(url1).toContain('246bb2d2621681528dfacb306212b8e0');

      console.log(`✅ Both formats produce same URL: ${url1}`);
    });
  });
});

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

import { describe, it, expect, beforeAll, afterAll } from 'vitest';
import { NotionLogger, type NotionLogEntry } from './notion-api';
import { type UserMessage, type AssistantMessage } from '@baml/sage-interface';

// Test data
const testSessionId = `test_session_${Date.now()}`;
const testTimestamp = new Date().toISOString();

const testUserMessage: UserMessage = {
  role: 'user',
  text: 'How do I use BAML with TypeScript for testing?',
  language_preference: 'en'
};

const testAssistantMessage: AssistantMessage = {
  role: 'assistant',
  text: 'You can use BAML with TypeScript by installing the package and following the quickstart guide. This is a test message.',
  ranked_docs: [
    {
      title: 'BAML TypeScript Quickstart',
      url: 'https://docs.baml.ai/typescript/quickstart',
      relevance: 'very-relevant'
    },
    {
      title: 'BAML Testing Guide',
      url: 'https://docs.baml.ai/testing',
      relevance: 'relevant'
    }
  ],
  suggested_messages: [
    'Can you show me an example?',
    'How do I handle errors?'
  ]
};

const testEntry: NotionLogEntry = {
  session_id: testSessionId,
  assistant_timestamp: testTimestamp,
  user_message: testUserMessage,
  assistant_message: testAssistantMessage
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
    console.log('🧹 Test completed. Note: Test entries remain in Notion database as API doesn\'t support deletion.');
    console.log(`Search for session_id: ${testSessionId} to find test entries.`);
  });

  it('should create a new log entry', async () => {
    console.log('📝 Testing appendEntry...');
    
    const pageId = await logger.appendEntry(testEntry);
    
    expect(pageId).toBeDefined();
    expect(typeof pageId).toBe('string');
    expect(pageId.length).toBeGreaterThan(0);
    
    createdPageId = pageId;
    console.log(`✅ Created page with ID: ${pageId}`);
  }, 30000); // 30 second timeout for API calls

  it('should update an existing log entry', async () => {
    console.log('✏️ Testing updateEntry...');
    
    // Modify the entry
    const updatedEntry: NotionLogEntry = {
      ...testEntry,
      user_message: {
        ...testUserMessage,
        text: 'How do I use BAML with TypeScript? (UPDATED)'
      },
      assistant_message: {
        ...testAssistantMessage,
        text: 'Updated answer: You can use BAML with TypeScript by following these updated steps...'
      }
    };
    
    const updatedPageId = await logger.updateEntry(updatedEntry);
    
    expect(updatedPageId).toBeDefined();
    expect(updatedPageId).toBe(createdPageId);
    console.log(`✅ Updated page with ID: ${updatedPageId}`);
  }, 30000);

  it('should return null when trying to update non-existent entry', async () => {
    console.log('🔍 Testing updateEntry with non-existent entry...');
    
    const nonExistentEntry: NotionLogEntry = {
      ...testEntry,
      session_id: 'non_existent_session',
      assistant_timestamp: 'non_existent_timestamp'
    };
    
    const result = await logger.updateEntry(nonExistentEntry);
    
    expect(result).toBeNull();
    console.log('✅ Correctly returned null for non-existent entry');
  }, 30000);

  it('should upsert (update existing entry)', async () => {
    console.log('🔄 Testing upsertEntry with existing entry...');
    
    const upsertEntry: NotionLogEntry = {
      ...testEntry,
      assistant_message: {
        ...testAssistantMessage,
        text: 'Upserted answer: This entry was updated via upsert operation.'
      }
    };
    
    const result = await logger.upsertEntry(upsertEntry);
    
    expect(result.id).toBeDefined();
    expect(result.operation).toBe('updated');
    expect(result.id).toBe(createdPageId);
    console.log(`✅ Upserted (updated) page with ID: ${result.id}`);
  }, 30000);

  it('should upsert (create new entry)', async () => {
    console.log('➕ Testing upsertEntry with new entry...');
    
    const newTimestamp = new Date(Date.now() + 1000).toISOString();
    const newEntry: NotionLogEntry = {
      ...testEntry,
      assistant_timestamp: newTimestamp,
      user_message: {
        ...testUserMessage,
        text: 'This is a new entry created via upsert'
      }
    };
    
    const result = await logger.upsertEntry(newEntry);
    
    expect(result.id).toBeDefined();
    expect(result.operation).toBe('created');
    expect(result.id).not.toBe(createdPageId);
    console.log(`✅ Upserted (created) new page with ID: ${result.id}`);
  }, 30000);

  it('should handle entries with minimal data', async () => {
    console.log('📋 Testing with minimal data...');
    
    const minimalUserMessage: UserMessage = {
      role: 'user',
      text: 'Minimal test question'
    };
    
    const minimalAssistantMessage: AssistantMessage = {
      role: 'assistant',
      text: 'Minimal test answer',
      ranked_docs: []
    };
    
    const minimalEntry: NotionLogEntry = {
      session_id: `minimal_${testSessionId}`,
      assistant_timestamp: new Date().toISOString(),
      user_message: minimalUserMessage,
      assistant_message: minimalAssistantMessage
    };
    
    const pageId = await logger.appendEntry(minimalEntry);
    
    expect(pageId).toBeDefined();
    expect(typeof pageId).toBe('string');
    console.log(`✅ Created minimal entry with ID: ${pageId}`);
  }, 30000);

  it('should handle entries with null assistant text', async () => {
    console.log('🚫 Testing with null assistant text...');
    
    const nullTextAssistantMessage: AssistantMessage = {
      role: 'assistant',
      text: null,
      ranked_docs: [
        {
          title: 'Test Doc',
          url: 'https://example.com',
          relevance: 'relevant'
        }
      ],
      suggested_messages: ['Try this', 'Or this']
    };
    
    const nullTextEntry: NotionLogEntry = {
      session_id: `null_text_${testSessionId}`,
      assistant_timestamp: new Date().toISOString(),
      user_message: testUserMessage,
      assistant_message: nullTextAssistantMessage
    };
    
    const pageId = await logger.appendEntry(nullTextEntry);
    
    expect(pageId).toBeDefined();
    expect(typeof pageId).toBe('string');
    console.log(`✅ Created entry with null text with ID: ${pageId}`);
  }, 30000);

  it('should handle custom configuration', async () => {
    console.log('⚙️ Testing with custom configuration...');
    
    const customLogger = new NotionLogger({
      token: process.env.NOTION_BOUNDARY_BOT_TOKEN,
      databaseId: process.env.NOTION_ASK_BAAAML_DATABASE_ID
    });
    
    const customEntry: NotionLogEntry = {
      session_id: `custom_${testSessionId}`,
      assistant_timestamp: new Date().toISOString(),
      user_message: {
        role: 'user',
        text: 'Custom logger test'
      },
      assistant_message: {
        role: 'assistant',
        text: 'Custom logger response',
        ranked_docs: []
      }
    };
    
    const pageId = await customLogger.appendEntry(customEntry);
    
    expect(pageId).toBeDefined();
    expect(typeof pageId).toBe('string');
    console.log(`✅ Custom logger created entry with ID: ${pageId}`);
  }, 30000);
});

// Run the tests if this file is executed directly
if (import.meta.url === `file://${process.argv[1]}`) {
  console.log('🚀 Running Notion API integration tests directly...');
  
  // Simple test runner for direct execution
  async function runTests() {
    try {
      console.log('Environment check...');
      if (!process.env.NOTION_BOUNDARY_BOT_TOKEN || !process.env.NOTION_ASK_BAAAML_DATABASE_ID) {
        throw new Error('Missing required environment variables');
      }
      
      const logger = new NotionLogger();
      console.log('✅ NotionLogger initialized successfully');
      
      console.log('\n📝 Testing appendEntry...');
      const pageId = await logger.appendEntry(testEntry);
      console.log(`✅ Created entry: ${pageId}`);
      
      console.log('\n✏️ Testing updateEntry...');
      const updatedEntry = {
        ...testEntry,
        assistant_message: {
          ...testAssistantMessage,
          text: 'Updated via direct test run'
        }
      };
      const updatedId = await logger.updateEntry(updatedEntry);
      console.log(`✅ Updated entry: ${updatedId}`);
      
      console.log('\n🔄 Testing upsertEntry...');
      const upsertResult = await logger.upsertEntry({
        ...testEntry,
        assistant_timestamp: new Date().toISOString(),
        user_message: {
          ...testUserMessage,
          text: 'Direct test upsert'
        }
      });
      console.log(`✅ Upserted entry: ${upsertResult.id} (${upsertResult.operation})`);
      
      console.log('\n🎉 All tests completed successfully!');
      
    } catch (error) {
      console.error('❌ Test failed:', error);
      process.exit(1);
    }
  }
  
  runTests();
}
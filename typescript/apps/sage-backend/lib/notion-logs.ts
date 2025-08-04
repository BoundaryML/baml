import { Client } from '@notionhq/client';
import { type UserMessage, type AssistantMessage } from '@baml/sage-interface';
import { z } from 'zod';

// Initialize Notion client
const notion = new Client({
  auth: process.env.NOTION_BOUNDARY_BOT_TOKEN,
});

const DATABASE_ID = process.env.NOTION_ASK_BAAAML_DATABASE_ID;

export interface NotionLogEntry {
  session_id: string;
  assistant_timestamp: string;
  user_message: UserMessage;
  assistant_message: AssistantMessage;
}

/**
 * Helper function to build Notion page properties from log entry data
 */
function buildNotionProperties(data: NotionLogEntry) {
  const { text: userMessageText, ...userMessageRest } = data.user_message;
  const { text: assistantMessageText, ...assistantMessageRest } = data.assistant_message;

  return {
    // Session ID (Text field)
    'Session ID': {
      rich_text: [
        {
          text: {
            content: data.session_id,
          },
        },
      ],
    },

    // Assistant Timestamp (Text field) - used for querying/updating
    'Assistant Timestamp': {
      rich_text: [
        {
          text: {
            content: data.assistant_timestamp,
          },
        },
      ],
    },

    // User Message (Title field)
    'User Message': {
      rich_text: [
        {
          text: {
            content: data.user_message.text,
          },
        },
      ],
    },
    
    'User Message Fields': {
      rich_text: [
        {
          text: {
            content: JSON.stringify(userMessageRest, null, 2),
          },
        },
      ],
    },

    // Assistant Message (Text field)
    'Assistant Message': {
      rich_text: [
        {
          text: {
            content: data.assistant_message.text || '',
          },
        },
      ],
    },

    'Assistant Message Fields': {
      rich_text: [
        {
          text: {
            content: JSON.stringify(assistantMessageRest, null, 2),
          },
        },
      ],
    },

    // Created At (Date field)
    'Created At': {
      date: {
        start: new Date().toISOString(),
      },
    },
  };
}

/**
 * Update database schema to ensure all required properties exist
 */
async function ensureDatabaseSchema() {
  if (!DATABASE_ID) {
    throw new Error('NOTION_ASK_BAAAML_DATABASE_ID environment variable is required');
  }

  try {
    await notion.databases.update({
      database_id: DATABASE_ID,
      properties: Object.fromEntries(
        Object.entries(buildNotionProperties({
          session_id: 'test',
          assistant_timestamp: 'test',
          user_message: { text: 'test' },
          assistant_message: { text: 'test' },
        })).map(([key, value]) => [
          key,
          // Extract just the type information from the property
          {
            [Object.keys(value)[0]]: {},
          },
        ])
      ),
    });
  } catch (error) {
    console.warn('Failed to update database schema:', error);
    // Continue anyway - the database might already have the correct schema
  }
}

/**
 * Append a new log entry to the Notion database
 */
export async function appendToNotionDatabase(entry: NotionLogEntry): Promise<string> {
  if (!process.env.NOTION_BOUNDARY_BOT_TOKEN) {
    throw new Error('NOTION_BOUNDARY_BOT_TOKEN environment variable is required');
  }

  if (!DATABASE_ID) {
    throw new Error('NOTION_ASK_BAAAML_DATABASE_ID environment variable is required');
  }

  // Ensure database schema is correct
  await ensureDatabaseSchema();

  try {
    const response = await notion.pages.create({
      parent: {
        database_id: DATABASE_ID,
      },
      properties: buildNotionProperties(entry),
    });

    return response.id;
  } catch (error) {
    console.error('Failed to append to Notion database:', error);
    throw error;
  }
}

/**
 * Find a page in the database by session_id and assistant_timestamp
 */
async function findPageBySessionAndTimestamp(
  session_id: string,
  assistant_timestamp: string
): Promise<string | null> {
  if (!DATABASE_ID) {
    throw new Error('NOTION_ASK_BAAAML_DATABASE_ID environment variable is required');
  }

  try {
    const response = await notion.databases.query({
      database_id: DATABASE_ID,
      filter: {
        and: [
          {
            property: 'Session ID',
            rich_text: {
              equals: session_id,
            },
          },
          {
            property: 'Assistant Timestamp',
            rich_text: {
              equals: assistant_timestamp,
            },
          },
        ],
      },
      page_size: 1, // We only need the first match
    });

    if (response.results.length > 0) {
      return response.results[0].id;
    }

    return null;
  } catch (error) {
    console.error('Failed to query Notion database:', error);
    throw error;
  }
}

/**
 * Update the first row that matches session_id and assistant_timestamp
 */
export async function updateNotionEntry(entry: NotionLogEntry): Promise<string | null> {
  if (!process.env.NOTION_BOUNDARY_BOT_TOKEN) {
    throw new Error('NOTION_BOUNDARY_BOT_TOKEN environment variable is required');
  }

  // Find the existing page
  const pageId = await findPageBySessionAndTimestamp(
    entry.session_id,
    entry.assistant_timestamp
  );

  if (!pageId) {
    console.warn(
      `No existing entry found for session_id: ${entry.session_id}, assistant_timestamp: ${entry.assistant_timestamp}`
    );
    return null;
  }

  try {
    // Update the existing page
    const response = await notion.pages.update({
      page_id: pageId,
      properties: buildNotionProperties(entry),
    });

    return response.id;
  } catch (error) {
    console.error('Failed to update Notion entry:', error);
    throw error;
  }
}

/**
 * Append or update a log entry based on whether it already exists
 */
export async function upsertNotionEntry(entry: NotionLogEntry): Promise<{
  id: string;
  operation: 'created' | 'updated';
}> {
  // Try to update first
  const updatedId = await updateNotionEntry(entry);
  
  if (updatedId) {
    return { id: updatedId, operation: 'updated' };
  }

  // If no existing entry, create a new one
  const createdId = await appendToNotionDatabase(entry);
  return { id: createdId, operation: 'created' };
}
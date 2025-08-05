import { Client } from '@notionhq/client';
import { type UserMessage, type AssistantMessage } from '@baml/sage-interface';

export interface NotionLogEntry {
  session_id: string;
  user_message: UserMessage;
  assistant_message: AssistantMessage;
}

export class NotionLogger {
  private notion: Client;
  private databaseId: string;

  constructor() {
    const token = process.env.NOTION_BOUNDARY_BOT_TOKEN;
    const databaseId = process.env.NOTION_ASK_BAML_LOGS_DATABASE_ID;

    if (!token) {
      throw new Error('NOTION_BOUNDARY_BOT_TOKEN environment variable is required');
    }

    if (!databaseId) {
      throw new Error('NOTION_ASK_BAML_LOGS_DATABASE_ID environment variable is required');
    }

    this.notion = new Client({ auth: token });
    this.databaseId = databaseId;
  }

  /**
   * Helper method to build Notion page properties from log entry data
   */
  private buildNotionProperties = (data: NotionLogEntry) => {
    const { text: userMessageText, role: _userRole, ...userMessageRest } = data.user_message;
    const { text: assistantMessageText, role: _assistantRole, message_id: assistantMessageId, ...assistantMessageRest } = data.assistant_message;

    return {
      // Session ID (Text field)
      'Session ID': {
        title: [
          {
            text: {
              content: data.session_id,
            },
          },
        ],
      },

      // Assistant Timestamp (Text field) - used for querying/updating
      'Response ID': {
        rich_text: [
          {
            text: {
              content: assistantMessageId,
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
  };

  /**
   * Update database schema to ensure all required properties exist
   */
  private ensureDatabaseSchema = async () => {
    try {
      await this.notion.databases.update({
        database_id: this.databaseId,
        properties: Object.fromEntries(
          Object.entries(this.buildNotionProperties({
            session_id: 'test',
            assistant_timestamp: 'test',
            user_message: { role: 'user', text: 'test' },
            assistant_message: { role: 'assistant', text: 'test', ranked_docs: [] },
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
  };

  getDatabaseSchema = async () => {
    const response = await this.notion.databases.retrieve({
      database_id: this.databaseId,
    });
    return response.properties;
  };

  /**
   * Append a new log entry to the Notion database
   */
  appendEntry = async (entry: NotionLogEntry): Promise<string> => {
    // Ensure database schema is correct
    await this.ensureDatabaseSchema();

    try {
      const response = await this.notion.pages.create({
        parent: {
          database_id: this.databaseId,
        },
        properties: this.buildNotionProperties(entry),
      });

      return response.id;
    } catch (error) {
      console.error('Failed to append to Notion database:', error);
      throw error;
    }
  };

  /**
   * Find a page in the database by session_id and assistant_timestamp
   */
  private findPageBySessionAndTimestamp = async (
    session_id: string,
    assistant_timestamp: string
  ): Promise<string | null> => {
    try {
      const response = await this.notion.databases.query({
        database_id: this.databaseId,
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
  };

  /**
   * Update the first row that matches session_id and assistant_timestamp
   */
  updateEntry = async (entry: NotionLogEntry): Promise<string | null> => {
    // Find the existing page
    const pageId = await this.findPageBySessionAndTimestamp(
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
      const response = await this.notion.pages.update({
        page_id: pageId,
        properties: this.buildNotionProperties(entry),
      });

      return response.id;
    } catch (error) {
      console.error('Failed to update Notion entry:', error);
      throw error;
    }
  };

  /**
   * Append or update a log entry based on whether it already exists
   */
  upsertEntry = async (entry: NotionLogEntry): Promise<{
    id: string;
    operation: 'created' | 'updated';
  }> => {
    // Try to update first
    const updatedId = await this.updateEntry(entry);
    
    if (updatedId) {
      return { id: updatedId, operation: 'updated' };
    }

    // If no existing entry, create a new one
    const createdId = await this.appendEntry(entry);
    return { id: createdId, operation: 'created' };
  };
}

// Export convenience functions for backward compatibility
export async function appendToNotionDatabase(entry: NotionLogEntry): Promise<string> {
  const logger = new NotionLogger();
  return logger.appendEntry(entry);
}

export async function updateNotionEntry(entry: NotionLogEntry): Promise<string | null> {
  const logger = new NotionLogger();
  return logger.updateEntry(entry);
}

export async function upsertNotionEntry(entry: NotionLogEntry): Promise<{
  id: string;
  operation: 'created' | 'updated';
}> {
  const logger = new NotionLogger();
  return logger.upsertEntry(entry);
}
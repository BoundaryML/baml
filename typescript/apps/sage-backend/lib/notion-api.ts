import { Client } from '@notionhq/client';
import { type UserMessage, type AssistantMessage, type SendFeedbackRequest } from '@baml/sage-interface';

export interface NotionLogEntry {
  session_id: string;
  user_message: UserMessage;
  assistant_message: AssistantMessage;
  feedback_type?: 'thumbs_up' | 'thumbs_down';
  feedback_comment?: string;
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

      // Feedback Type (Select field)
      'Feedback Type': data.feedback_type ? {
        select: {
          name: data.feedback_type,
        },
      } : { select: null },

      // Feedback Comment (Text field)
      'Feedback Comment': data.feedback_comment ? {
        rich_text: [
          {
            text: {
              content: data.feedback_comment,
            },
          },
        ],
      } : { rich_text: [] },
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
            user_message: { role: 'user', text: 'test' },
            assistant_message: { role: 'assistant', text: 'test', message_id: 'test', ranked_docs: [] },
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
   * Find a page in the database by session_id and response_id
   */
  private findPageBySessionAndResponseId = async (
    session_id: string,
    response_id: string
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
              property: 'Response ID',
              rich_text: {
                equals: response_id,
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
   * Update the first row that matches session_id and response_id
   */
  updateEntry = async (entry: NotionLogEntry): Promise<string | null> => {
    // Find the existing page
    const pageId = await this.findPageBySessionAndResponseId(
      entry.session_id,
      entry.assistant_message.message_id
    );

    if (!pageId) {
      console.warn(
        `No existing entry found for session_id: ${entry.session_id}, response_id: ${entry.assistant_message.message_id}`
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

  /**
   * Update feedback for entries based on session_id and message_ids from feedback request
   */
  updateFeedback = async (feedbackRequest: SendFeedbackRequest): Promise<{
    updated: number;
    failed: string[];
  }> => {
    const results = {
      updated: 0,
      failed: [] as string[],
    };

    // Find all assistant messages in the feedback request
    const assistantMessages = feedbackRequest.messages.filter(
      (msg) => msg.role === 'assistant'
    ) as AssistantMessage[];

    // Update each assistant message with feedback
    for (const assistantMessage of assistantMessages) {
      try {
        const pageId = await this.findPageBySessionAndResponseId(
          feedbackRequest.session_id,
          assistantMessage.message_id
        );

        if (!pageId) {
          results.failed.push(
            `No entry found for session_id: ${feedbackRequest.session_id}, response_id: ${assistantMessage.message_id}`
          );
          continue;
        }

        // Update only the feedback properties
        await this.notion.pages.update({
          page_id: pageId,
          properties: {
            'Feedback Type': {
              select: {
                name: feedbackRequest.feedback_type,
              },
            },
            'Feedback Comment': feedbackRequest.comment ? {
              rich_text: [
                {
                  text: {
                    content: feedbackRequest.comment,
                  },
                },
              ],
            } : { rich_text: [] },
          },
        });

        results.updated++;
      } catch (error) {
        console.error(`Failed to update feedback for message ${assistantMessage.message_id}:`, error);
        results.failed.push(
          `Failed to update message ${assistantMessage.message_id}: ${error instanceof Error ? error.message : 'Unknown error'}`
        );
      }
    }

    return results;
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

export async function updateNotionFeedback(feedbackRequest: SendFeedbackRequest): Promise<{
  updated: number;
  failed: string[];
}> {
  const logger = new NotionLogger();
  return logger.updateFeedback(feedbackRequest);
}
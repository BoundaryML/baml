import type { AssistantMessage, SendFeedbackRequest, UserMessage } from '@baml/sage-interface';
import { Client, type CreatePageParameters } from '@notionhq/client';

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

  private buildNotionProperties = (
    data: NotionLogEntry,
  ): NonNullable<CreatePageParameters['properties']> => {
    const { text: userMessageText, role: _userRole, ...userMessageRest } = data.user_message;
    const {
      text: assistantMessageText,
      role: _assistantRole,
      message_id: assistantMessageId,
      ...assistantMessageRest
    } = data.assistant_message;

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

      // Feedback Type (Select field)
      ...(data.feedback_type && {
        'Feedback Type': {
          select: {
            name: data.feedback_type,
            color: data.feedback_type === 'thumbs_up' ? ('green' as const) : ('red' as const),
          },
        },
      }),

      // Feedback Comment (Text field)
      'Feedback Comment': {
        rich_text: [
          {
            text: {
              content: data.feedback_comment ?? '',
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
   * Helper method to build Notion page properties from log entry data
   */
  private buildNotionSchema = (): Record<
    keyof ReturnType<typeof this.buildNotionProperties>,
    any
  > => {
    return {
      // Session ID (Title field)
      'Session ID': {
        title: {},
      },

      // Response ID (Text field)
      'Response ID': {
        rich_text: {},
      },

      // Feedback Type (Select field)
      'Feedback Type': {
        select: {
          options: [
            { name: 'thumbs_up', color: 'green' as const },
            { name: 'thumbs_down', color: 'red' as const },
          ],
        },
      },

      // Feedback Comment (Text field)
      'Feedback Comment': {
        rich_text: {},
      },

      // User Message (Text field)
      'User Message': {
        rich_text: {},
      },

      // User Message Fields (Text field)
      'User Message Fields': {
        rich_text: {},
      },

      // Assistant Message (Text field)
      'Assistant Message': {
        rich_text: {},
      },

      // Assistant Message Fields (Text field)
      'Assistant Message Fields': {
        rich_text: {},
      },

      // Created At (Date field)
      'Created At': {
        date: {},
      },
    };
  };

  private ensureDatabaseSchema = async () => {
    try {
      const properties = this.buildNotionSchema();

      await this.notion.databases.update({
        database_id: this.databaseId,
        properties: this.buildNotionSchema(),
      });

      return properties;
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
    response_id: string,
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
   * Update feedback for entries based on session_id and message_ids from feedback request
   */
  updateFeedback = async (
    feedbackRequest: SendFeedbackRequest,
  ): Promise<{
    pageId: string | null;
  }> => {
    // Find all assistant messages in the feedback request
    const assistantMessages = feedbackRequest.messages.filter(
      (msg) => msg.role === 'assistant',
    ) as AssistantMessage[];

    if (assistantMessages.length > 1) {
      throw new Error('More than one assistant message in feedback request');
    }
    const assistantMessage = assistantMessages[0];
    if (!assistantMessage) {
      throw new Error('No assistant message in feedback request');
    }

    // Update each assistant message with feedback
    try {
      const pageId = await this.findPageBySessionAndResponseId(
        feedbackRequest.session_id,
        assistantMessage.message_id,
      );

      if (!pageId) {
        return { pageId: null };
      }

      // Update only the feedback properties
      await this.notion.pages.update({
        page_id: pageId,
        properties: {
          'Feedback Type': {
            select: {
              name: feedbackRequest.feedback_type,
              color:
                feedbackRequest.feedback_type === 'thumbs_up'
                  ? ('green' as const)
                  : ('red' as const),
            },
          },
          'Feedback Comment': feedbackRequest.comment
            ? {
                rich_text: [
                  {
                    text: {
                      content: feedbackRequest.comment,
                    },
                  },
                ],
              }
            : { rich_text: [] },
        },
      });

      return { pageId };
    } catch (error) {
      console.error(`Failed to update feedback for message ${assistantMessage.message_id}:`, error);
      return { pageId: null };
    }
  };

  /**
   * Convert a Notion page ID to a clickable URL
   * Format: https://www.notion.so/{workspace}/{database_id}?v={view_id}&p={page_id}&pm=s
   */
  toUrl = ({ pageId }: { pageId: string }): string => {
    const cleanDatabaseId = this.databaseId.replace(/-/g, '');
    const cleanPageId = pageId.replace(/-/g, '');

    // Use database ID as view ID (common pattern in Notion URLs)
    const viewId = cleanDatabaseId;

    return `https://www.notion.so/gloochat/${cleanDatabaseId}?v=${viewId}&p=${cleanPageId}&pm=s`;
  };
}

// CLAUDE: do not add convenience functions here, they should never be used.

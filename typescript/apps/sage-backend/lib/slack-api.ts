import type { SendFeedbackRequest } from '@baml/sage-interface';
import { WebClient } from '@slack/web-api';

export class SlackFeedbackLogger {
  private slack: WebClient;
  private channel = '#support-docs';

  constructor() {
    const token = process.env.SLACK_BOUNDARY_BOT_TOKEN;

    if (!token) {
      throw new Error('SLACK_BOUNDARY_BOT_TOKEN environment variable is required');
    }

    this.slack = new WebClient(token);
  }

  /**
   * Send feedback to Slack using blocks for better formatting
   */
  sendFeedback = async (request: SendFeedbackRequest & { notionLink?: string }): Promise<any> => {
    const blocks = [
      {
        type: 'section',
        fields: [
          {
            type: 'mrkdwn',
            text: `${request.feedback_type === 'thumbs_up' ? '👍' : '❌'}  from user: _${request.comment}_\n\n[Click here to see the full conversation](${request.notionLink})`,
          },
        ],
      },
      {
        type: 'rich_text',
        elements: [
          {
            type: 'rich_text_quote',
            elements: request.messages.map((msg) => {
              let text = '???';

              if (msg.role === 'user') {
                text = `🧑 ${msg.text || 'N/A'}`;
              } else if (msg.role === 'assistant') {
                text = `🐑 ${msg.text || 'N/A'}`;
              }

              return {
                type: 'text',
                text: `${text}\n\n`,
              };
            }),
          },
        ],
      },
    ];

    try {
      const result = await this.slack.chat.postMessage({
        channel: this.channel,
        blocks,
        text: '??? fallback text ???',
      });

      return result;
    } catch (error) {
      console.error('Failed to send feedback to Slack:', error);
      throw error;
    }
  };

  /**
   * Test the Slack connection
   */
  testConnection = async (): Promise<any> => {
    try {
      const authTest = await this.slack.auth.test();
      return {
        success: true,
        botUserId: authTest.user_id,
        team: authTest.team,
        botName: authTest.user,
      };
    } catch (error) {
      console.error('Failed to test Slack connection:', error);
      throw error;
    }
  };
}

// Export convenience functions for backward compatibility
export async function sendFeedbackToSlack(feedback: SendFeedbackRequest): Promise<any> {
  const logger = new SlackFeedbackLogger();
  return logger.sendFeedback(feedback);
}

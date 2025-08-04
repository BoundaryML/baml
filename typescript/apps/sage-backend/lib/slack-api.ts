import { WebClient } from '@slack/web-api';
import { type SendFeedbackRequest, type Message } from '@baml/sage-interface';

export class SlackFeedbackLogger {
  private slack: WebClient;
  private channel: string = '#support-docs';

  constructor() {
    const token = process.env.SLACK_BOUNDARY_BOT_TOKEN;

    if (!token) {
      throw new Error('SLACK_BOUNDARY_BOT_TOKEN environment variable is required');
    }

    this.slack = new WebClient(token);
  }

  /**
   * Helper method to extract the last user question and assistant response from messages
   */
  private extractLastQnA = (messages: Message[]): { question: string; answer: string } => {
    // Find the last user message and the assistant response that follows it
    let lastUserMessage = '';
    let lastAssistantMessage = '';

    for (let i = messages.length - 1; i >= 0; i--) {
      const message = messages[i];
      if (message.role === 'user' && !lastUserMessage) {
        lastUserMessage = message.text;
        // Look for the next assistant message after this user message
        for (let j = i + 1; j < messages.length; j++) {
          if (messages[j].role === 'assistant') {
            lastAssistantMessage = messages[j].text || '';
            break;
          }
        }
        break;
      }
    }

    return {
      question: lastUserMessage,
      answer: lastAssistantMessage
    };
  };

  /**
   * Helper method to format conversation for display
   */
  private formatConversation = (messages: Message[]): string => {
    return messages.map((msg) => {
      const roleEmoji = msg.role === 'user' ? '🧑' : '🤖';
      const text = msg.role === 'user' ? msg.text : (msg.text || '');
      return `${roleEmoji} *${msg.role}:* ${text}`;
    }).join('\n\n');
  };

  /**
   * Send feedback to Slack using blocks for better formatting
   */
  sendFeedback = async (feedback: SendFeedbackRequest): Promise<any> => {
    const { question, answer } = this.extractLastQnA(feedback.messages);
    const emoji = feedback.feedback_type === 'thumbs_up' ? '👍' : '👎';
    const feedbackText = feedback.feedback_type === 'thumbs_up' ? 'Positive' : 'Negative';

    const blocks = [
      {
        type: 'header',
        text: {
          type: 'plain_text',
          text: `${emoji} ${feedbackText} Feedback Received`
        }
      },
      {
        type: 'section',
        fields: [
          {
            type: 'mrkdwn',
            text: `*Question:*\n${question || 'N/A'}`
          },
          {
            type: 'mrkdwn',
            text: `*Timestamp:*\n${new Date().toLocaleString()}`
          }
        ]
      }
    ];

    // Add answer section if available
    if (answer) {
      blocks.push({
        type: 'section',
        text: {
          type: 'mrkdwn',
          text: `*Answer:*\n${answer}`
        }
      });
    }

    // Add comment if provided
    if (feedback.comment) {
      blocks.push({
        type: 'section',
        text: {
          type: 'mrkdwn',
          text: `*User Comment:*\n${feedback.comment}`
        }
      });
    }

    // Add session info
    blocks.push({
      type: 'section',
      fields: [
        {
          type: 'mrkdwn',
          text: `*Session ID:*\n${feedback.session_id}`
        },
        {
          type: 'mrkdwn',
          text: `*Feedback Type:*\n${feedback.feedback_type}`
        }
      ]
    });

    // Add conversation context as attachment if there are multiple messages
    const attachments = [];
    if (feedback.messages.length > 1) {
      const conversationText = this.formatConversation(feedback.messages);
      
      attachments.push({
        color: feedback.feedback_type === 'thumbs_up' ? '#36a64f' : '#ff0000',
        title: 'Full Conversation Context',
        text: conversationText,
        mrkdwn_in: ['text']
      });
    }

    try {
      const result = await this.slack.chat.postMessage({
        channel: this.channel,
        blocks,
        attachments: attachments.length > 0 ? attachments : undefined,
        text: `${feedbackText} feedback received: ${question || 'Session ' + feedback.session_id}` // Fallback text
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
        botName: authTest.user
      };
    } catch (error) {
      console.error('Failed to test Slack connection:', error);
      throw error;
    }
  };

  /**
   * Send a test feedback message
   */
  sendTestFeedback = async (): Promise<any> => {
    const testFeedback: SendFeedbackRequest = {
      session_id: 'test_session_' + Date.now(),
      feedback_type: 'thumbs_up',
      comment: 'This is a test feedback message from the Slack integration.',
      messages: [
        {
          role: 'user',
          text: 'How do I use BAML with TypeScript?'
        },
        {
          role: 'assistant',
          text: 'You can use BAML with TypeScript by installing the @baml/typescript package and following the quickstart guide.',
          ranked_docs: [
            {
              title: 'BAML TypeScript Quickstart',
              url: 'https://docs.boundaryml.com/guide/typescript',
              relevance: 'very-relevant'
            }
          ]
        }
      ]
    };

    return this.sendFeedback(testFeedback);
  };
}

// Export convenience functions for backward compatibility
export async function sendFeedbackToSlack(
  feedback: SendFeedbackRequest,
): Promise<any> {
  const logger = new SlackFeedbackLogger();
  return logger.sendFeedback(feedback);
}

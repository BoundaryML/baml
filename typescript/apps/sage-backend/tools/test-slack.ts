/**
 * Test script to send a feedback message to Slack
 * 
 * Usage:
 * 1. Make sure you have the environment variables set:
 *    SLACK_BOUNDARY_BOT_APP_ID=your_app_id
 *    SLACK_BOUNDARY_BOT_CLIENT_ID=your_client_id  
 *    SLACK_BOUNDARY_BOT_CLIENT_SECRET=your_client_secret
 *    SLACK_BOUNDARY_BOT_TOKEN=xoxb-your-bot-token (if using bot token directly)
 * 
 * 2. Run the script:
 *    pnpm tsx tools/test-slack.ts
 */

import { WebClient } from '@slack/web-api';

// Initialize Slack client
const slack = new WebClient(process.env.SLACK_BOUNDARY_BOT_TOKEN);

interface FeedbackData {
  question: string;
  feedbackType: 'thumbs_up' | 'thumbs_down';
  answer: string;
  comment?: string;
  userEmail?: string;
  sessionId?: string;
  conversation: Array<{ role: 'user' | 'assistant'; text: string }>;
}

/**
 * Format feedback data into a Slack message
 */
function formatFeedbackMessage(data: FeedbackData): string {
  const emoji = data.feedbackType === 'thumbs_up' ? '👍' : '👎';
  const feedbackText = data.feedbackType === 'thumbs_up' ? 'Positive' : 'Negative';
  
  let message = `${emoji} *${feedbackText} Feedback Received*\n\n`;
  
  message += `*Question:*\n${data.question}\n\n`;
  message += `*Answer:*\n${data.answer}\n\n`;
  
  if (data.comment) {
    message += `*User Comment:*\n${data.comment}\n\n`;
  }
  
  if (data.userEmail) {
    message += `*User:* ${data.userEmail}\n`;
  }
  
  if (data.sessionId) {
    message += `*Session:* ${data.sessionId}\n`;
  }
  
  message += `*Timestamp:* ${new Date().toISOString()}\n\n`;
  
  // Add conversation context if more than just the Q&A
  if (data.conversation.length > 2) {
    message += `*Full Conversation:*\n`;
    data.conversation.forEach((msg, index) => {
      const roleEmoji = msg.role === 'user' ? '🧑' : '🤖';
      message += `${roleEmoji} *${msg.role}:* ${msg.text}\n`;
    });
  }
  
  return message;
}

/**
 * Send feedback to Slack using blocks for better formatting
 */
async function sendFeedbackToSlack(data: FeedbackData, channel: string = '#support-docs') {
  const emoji = data.feedbackType === 'thumbs_up' ? '👍' : '👎';
  const feedbackText = data.feedbackType === 'thumbs_up' ? 'Positive' : 'Negative';
  const color = data.feedbackType === 'thumbs_up' ? 'good' : 'danger';
  
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
          text: `*Question:*\n${data.question}`
        },
        {
          type: 'mrkdwn',
          text: `*Timestamp:*\n${new Date().toLocaleString()}`
        }
      ]
    },
    {
      type: 'section',
      text: {
        type: 'mrkdwn',
        text: `*Answer:*\n${data.answer}`
      }
    }
  ];
  
  // Add comment if provided
  if (data.comment) {
    blocks.push({
      type: 'section',
      text: {
        type: 'mrkdwn',
        text: `*User Comment:*\n${data.comment}`
      }
    });
  }
  
  // Add user info if available
  if (data.userEmail || data.sessionId) {
    const fields = [];
    if (data.userEmail) {
      fields.push({
        type: 'mrkdwn',
        text: `*User:*\n${data.userEmail}`
      });
    }
    if (data.sessionId) {
      fields.push({
        type: 'mrkdwn',
        text: `*Session:*\n${data.sessionId}`
      });
    }
    
    blocks.push({
      type: 'section',
      fields
    });
  }
  
  // Add conversation context as attachment if lengthy
  const attachments = [];
  if (data.conversation.length > 2) {
    let conversationText = '';
    data.conversation.forEach((msg) => {
      const roleEmoji = msg.role === 'user' ? '🧑' : '🤖';
      conversationText += `${roleEmoji} *${msg.role}:* ${msg.text}\n\n`;
    });
    
    attachments.push({
      color: '#36a64f',
      title: 'Full Conversation Context',
      text: conversationText,
      mrkdwn_in: ['text']
    });
  }
  
  const result = await slack.chat.postMessage({
    channel,
    blocks,
    attachments: attachments.length > 0 ? attachments : undefined,
    text: `${feedbackText} feedback received: ${data.question}` // Fallback text
  });
  
  return result;
}

async function testSlackIntegration() {
  try {
    console.log('Testing Slack integration...');
    console.log('Bot Token:', process.env.SLACK_BOUNDARY_BOT_TOKEN ? 'Set' : 'Missing');
    console.log('App ID:', process.env.SLACK_BOUNDARY_BOT_APP_ID || 'Missing');
    console.log('Client ID:', process.env.SLACK_BOUNDARY_BOT_CLIENT_ID || 'Missing');
    console.log('Client Secret:', process.env.SLACK_BOUNDARY_BOT_CLIENT_SECRET ? 'Set' : 'Missing');
    
    if (!process.env.SLACK_BOUNDARY_BOT_TOKEN) {
      throw new Error('SLACK_BOUNDARY_BOT_TOKEN environment variable is required');
    }
    
    // Test authentication first
    console.log('\n1. Testing authentication...');
    const authTest = await slack.auth.test();
    console.log('✅ Authentication successful!');
    console.log(`   Bot User ID: ${authTest.user_id}`);
    console.log(`   Team: ${authTest.team}`);
    console.log(`   Bot Name: ${authTest.user}`);
    
    // Test data - simulating feedback submission
    const testData: FeedbackData = {
      question: 'How do I use BAML with TypeScript?',
      feedbackType: 'thumbs_up',
      answer: 'You can use BAML with TypeScript by installing the @baml/typescript package and following the quickstart guide. This includes setting up your BAML files, generating TypeScript types, and calling BAML functions from your code.',
      comment: 'This answer was very helpful! The step-by-step guide made it easy to get started.',
      userEmail: 'test@example.com',
      sessionId: 'session_123456',
      conversation: [
        { role: 'user', text: 'How do I use BAML with TypeScript?' },
        { role: 'assistant', text: 'You can use BAML with TypeScript by installing the @baml/typescript package and following the quickstart guide. This includes setting up your BAML files, generating TypeScript types, and calling BAML functions from your code.' },
        { role: 'user', text: 'Can you show me an example?' },
        { role: 'assistant', text: 'Sure! Here\'s a basic example: First install with `npm install @baml/typescript`, then create a BAML file with your function definitions, and finally import and use the generated types in your TypeScript code.' }
      ]
    };
    
    console.log('\n2. Sending test feedback message...');
    const result = await sendFeedbackToSlack(testData, '#support-docs');
    
    console.log('✅ Successfully sent feedback to Slack!');
    console.log(`   Message Timestamp: ${result.ts}`);
    console.log(`   Channel: ${result.channel}`);
    if (result.message?.permalink) {
      console.log(`   Permalink: ${result.message.permalink}`);
    }
    
    // Test negative feedback too
    console.log('\n3. Sending negative feedback test...');
    const negativeTestData: FeedbackData = {
      ...testData,
      feedbackType: 'thumbs_down',
      comment: 'The answer was too generic and didn\'t help with my specific use case.',
    };
    
    const negativeResult = await sendFeedbackToSlack(negativeTestData, '#support-docs');
    console.log('✅ Successfully sent negative feedback to Slack!');
    console.log(`   Message Timestamp: ${negativeResult.ts}`);
    
  } catch (error: any) {
    console.error('❌ Error testing Slack integration:');
    console.error('Error code:', error.code);
    console.error('Error message:', error.message);
    
    if (error.code === 'not_authed') {
      console.error('\n💡 Troubleshooting:');
      console.error('1. Check that SLACK_BOUNDARY_BOT_TOKEN is correct and starts with "xoxb-"');
      console.error('2. Make sure the bot is installed in your workspace');
      console.error('3. Verify the bot has the required scopes (chat:write, chat:write.public)');
    }
    
    if (error.code === 'channel_not_found') {
      console.error('\n💡 Troubleshooting:');
      console.error('1. Make sure the #general channel exists');
      console.error('2. Invite the bot to the channel first');
      console.error('3. Or try using a channel ID instead of name');
    }
    
    if (error.code === 'missing_scope') {
      console.error('\n💡 Troubleshooting:');
      console.error('1. Add required OAuth scopes to your Slack app:');
      console.error('   - chat:write');
      console.error('   - chat:write.public');
      console.error('2. Reinstall the app to your workspace');
    }
    
    process.exit(1);
  }
}

// Run the test
testSlackIntegration();
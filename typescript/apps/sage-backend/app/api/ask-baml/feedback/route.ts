import { NotionLogger } from '@/lib/notion-api';
import { SlackFeedbackLogger } from '@/lib/slack-api';
import { SendFeedbackRequestSchema } from '@baml/sage-interface';
import type { NextRequest } from 'next/server';
import { after, NextResponse } from 'next/server';

const slack = new SlackFeedbackLogger();
const notionLogger = new NotionLogger();

export async function POST(request: NextRequest) {
  try {
    const body = await request.json();

    const reqBody = SendFeedbackRequestSchema.safeParse(body);

    if (!reqBody.success) {
      return NextResponse.json(
        {
          error: 'Request does not match expected schema',
          details: reqBody.error,
          expectedSchema: SendFeedbackRequestSchema.toString(),
        },
        { status: 400 },
      );
    }

    const feedbackData = reqBody.data;

    // Deliberately do not await these, so that the request can return immediately.
    after(async () => {
      console.info('Feedback will be logged to Notion and Slack');
      const { pageId: notionPageId } = await notionLogger.updateFeedback(feedbackData);
      const notionLink = notionPageId ? notionLogger.toUrl({ pageId: notionPageId }) : undefined;
      await slack.sendFeedback({ ...feedbackData, notionLink });
      console.info('Feedback logged to Notion and Slack');
    });

    return NextResponse.json({
      enqueued: true,
      message: 'Feedback received',
    });
  } catch (error) {
    console.error('Error in send-feedback API:', error);
    return NextResponse.json(
      {
        enqueued: false,
        error: 'Internal server error',
        message: error instanceof Error ? error.message : 'Unknown error',
      },
      { status: 500 },
    );
  }
}
